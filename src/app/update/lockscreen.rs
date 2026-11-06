//! Lockscreen-related update handlers, extracted from `app/update.rs`.
//!
//! Covers:
//!   - `Message::Lock(msg)` — LockMessage dispatch (KeyPress, PollIpc)
//!   - `start_lockscreen_service` — trigger the systemd-managed
//!     lockscreen coordinator (moonbeauty-lockscreen.service)
//!
//! Crash recovery for the coordinator is handled by systemd
//! (Restart=on-failure + RestartSec=500ms on the unit), not by a
//! bar-side watchdog. Auth success exits the coordinator with code 0,
//! which Restart=on-failure correctly does NOT restart.

#![allow(clippy::collapsible_match)]

use super::*;

use moonbeauty::overlays::lockscreen::LockMessage;

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

    /// Start the systemd-managed lockscreen coordinator. Idempotent:
    /// `systemctl --user start` is a no-op when already active. Never
    /// stop from here — unlocking requires password auth handled by
    /// the coordinator; the only exit path is the coordinator's own
    /// SIGTERM handler after successful auth (exit 0, which systemd's
    /// Restart=on-failure honors).
    ///
    /// Non-blocking: `.spawn()` forks systemctl without waiting for
    /// the unit to be up. The coordinator becomes visible within
    /// ~200-500ms; the UI does not stall in the meantime.
    pub(crate) fn start_lockscreen_service(&self) -> Command<Message> {
        eprintln!("[ipc] lockscreen: systemctl --user start moonbeauty-lockscreen.service");
        match std::process::Command::new("systemctl")
            .args(["--user", "start", "moonbeauty-lockscreen.service"])
            .spawn()
        {
            Ok(_) => {}
            Err(e) => eprintln!("[ipc] systemctl spawn failed: {e}"),
        }
        Command::none()
    }
}
