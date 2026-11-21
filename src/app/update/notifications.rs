//! Notification-related update handlers, extracted from `app/update.rs`.
//!
//! Covers:
//!   - `Message::Notif(msg)` — NotifMessage dispatch
//!   - `Message::PollNotifs` — background poll, CLI command, resize, DND
//!   - `Message::TickToasts` — toast animation tick
//!   - `IpcCommand::{DismissLatestNotif, JumpToLatestNotif, ToggleHistory, TestNotification}`
//!   - `CapsuleAction::ToggleNotificationDnd`


#![allow(clippy::collapsible_match)]

use super::*;

impl MoonBeautyShellApp {
    fn trigger_toast_handshake(
        &mut self,
        t: moonbeauty::overlays::notifications::NotificationToast,
        action_key: Option<String>,
    ) -> Command<Message> {
        let nid = t.id;
        let is_resident = t.is_resident;
        let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        let cb = iced_layershell::actions::ActivationCallback::new(move |token| {
            if let Ok(mut guard) = tx.lock() {
                if let Some(sender) = guard.take() {
                    let _ = sender.send(token);
                }
            }
        });

        let handshake_task = Command::perform(
            async move {
                // Step 1: Wait for the xdg_activation_v1 token minted from the click event.
                let token =
                    match tokio::time::timeout(std::time::Duration::from_millis(500), rx).await {
                        Ok(Ok(tok)) => tok,
                        _ => None,
                    };
                // Steps 2 -> 3 -> 4 (D-Bus):
                // ActivationToken -> ActionInvoked (if registered) -> NotificationClosed(2) (unless resident)
                moonbeauty::overlays::notifications::emit_click_handshake(
                    nid,
                    token,
                    action_key,
                    is_resident,
                )
                .await;
            },
            move |_| {
                Message::Notif(NotifMessage::FinishToastActivation {
                    id: nid,
                    dismiss: !is_resident,
                })
            },
        );

        Command::batch(vec![
            Command::done(Message::RequestXdgActivation(cb)),
            handshake_task,
        ])
    }

