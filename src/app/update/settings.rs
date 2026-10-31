//! Settings-related update handlers, extracted from `app/update.rs`.
//!
//! Covers `Message::Settings(msg)` — the entire settings overlay dispatch.
//! The dead `{ let s = format!("{:?}", msg); ... }` debug block at the top
//! of this handler is left verbatim; removing it is a separate cleanup.

#![allow(clippy::collapsible_match)]

use super::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_settings(&mut self, msg: SettingsMessage) -> Command<Message> {

            {
                let s = format!("{:?}", msg);
                let _truncated = if s.len() > 120 {
                    format!("{}…", &s[..120])
                } else {
                    s
                };
                let _ts = self.settings.tooltip_sizes.len();
                let _bm = &self.settings.bar_monitor;
            }

            if let SettingsMessage::TestProfileSound(idx) = msg {
                if let Some(p) = self.settings.friend_profiles.get(idx) {
                    if let Some(path) = &p.sound_path {
                        moon_beauty_shell::overlays::notifications::NotificationOverlay::play_sound(
                            path.clone(),
                            self.settings.notif_volume,
                        );
                    }
                }
                return Command::none();
            }

            if let SettingsMessage::TestProfileTts(idx) = msg {
                if let Some(p) = self.settings.friend_profiles.get(idx) {
                    moon_beauty_shell::overlays::notifications::NotificationOverlay::speak_tts(
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
