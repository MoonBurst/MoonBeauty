use super::*;
use crate::app::keys::{handle_annotator_text_input, handle_email_keypress, handle_lock_keypress};

pub(crate) static IPC_RX: std::sync::OnceLock<
    std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<IpcCommand>>>,
> = std::sync::OnceLock::new();

fn ipc_stream() -> impl futures_util::Stream<Item = Message> {
    let rx = IPC_RX.get().expect("IPC_RX not initialized").clone();
    futures_util::stream::unfold(rx, |rx| async move {
        let mut guard = rx.lock().await;
        let cmd = guard.recv().await;
        drop(guard);
        cmd.map(|c| (Message::IpcReceived(c), rx))
    })
}

impl MoonBeautyShellApp {
    pub fn subscription(&self) -> iced::Subscription<Message> {
        let is_bar = !self.is_standalone && !self.is_notif_surface;
        let toast_sub = if self.is_notif_surface {
            let interval = if self.notifs.active_toasts.is_empty() {
                1000
            } else {
                16
            };
            iced::time::every(std::time::Duration::from_millis(interval))
                .map(|_| Message::TickToasts)
        } else {
            iced::Subscription::none()
        };

        let email_sync_sub = if is_bar
            && moonbeauty::libraries::backend::email::client::config_exists()
        {
            iced::time::every(std::time::Duration::from_secs(60)).map(|_| Message::TickEmailSync)
        } else {
            iced::Subscription::none()
        };

        let email_sub = if matches!(self.active_overlay, Some(OverlayTarget::Email)) {
            iced::event::listen_with(handle_email_keypress)
        } else {
            iced::Subscription::none()
        };

        let weather_sub = if is_bar {
            iced::time::every(std::time::Duration::from_secs(900)).map(|_| Message::TickWeather)
        } else {
            iced::Subscription::none()
        };

        let clip_sub = if matches!(self.active_overlay, Some(OverlayTarget::Clipboard)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(Message::Clipboard(ClipboardMessage::SelectPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(Message::Clipboard(ClipboardMessage::SelectNext))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                        Some(Message::Clipboard(ClipboardMessage::CopySelected))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => {
                        Some(Message::Clipboard(ClipboardMessage::DeleteSelected))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => {
                        Some(Message::Clipboard(ClipboardMessage::DeleteSelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let text_typing_sub = if self.annotator.text_editing.is_some() {
            iced::event::listen_with(handle_annotator_text_input)
        } else {
            iced::Subscription::none()
        };

        let todo_sub = if matches!(self.active_overlay, Some(OverlayTarget::Todo)) {
            iced::event::listen_with(|event, status, _window| {
                // No status gate: text_input returns Captured for Delete/arrows
                // even when empty. Handlers check new_task_text.is_empty() instead.
                if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: ref dbg_key,
                    ..
                }) = event
                {
                    moonbeauty::dbg_log!("[todo-sub] key={:?} status={:?}", dbg_key, status);
                }
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                use iced::keyboard::{key::Named, Key};
                match key {
                    Key::Named(Named::ArrowLeft) => Some(Message::Todo(TodoMessage::CategoryPrev)),
                    Key::Named(Named::ArrowRight) => Some(Message::Todo(TodoMessage::CategoryNext)),
                    Key::Named(Named::ArrowUp) => Some(Message::Todo(TodoMessage::TaskSelectPrev)),
                    Key::Named(Named::ArrowDown) => {
                        Some(Message::Todo(TodoMessage::TaskSelectNext))
                    }
                    Key::Named(Named::Delete) => Some(Message::Todo(TodoMessage::DeleteSelected)),
                    // Enter is handled by text_input::on_submit -> AddTask.
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let reminders_tick_sub = if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
            iced::time::every(std::time::Duration::from_millis(1000))
                .map(|_| Message::Reminders(ReminderMessage::Tick))
        } else {
            iced::Subscription::none()
        };

        let reminders_sub = if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) = event
                else {
                    return None;
                };
                use iced::keyboard::{key::Named, Key};
                match key {
                    // Tab / Shift+Tab cycle between the two input fields
                    Key::Named(Named::Tab) if modifiers.shift() => {
                        Some(Message::Reminders(ReminderMessage::FocusPrevField))
                    }
                    Key::Named(Named::Tab) => {
                        Some(Message::Reminders(ReminderMessage::FocusNextField))
                    }
                    // ArrowUp/Down scroll the list regardless of input focus
                    Key::Named(Named::ArrowUp) => {
                        Some(Message::Reminders(ReminderMessage::ScrollUp))
                    }
                    Key::Named(Named::ArrowDown) => {
                        Some(Message::Reminders(ReminderMessage::ScrollDown))
                    }
                    Key::Named(Named::Delete) => {
                        Some(Message::Reminders(ReminderMessage::DeleteSelected))
                    }
                    Key::Named(Named::Home) => {
                        Some(Message::Reminders(ReminderMessage::SelectFirst))
                    }
                    Key::Named(Named::End) => Some(Message::Reminders(ReminderMessage::SelectLast)),
                    Key::Character(c) if modifiers.control() && c.to_lowercase() == "s" => {
                        Some(Message::Reminders(ReminderMessage::ToggleSoundSelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let rng_sub = if matches!(self.active_overlay, Some(OverlayTarget::Rng)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) = event
                else {
                    return None;
                };
                moonbeauty::overlays::rng::resolve_rng_key(&key, modifiers).map(Message::Rng)
            })
        } else {
            iced::Subscription::none()
        };

        let notes_sub = if matches!(self.active_overlay, Some(OverlayTarget::Notes)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                eprintln!("[notes-sub] key={:?}", key);
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(Message::Notes(NotesMessage::SelectNext))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(Message::Notes(NotesMessage::SelectPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => {
                        Some(Message::Notes(NotesMessage::DeleteSelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let unicode_sub = if matches!(self.active_overlay, Some(OverlayTarget::Unicode)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                use iced::keyboard::{key::Named, Key};
                match key {
                    Key::Named(Named::ArrowLeft) => {
                        Some(Message::Unicode(UnicodeMessage::SelectPrev))
                    }
                    Key::Named(Named::ArrowRight) => {
                        Some(Message::Unicode(UnicodeMessage::SelectNext))
                    }
                    Key::Named(Named::ArrowUp) => {
                        Some(Message::Unicode(UnicodeMessage::SelectPrevRow))
                    }
                    Key::Named(Named::ArrowDown) => {
                        Some(Message::Unicode(UnicodeMessage::SelectNextRow))
                    }
                    Key::Named(Named::Enter) => {
                        Some(Message::Unicode(UnicodeMessage::CopySelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let history_sub = if matches!(
            self.active_overlay,
            Some(OverlayTarget::NotificationsHistory)
        ) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                use iced::keyboard::{key::Named, Key};
                match key {
                    Key::Named(Named::ArrowUp) => {
                        Some(Message::Notif(NotifMessage::HistorySelectPrev))
                    }
                    Key::Named(Named::ArrowDown) => {
                        Some(Message::Notif(NotifMessage::HistorySelectNext))
                    }
                    Key::Named(Named::Enter) => {
                        Some(Message::Notif(NotifMessage::HistoryActivateSelected))
                    }
                    Key::Named(Named::Delete) => {
                        Some(Message::Notif(NotifMessage::HistoryDeleteSelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let launcher_sub = if matches!(self.active_overlay, Some(OverlayTarget::Launcher)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key, ..
                }) = event
                else {
                    return None;
                };

                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(Message::Launcher(LauncherMessage::SelectUp))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(Message::Launcher(LauncherMessage::SelectDown))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight) => {
                        Some(Message::Launcher(LauncherMessage::SelectNext))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft) => {
                        Some(Message::Launcher(LauncherMessage::SelectPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                        Some(Message::Launcher(LauncherMessage::EnterPressed))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let lock_jump_sub = if self.is_standalone
            && matches!(self.active_overlay, Some(OverlayTarget::Lockscreen))
        {
            iced::time::every(std::time::Duration::from_millis(500))
                .map(|_| Message::Lock(LockMessage::Tick))
        } else {
            iced::Subscription::none()
        };

        let lock_key_sub = if matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            iced::event::listen_with(handle_lock_keypress)
        } else {
            iced::Subscription::none()
        };

        let lock_ipc_sub = if matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::Lock(LockMessage::PollIpc))
        } else {
            iced::Subscription::none()
        };

        let esc_sub = if matches!(self.active_overlay, Some(OverlayTarget::Email)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    Some(Message::Email(EmailMessage::Dismiss))
                } else {
                    None
                }
            })
        } else if self.annotator.text_editing.is_some() {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                        Some(Message::Annotator(AnnotatorMessage::CancelText))
                    }
                    _ => None,
                }
            })
        } else if matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            // Lock screen cannot be dismissed with Escape
            iced::Subscription::none()
        } else {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                {
                    if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                        Some(Message::DismissActiveOverlay)
                    } else {
                        None
                    }
                }
            })
        };

        // Enter → save when annotator overlay is active.
        let annotator_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator))
            && self.annotator.text_editing.is_none()
        {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                match key {
                    iced::keyboard::Key::Character(c) => match c.as_str() {
                        "r" | "R" => {
                            Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Rect)))
                        }
                        "o" | "O" | "c" | "C" => {
                            Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Ellipse)))
                        }
                        "a" | "A" => {
                            Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Arrow)))
                        }
                        "p" | "P" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Pen))),
                        "t" | "T" => {
                            Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Text)))
                        }
                        "n" | "N" => {
                            Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Counter)))
                        }
                        _ => None,
                    },
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let enter_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator))
            && self.annotator.text_editing.is_none()
        {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                else {
                    return None;
                };
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                        Some(Message::Annotator(AnnotatorMessage::Save))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let idle_tick_sub = if is_bar {
            iced::time::every(std::time::Duration::from_millis(1500)).map(|_| Message::IdleTick)
        } else {
            iced::Subscription::none()
        };
        let ipc_sub = if self.is_standalone || self.is_notif_surface {
            iced::Subscription::none()
        } else {
            iced::Subscription::run(ipc_stream)
        };

        let notif_sub = if self.is_notif_surface {
            let interval = if self.notifs.active_toasts.is_empty() {
                100
            } else {
                16
            };
            iced::time::every(std::time::Duration::from_millis(interval))
                .map(|_| Message::PollNotifs)
        } else {
            iced::Subscription::none()
        };
        // Standalone window-close listener: when Sway detaches the surface
        // on a TTY switch, exit so the coordinator/bar can respawn cleanly.
        let close_sub = if self.is_standalone {
            iced::event::listen_with(|event, _, _| {
                if let iced::Event::Window(iced::window::Event::Closed) = event {
                    Some(Message::CloseShell)
                } else {
                    None
                }
            })
        } else {
            iced::Subscription::none()
        };

        let launcher_music_sub = if matches!(self.active_overlay, Some(OverlayTarget::Launcher))
            && self.launcher.music_state.is_some()
            && self.launcher.search_query.trim().to_lowercase() == "music"
        {
            iced::time::every(std::time::Duration::from_millis(1000))
                .map(|_| Message::Launcher(LauncherMessage::MusicTick))
        } else {
            iced::Subscription::none()
        };

        let tray_animating = (self.tray_anim_width - self.tray_target_width).abs() > 0.01;
        // Pre-fetch tray items every 15s so the first open animates to the right width.
        let tray_fetch_sub = if self.is_notif_surface || self.is_standalone {
            iced::Subscription::none()
        } else {
            iced::time::every(std::time::Duration::from_secs(15)).map(|_| Message::FetchTrayItems)
        };

        // Drive tray expand/collapse animation (16ms) and timeout watchdog (500ms)
        // via Subscription::every rather than Task::perform timers.
        let tray_watchdog_sub = if tray_animating {
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::TrayWatchdogTick)
        } else if self.tray_expanded && self.settings.tray_timeout_sec > 0 {
            iced::time::every(std::time::Duration::from_millis(500))
                .map(|_| Message::TrayWatchdogTick)
        } else {
            iced::Subscription::none()
        };
        let in_bar_music_sub = if matches!(
            self.in_bar_tooltip,
            Some(InBarTooltip::Info(ref app)) if app.is_music_search_nav()
        ) {
            iced::event::listen_with(|event, _status, _window| {
                if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                {
                    match key {
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                            Some(Message::InBarInfoTip(
                                moonbeauty::overlays::info_tooltip::Message::MusicSelectNext,
                            ))
                        }
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                            Some(Message::InBarInfoTip(
                                moonbeauty::overlays::info_tooltip::Message::MusicSelectPrev,
                            ))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            })
        } else {
            iced::Subscription::none()
        };
        iced::Subscription::batch(vec![
            idle_tick_sub,
            toast_sub,
            email_sync_sub,
            weather_sub,
            esc_sub,
            enter_sub,
            ipc_sub,
            notif_sub,
            clip_sub,
            annotator_sub,
            text_typing_sub,
            email_sub,
            launcher_sub,
            launcher_music_sub,
            history_sub,
            todo_sub,
            notes_sub,
            rng_sub,
            reminders_sub,
            reminders_tick_sub,
            unicode_sub,
            lock_jump_sub,
            lock_key_sub,
            lock_ipc_sub,
            tray_watchdog_sub,
            tray_fetch_sub,
            close_sub,
            in_bar_music_sub,
        ])
    }
}