    pub(crate) fn handle_notif(&mut self, msg: NotifMessage) -> Command<Message> {
        match msg {
            NotifMessage::DismissDrawer => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            NotifMessage::HistoryScrollTo(y) => {
                return iced::widget::operation::scroll_to(
                    iced::widget::Id::new("history_list"),
                    iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                );
            }
            NotifMessage::HistorySelectNext | NotifMessage::HistorySelectPrev => {
                self.notifs.update(msg);
                if let Some(y) = self
                    .notifs
                    .scroll_offset_for_selected(&self.theme, &self.settings)
                {
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("history_list"),
                        iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                    );
                }
            }
            NotifMessage::HistoryDeleteSelected => {
                self.notifs.update(msg);
                if let Some(y) = self
                    .notifs
                    .scroll_offset_for_selected(&self.theme, &self.settings)
                {
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("history_list"),
                        iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                    );
                }
            }
            NotifMessage::DismissToast(id) => {
                self.notifs.dismiss_toast_ui(id);
                if self.is_notif_surface {
                    return Command::perform(
                        moonbeauty::overlays::notifications::emit_notification_closed(id, 2),
                        |_| Message::Noop,
                    );
                }
            }
            NotifMessage::ActivateToast(id) => {
                if let Some(t) = self
                    .notifs
                    .active_toasts
                    .iter()
                    .find(|t| t.id == id && t.exit_started_at.is_none())
                    .cloned()
                {
                    let action_key = t
                        .actions
                        .chunks_exact(2)
                        .find(|pair| pair[0] == "default")
                        .map(|_| "default".to_string());
                    return self.trigger_toast_handshake(t, action_key);
                }
            }
            NotifMessage::InvokeToastAction(id, key) => {
                if let Some(t) = self
                    .notifs
                    .active_toasts
                    .iter()
                    .find(|t| t.id == id && t.exit_started_at.is_none())
                    .cloned()
                {
                    let action_key = if t.actions.chunks_exact(2).any(|pair| pair[0] == key) {
                        Some(key)
                    } else {
                        None
                    };
                    return self.trigger_toast_handshake(t, action_key);
                }
            }
            NotifMessage::FinishToastActivation { id, dismiss } => {
                if dismiss {
                    self.notifs.dismiss_toast_ui(id);
                }
            }
            _ => self.notifs.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_poll_notifs(&mut self) -> Command<Message> {
        let mtime = std::fs::metadata(ShellSettings::config_path())
            .ok()
            .and_then(|m| m.modified().ok());
        if mtime != self.settings_json_mtime {
            self.settings_json_mtime = mtime;
            self.settings = ShellSettings::load();
            self.theme = ShellTheme::from_settings(&self.settings);
        }
        if !self.is_notif_surface
            && !self.is_standalone
            && self.settings.notif_screen_name != self.notif_screen_at_startup
        {
            self.notif_screen_at_startup = self.settings.notif_screen_name.clone();
            let _ = std::process::Command::new("pkill")
                .args(["-f", "--", "(^|/)moonbeauty --notif-surface$"])
                .status();
            std::thread::sleep(std::time::Duration::from_millis(300));
            let exe = resolve_exe();
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("--notif-surface");
            let target = &self.settings.notif_screen_name;
            if !target.is_empty() && target != "Auto" && target != "Auto (Primary)" {
                cmd.env("MBS_OUTPUT", target);
            } else {
                cmd.env_remove("MBS_OUTPUT");
            }
            let _ = moonbeauty::libraries::process::spawn_and_reap(&mut cmd);
        }
        if let Some(rx) = self.notif_rx.as_ref() {
            while let Ok(n) = rx.try_recv() {
                let is_test = n.app_name == "Diagnostics";
                self.notifs.push_notification(
                    n.id,
                    &n.app_name,
                    &n.summary,
                    &n.body,
                    n.is_urgent,
                    n.is_resident,
                    is_test,
                    n.desktop_entry.as_deref(),
                    n.actions.clone(),
                    &self.settings,
                );
            }
        }
        if self.is_notif_surface {
            let runtime = std::env::var("XDG_RUNTIME_DIR")
                .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
            let cmd_path = std::path::PathBuf::from(&runtime).join("moonbeauty-notif-cmd");
            if let Ok(cmd) = std::fs::read_to_string(&cmd_path) {
                let _ = std::fs::remove_file(&cmd_path);
                match cmd.trim() {
                    "dismiss" => {
                        // Dismiss the oldest active toast first so newer ones remain visible
                        if let Some(oldest) = self.notifs.oldest_active_toast() {
                            let oid = oldest.id;
                            self.notifs.dismiss_toast_ui(oid);
                            return Command::perform(
                                moonbeauty::overlays::notifications::emit_notification_closed(
                                    oid, 2,
                                ),
                                |_| Message::Noop,
                            );
                        }
                    }
                    "activate-latest" => {
                        if let Some(t) = self.notifs.oldest_active_toast() {
                            let action_key = t
                                .actions
                                .chunks_exact(2)
                                .find(|pair| pair[0] == "default")
                                .map(|_| "default".to_string());
                            return self.trigger_toast_handshake(t, action_key);
                        } else {
                            eprintln!("[activate-latest] no active toast");
                        }
                    }
                    _ => {}
                }
            }


            if !moonbeauty::overlays::notifications::dnd_active() {
                let held = moonbeauty::overlays::notifications::take_held_notifs();
                if !held.is_empty() {
                    eprintln!(
                        "[dnd] unpaused! releasing {} held toast(s) to screen",
                        held.len()
                    );
                    for mut item in held {
                        item.created_at = std::time::Instant::now();
                        item.hold_reset_at = Some(std::time::Instant::now());
                        item.exit_started_at = None;
                        item.slot_shift_start = None;
                        const MAX_PENDING_TOASTS: usize = 50;
                        if self.notifs.active_toasts.len() < 4 {
                            self.notifs.active_toasts.push(item);
                        } else {
                            if self.notifs.pending_toasts.len() >= MAX_PENDING_TOASTS {
                                self.notifs.pending_toasts.pop_front();
                            }
                            self.notifs.pending_toasts.push_back(item);
                        }
                    }
                }
            }
            let big_w =
                (self.settings.notif_card_width as i32 + 2 * self.settings.notif_margin_x + 100)
                    .max(200) as u32;
            let big_h = (3 * self.settings.notif_card_height as i32
                + 2 * self.settings.notif_margin_x
                + 100)
                .max(300) as u32;
            let want = if self.notifs.active_toasts.is_empty() {
                (1u32, 1u32)
            } else {
                (big_w, big_h)
            };
            let toast_count = self.notifs.active_toasts.len();
            let count_changed = toast_count != self.last_toast_count;

            let active_pending = self.notifs.active_toasts.len() + self.notifs.pending_toasts.len();
            let should_write = match self.last_active_pending_written {
                None => true,
                Some((prev, at)) => prev != active_pending || at.elapsed().as_secs() >= 3,
            };
            if should_write {
                moonbeauty::overlays::notifications::write_active_count(active_pending);
                self.last_active_pending_written =
                    Some((active_pending, std::time::Instant::now()));
            }

            if want != self.notif_surface_size || count_changed {
                self.notif_surface_size = want;
                self.last_toast_count = toast_count;
                let rects = self.compute_toast_rects();
                return Command::batch(vec![
                    Command::done(Message::ResizeNotifSurface(want.0, want.1)),
                    Command::done(Message::SetToastInputRegion(rects)),
                ]);
            }
        }
        Command::none()
    }

