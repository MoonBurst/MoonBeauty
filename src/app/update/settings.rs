//! Settings-related update handlers, extracted from `app/update.rs`.
//!
//! Covers `Message::Settings(msg)` — the entire settings overlay dispatch.
//! The dead `{ let s = format!("{:?}", msg); ... }` debug block at the top
//! of this handler is left verbatim; removing it is a separate cleanup.

#![allow(clippy::collapsible_match)]

use super::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_settings(&mut self, msg: SettingsMessage) -> Command<Message> {

            if let SettingsMessage::TestProfileSound(idx) = msg {
                if let Some(p) = self.settings.friend_profiles.get(idx) {
                    if let Some(path) = &p.sound_path {
                        moonbeauty::overlays::notifications::NotificationOverlay::play_sound(
                            path.clone(),
                            self.settings.notif_volume,
                        );
                    }
                }
                return Command::none();
            }

            if let SettingsMessage::TestProfileTts(idx) = msg {
                if let Some(p) = self.settings.friend_profiles.get(idx) {
                    moonbeauty::overlays::notifications::NotificationOverlay::speak_tts(
                        "Diagnostics",
                        "Test",
                        &p.summary,
                        p.default_readout.as_deref(),
                        self.settings.tts_command.clone(),
                        self.settings.notif_volume,
                    );
                }
                return Command::none();
            }

            // Copy command to clipboard. Handled here (not in
            // settings_ui.update) because it returns a Command;
            // settings_ui.update is sync.
            if let SettingsMessage::CopyIpc(cmd) = &msg {
                let cmd = cmd.clone();
                eprintln!("[settings] copied to clipboard: {}", cmd);
                return iced::clipboard::write(cmd);
            }

            match msg {
                SettingsMessage::PickAvatar(idx) => {
                    if let Some(p) = self.settings.friend_profiles.get(idx) {
                        let name = p.name.clone();
                        return Command::perform(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                                .pick_file()
                                .await;
                            
                            if let Some(f) = file {
                                let src_path = f.path().to_path_buf();
                                let name_clone = name.clone();
                                
                                // Offload to blocking pool
                                let _ = tokio::task::spawn_blocking(move || {
                                    let av_dir = dirs::data_dir()
                                        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                                        .join("moonbeauty").join("avatars").join(&name_clone);
                                    let _ = std::fs::create_dir_all(&av_dir);
                                    let dest_path = av_dir.join(format!("{}.png", name_clone));

                                    let status = std::process::Command::new("magick")
                                        .arg(&src_path)
                                        .args(["-trim", "+repage", "-resize", "256x256>", "-background", "none", "-gravity", "center", "-extent", "256x256"])
                                        .arg(&dest_path)
                                        .status();
                                        
                                    if let Err(e) = status {
                                        eprintln!("[avatar-picker] magick failed: {e}");
                                    }
                                }).await;
                            }
                            
                            // Send SIGUSR1 to gracefully restart/clear cache
                            let _ = std::process::Command::new("pkill")
                                .args(["-SIGUSR1", "-f", "(^|/)moonbeauty --notif-surface$"])
                                .status();
                                
                        }, |_| Message::Settings(SettingsMessage::AvatarUpdated));
                    }
                    return Command::none();
                }
                SettingsMessage::AvatarUpdated => {
                    return Command::none();
                }
                _ => {}
            }

            let is_test = matches!(msg, SettingsMessage::TestNotification);

            if is_test {
                for (title, body) in [
                    (
                        "System Diagnostics #1 (first)",
                        "Verification toast successfully routed.",
                    ),
                    (
                        "System Diagnostics #2 (second)",
                        "Background worker service nominal.",
                    ),
                    (
                        "System Diagnostics #3 (third)",
                        "Telemetry stream established.",
                    ),
                ] {
                    let _ = std::process::Command::new("notify-send")
                        .args(["-a", "Diagnostics", title, body])
                        .spawn();
                }
            } else if matches!(msg, SettingsMessage::Dismiss) {
                // Persist before closing / exiting the settings process.
                let _ = self.settings.save();
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            } else {
                self.settings_ui.update(msg, &mut self.settings);
                self.theme = ShellTheme::from_settings(&self.settings);

                if self.is_standalone {
                    tokio::spawn(async {
                        let _ = send_client_command(IpcCommand::ReloadSettings).await;
                    });
                }
            }
        
        Command::none()
    }
}
