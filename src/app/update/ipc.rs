use super::*;
use moonbeauty::libraries::ipc::IpcCommand;

impl MoonBeautyShellApp {
    pub(crate) fn handle_ipc(&mut self, cmd: IpcCommand) -> Command<Message> {
        let _ = match cmd {
            IpcCommand::Toggle(target) => {
                // Every overlay runs as a standalone child so it gets
                // keyboard=Exclusive from its layer surface. The bar's own
                // layer is KeyboardInteractivity::None, so in-bar overlays
                // cannot receive keystrokes.
                eprintln!("[ipc] toggle {:?} (standalone)", target);

                // SECURITY: the lockscreen must NEVER be unlockable via
                // IPC. Before this guard, two rapid Toggle(Lockscreen)
                // calls spawn-then-kill (200ms+ desktop-visible window),
                // and any local script could dismiss the lock by sending
                // two toggles. Lockscreen is now idempotent-spawn: IPC
                // can spawn it, but only password auth removes it.
                if matches!(target, OverlayTarget::Lockscreen) {
                    return self.start_lockscreen_service();
                } else {
                    self.toggle_standalone(target);
                }
            }
            IpcCommand::Open(target, arg) => {
                if matches!(
                    target,
                    OverlayTarget::Launcher
                        | OverlayTarget::Clipboard
                        | OverlayTarget::NotificationsHistory
                        | OverlayTarget::Settings
                        | OverlayTarget::Todo
                        | OverlayTarget::Notes
                        | OverlayTarget::Unicode
                ) {
                    self.spawn_standalone(target);
                } else if matches!(target, OverlayTarget::Lockscreen) {
                    return self.start_lockscreen_service();
                } else if matches!(target, OverlayTarget::Annotator) {
                    // Annotator needs fullscreen + exclusive keyboard, which only
                    // the standalone (--annotator-session) process gets via its
                    // LayerShellSettings. Spawn a child with the captured path.
                    let exe = resolve_exe();
                    let mut cmd = std::process::Command::new(exe);
                    cmd.arg("--annotator-session");
                    if let Some(ref p) = arg {
                        cmd.env("MBS_ANNOTATOR_PATH", p);
                    }
                    match cmd.spawn() {
                        Ok(child) => {
                            eprintln!("[annotator] spawned pid={}", child.id());
                            self.standalone_child = Some((OverlayTarget::Annotator, child));
                        }
                        Err(e) => eprintln!("[annotator] spawn failed: {e}"),
                    }
                } else {
                    self.active_overlay = Some(target);
                }
            }
            IpcCommand::Close(target) => {
                // SECURITY: the lockscreen cannot be closed via IPC.
                // Unlocking requires password auth handled by the
                // coordinator; the only exit path is the coordinator's
                // own SIGTERM handler after successful auth (exit 0,
                // which systemd's Restart=on-failure honors).
                if matches!(target, OverlayTarget::Lockscreen) {
                    eprintln!("[ipc] Close(Lockscreen) refused — requires auth via coordinator");
                } else if self.active_overlay == Some(target) {
                    self.active_overlay = None;
                }
            }
            IpcCommand::DismissLatestNotif => return self.dismiss_latest_notif(),
            IpcCommand::JumpToLatestNotif => return self.jump_to_latest_notif(),
            IpcCommand::ToggleHistory => return self.toggle_history(),
            IpcCommand::ReloadSettings => {
                self.settings = ShellSettings::load();
                self.theme = ShellTheme::from_settings(&self.settings);
            }
            IpcCommand::TestNotification => return self.test_notification(),
            IpcCommand::MusicCycle => return self.music_cycle(),
            IpcCommand::MusicNext => return self.music_next_track_ipc(),
            IpcCommand::MusicPrevious => return self.music_previous_track_ipc(),
            IpcCommand::MusicPlayPause => return self.music_toggle_play_pause_ipc(),
        };
        Command::none()
    }
}