    pub(crate) fn handle_tick_toasts(&mut self) -> Command<Message> {
        self.notifs.tick_toasts(
            self.settings.notif_hold_duration_sec,
            self.settings.notif_volume,
        );
        self.notifs.flush_history();
        Command::none()
    }

    pub(crate) fn dismiss_latest_notif(&self) -> Command<Message> {
        let cmd_path =
            moonbeauty::libraries::process::mbs_runtime_dir().join("moonbeauty-notif-cmd");
        // private_write_file: O_EXCL + 0600 + refuses pre-placed
        // symlinks. std::fs::write followed symlinks and inherited umask.
        let tmp_path = cmd_path.with_extension("tmp");
        if moonbeauty::libraries::process::private_write_file(&tmp_path, b"dismiss").is_ok() {
            let _ = std::fs::rename(&tmp_path, &cmd_path);
            eprintln!("[ipc] DismissLatestNotif -> notif-surface");
        } else {
            eprintln!("[ipc] DismissLatestNotif write failed");
        }
        Command::none()
    }

    pub(crate) fn jump_to_latest_notif(&mut self) -> Command<Message> {
        self.spawn_standalone(OverlayTarget::NotificationsHistory);
        Command::none()
    }

    pub(crate) fn toggle_history(&mut self) -> Command<Message> {
        self.toggle_standalone(OverlayTarget::NotificationsHistory);
        Command::none()
    }

    pub(crate) fn test_notification(&self) -> Command<Message> {
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
            let _ = moonbeauty::libraries::process::spawn_and_reap(
                std::process::Command::new("notify-send").args([
                    "-a",
                    "Diagnostics",
                    "--",
                    title,
                    body,
                ]),
            );
        }
        Command::none()
    }

    pub(crate) fn toggle_notification_dnd(&mut self) -> Command<Message> {
        let paused = moonbeauty::overlays::notifications::toggle_dnd();
        self.settings.notifications_enabled = !paused;
        let _ = self.settings.save();
        eprintln!("[dnd] paused={}", paused);

        let running = std::process::Command::new("pgrep")
            .args(["-f", "--", "(^|/)moonbeauty --notif-surface$"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false);
        if !running {
            let exe = resolve_exe();
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("--notif-surface");
            let _ = moonbeauty::libraries::process::spawn_and_reap(&mut cmd);
            eprintln!("[dnd] respawned notif-surface");
        }
        Command::none()
    }
}
