//! Lockscreen-related update handlers, extracted from `app/update.rs`.
//!
//! Covers:
//!   - `Message::Lock(msg)` — LockMessage dispatch (KeyPress, PollIpc)
//!   - `Message::CheckLockscreenAlive` — 500ms watchdog that respawns the
//!     lock child after a crash or suspend
//!
//! **Security-critical.**  The watchdog is what keeps the screen locked
//! when a lock child dies.  The 2s suspend-detection threshold below is
//! deliberate and MUST NOT be lowered — see docs/REFACTOR.md.

#![allow(clippy::collapsible_match)]

use super::*;

use moon_beauty_shell::overlays::lockscreen::LockMessage;

impl MoonBeautyShellApp {
    pub(crate) fn handle_lock(&mut self, msg: LockMessage) -> Command<Message> {
        match msg {
            LockMessage::KeyPress(key) => {
                self.lockscreen.forward_key(&key);
                Command::none()
            }
            LockMessage::PollIpc => {
                if let Some(effect) = self.lockscreen.drain_ipc() {
                    if let LockMessage::AuthSuccess = effect {
                        eprintln!("[lock] coordinator signaled success — exiting");
                        std::process::exit(0);
                    }
                }
                Command::none()
            }
            _ => Command::none(),
        }
    }

    /// 500ms watchdog: respawn the lockscreen child if it has crashed,
    /// and force-respawn after suspend/resume (detected via elapsed-time
    /// gap between ticks).
    pub(crate) fn check_lockscreen_alive(&mut self) -> Command<Message> {
        // Only the primary bar watches the lock.
        if self.is_notif_surface || self.is_standalone {
            return Command::none();
        }

        // Pause detection: session was suspended (TTY switch, sleep).
        // Layer surfaces held by lock children are dropped by sway
        // on output detach WITHOUT sending Closed to the client —
        // from the child's perspective nothing happened, but its
        // wl_surface is no longer composited and keyboard grab is
        // gone.  Detect via elapsed-time gap and force a full respawn.
        let now = std::time::Instant::now();
        let since_last = now.saturating_duration_since(self.last_lockscreen_check);
        self.last_lockscreen_check = now;

        let was_lockscreen = matches!(
            self.standalone_child.as_ref().map(|(t, _)| t),
            Some(OverlayTarget::Lockscreen)
        );
        if !was_lockscreen {
            return Command::none();
        }

        if since_last > std::time::Duration::from_millis(2000) {
            eprintln!(
                "[lockscreen-watchdog] session was paused ({:?}); forcing lock respawn",
                since_last
            );
            self.kill_standalone();
            self.spawn_standalone(OverlayTarget::Lockscreen);
            return Command::none();
        }

        // Exit status discrimination:
        //   success() == true  → coordinator exited cleanly.  The only
        //     code path that does this is auth success: coordinator
        //     broadcasts Success, kills children, exits(0).  Respawning
        //     here would immediately re-lock the screen — that's the
        //     "locks as soon as I unlock it" bug.
        //   success() == false → crash (nonzero exit or signal kill).
        //     Respawn to keep the screen locked.
        match self.standalone_child.as_mut() {
            Some((_, child)) => match child.try_wait() {
                Ok(None) => Command::none(),
                Ok(Some(status)) => {
                    if status.success() {
                        eprintln!(
                            "[lockscreen-watchdog] coordinator exited cleanly (auth success), not respawning"
                        );
                        self.standalone_child = None;
                        return Command::none();
                    }
                    eprintln!(
                        "[lockscreen-watchdog] coordinator crashed (code={:?}), respawning",
                        status.code()
                    );
                    self.standalone_child = None;
                    self.spawn_standalone(OverlayTarget::Lockscreen);
                    Command::none()
                }
                Err(e) => {
                    eprintln!("[lockscreen-watchdog] try_wait error ({e}), respawning");
                    self.standalone_child = None;
                    self.spawn_standalone(OverlayTarget::Lockscreen);
                    Command::none()
                }
            },
            None => Command::none(),
        }
    }
}
