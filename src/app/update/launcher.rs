use super::*;
use moonbeauty::overlays::launcher::LauncherMessage;

impl MoonBeautyShellApp {
    pub(crate) fn handle_launcher(&mut self, msg: LauncherMessage) -> Command<Message> {
        match msg {
            LauncherMessage::EnterPressed => {
                if self.launcher.search_query.trim().to_lowercase() == "music" {
                    if self.launcher.music_focus_idx == 0 {
                        return self.handle_launcher(LauncherMessage::MusicPlaySelected);
                    } else {
                        return self.handle_launcher(LauncherMessage::MusicTriggerAction);
                    }
                } else if let Some(selected) = self.launcher.filtered_apps.get(self.launcher.selected_index) {
                    return self.handle_launcher(LauncherMessage::ExecuteApp(selected.exec.clone()));
                }
                return Command::none();
            }
            LauncherMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            LauncherMessage::ExecuteApp(cmd) => {
                // __tab__ branch removed (problem 10): nothing emits
                // ExecuteApp("__tab__").
                // Math result — copy to clipboard and close
                if let Some(result) = cmd.strip_prefix("math:") {
                    let result = result.to_string();
                    eprintln!("[launcher] math result -> wl-copy: {}", result);
                    let _ = std::process::Command::new("notify-send")
                        .args(["-a", "Launcher", "Copied", &result])
                        .spawn();
                    if self.is_standalone {
                        let r = result.clone();
                        crate::app::helpers::run_then_exit(async move {
                            let _ = tokio::process::Command::new("wl-copy")
                                .arg(&r)
                                .status()
                                .await;
                        });
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move {
                            let _ = tokio::process::Command::new("wl-copy")
                                .arg(&result)
                                .status()
                                .await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }

                if let Some(definition) = cmd.strip_prefix("dict:") {
                    // Ignore sentinel entries (pending/hint)
                    if definition == "__pending__" || definition == "__hint__" {
                        return Command::none();
                    }
                    let def = definition.to_string();
                    eprintln!(
                        "[launcher] dict copy: {}...",
                        def.chars().take(40).collect::<String>()
                    );
                    let _ = std::process::Command::new("wl-copy").arg(&def).spawn();
                    let _ = std::process::Command::new("notify-send")
                        .args(["-a", "Dictionary", "Copied", &def])
                        .spawn();
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::none();
                }

                if let Some(key) = cmd.strip_prefix("pass:") {
                    eprintln!("[launcher] pass copy: {}", key);
                    // Detached: `pass -c` may block on GPG pinentry, so we
                    // spawn it as its own process and exit immediately.
                    // The `sh -c '... "$1"' -- <key>` pattern prevents shell
                    // injection when the key has special chars.
                    let _ = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(
                            r#"pass -c "$1" 2>&1 && notify-send -a 'Password Store' 'Copied' "$1""#,
                        )
                        .arg("--")
                        .arg(key)
                        .spawn();
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::none();
                }

                if let Some(target_str) = cmd.strip_prefix("internal:") {
                    if let Some(target) = OverlayTarget::from_str(target_str) {
                        if self.is_standalone {
                            self.spawn_standalone(target);
                            std::process::exit(0);
                        } else {
                            self.active_overlay = Some(target);
                            return Command::none();
                        }
                    }
                }
                if self.is_standalone {
                    tokio::spawn(async move {
                        AppLauncher::launch(&cmd).await;
                    });
                    std::process::exit(0);
                }
                self.active_overlay = None;
                return Command::perform(
                    async move {
                        AppLauncher::launch(&cmd).await;
                    },
                    |_| Message::DismissActiveOverlay,
                );
            }
            LauncherMessage::SelectNext
            | LauncherMessage::SelectPrev
            | LauncherMessage::SelectDown
            | LauncherMessage::SelectUp => {
                let cmd = self.launcher.update(msg);

                // If we are in music mode, the launcher update already returns a scroll task for "music_scroll"
                if self.launcher.search_query.trim().to_lowercase() == "music" {
                    return cmd.map(Message::Launcher);
                }

                let idx = self.launcher.selected_index;
                let math_count = self
                    .launcher
                    .filtered_apps
                    .iter()
                    .filter(|a| a.exec.starts_with("math:"))
                    .count();

                let y = if idx < math_count {
                    let row = idx / 3;
                    (row as f32) * (76.0 + 6.0)
                } else {
                    let math_rows = (math_count + 2) / 3;
                    let math_height = if math_rows > 0 {
                        (math_rows as f32) * 76.0 + ((math_rows - 1) as f32) * 6.0 + 10.0
                    } else {
                        0.0
                    };

                    let app_idx = idx - math_count;
                    let icon_size = self.settings.launcher_icon_size as f32;
                    let row_h = icon_size * 1.4 + 20.0;
                    math_height + (app_idx as f32) * (row_h + 12.0)
                };

                let scroll_y = (y - 150.0).max(0.0);

                eprintln!("[launcher-handler] scroll_to y={}", scroll_y);

                let scroll_cmd = iced::widget::operation::scroll_to(
                    iced::widget::Id::new("launcher_scroll"),
                    iced::widget::scrollable::AbsoluteOffset {
                        x: 0.0,
                        y: scroll_y,
                    },
                );

                return Command::batch([cmd.map(Message::Launcher), scroll_cmd]);
            }
            _ => return self.launcher.update(msg).map(Message::Launcher),
        }

        Command::none()
    }
}
