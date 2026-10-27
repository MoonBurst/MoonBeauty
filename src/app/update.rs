use super::*;

impl MoonBeautyShellApp {
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::WindowResized(w) => {
                self.window_w = w;
            }
            Message::ResizeNotifSurface(w, h) => {
                eprintln!(
                    "[resize] update caught ({}, {}) — try_into NOT called",
                    w, h
                );
            }
            Message::PollNotifs => {
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
                        .args(["-f", "--", "moon-beauty-shell --notif-surface"])
                        .status();
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let exe = resolve_exe();
                    let _ = std::process::Command::new(exe)
                        .arg("--notif-surface")
                        .spawn();
                }
                if let Some(rx) = self.notif_rx.as_ref() {
                    while let Ok(n) = rx.try_recv() {
                        let is_test = n.app_name == "Diagnostics";
                        self.notifs.push_notification(
                            &n.app_name,
                            &n.summary,
                            &n.body,
                            n.is_urgent,
                            is_test,
                            n.desktop_entry.as_deref(),
                            n.actions.clone(),
                            &self.settings,
                        );
                    }
                }
                if self.is_notif_surface {
                    // CLI notification control (see --activate-latest-notif / --dismiss-notif)
                    let runtime = std::env::var("XDG_RUNTIME_DIR")
                        .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
                    let cmd_path =
                        std::path::PathBuf::from(&runtime).join("moon-beauty-shell-notif-cmd");
                    if let Ok(cmd) = std::fs::read_to_string(&cmd_path) {
                        let _ = std::fs::remove_file(&cmd_path);
                        match cmd.trim() {
                            "dismiss" => {
                                // Target newest — after the view_toasts z-order
                                // fix, the newest toast is on top of z-order and
                                // is the one the user sees. Dismissing .first()
                                // (oldest) would hide a toast they weren't looking
                                // at, making the whole command feel like a no-op.
                                if let Some(last) = self.notifs.active_toasts.first().cloned() {
                                    self.notifs.update(NotifMessage::DismissToast(last.id));
                                }
                            }
                            "activate-latest" => {
                                if let Some(t) = self.notifs.active_toasts.first().cloned() {
                                    let app = t.app_name.clone();
                                    let entry = t.desktop_entry.clone();
                                    let nid = t.id;
                                    self.notifs.update(NotifMessage::DismissToast(t.id));
                                    std::thread::spawn(move || {
                                        moon_beauty_shell::overlays::notifications::emit_action_invoked(nid, "default");
                                        std::thread::sleep(std::time::Duration::from_millis(200));
                                        let matches =
                                            notification_candidates(&app, entry.as_deref());
                                        if !is_app_focused(&matches) {
                                            let ok = focus_notification_app(&app, entry.as_deref());
                                            eprintln!("[activate-latest] fallback app={:?} entry={:?} ok={}", app, entry, ok);
                                        } else {
                                            eprintln!(
                                                "[activate-latest] client self-focused app={:?}",
                                                app
                                            );
                                        }
                                    });
                                } else {
                                    eprintln!("[activate-latest] no active toast");
                                }
                            }
                            _ => {}
                        }
                    }

                    if !moon_beauty_shell::overlays::notifications::dnd_active() {
                        let held = moon_beauty_shell::overlays::notifications::take_held_notifs();
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
                                if self.notifs.active_toasts.len() < 4 {
                                    self.notifs.active_toasts.push(item);
                                } else {
                                    self.notifs.pending_toasts.push_back(item);
                                }
                            }
                        }
                    }
                    let big_w = (self.settings.notif_card_width as i32
                        + 2 * self.settings.notif_margin_x
                        + 100)
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

                    // Publish active+pending to the bar. Runs on every tick,
                    // but only writes on change or once every 3s heartbeat.
                    let active_pending =
                        self.notifs.active_toasts.len() + self.notifs.pending_toasts.len();
                    let should_write = match self.last_active_pending_written {
                        None => true,
                        Some((prev, at)) => prev != active_pending || at.elapsed().as_secs() >= 3,
                    };
                    if should_write {
                        moon_beauty_shell::overlays::notifications::write_active_count(
                            active_pending,
                        );
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
            }
            Message::TickClock => {
                self.telemetry.bar_time = ClockBackend::get_bar_time();
                // Only the primary bar process fires reminder notifications.
                // The standalone reminders overlay and the notification surface
                // also run TickClock every second; without this guard both see
                // alerted=false simultaneously and each fires a notify-send +
                // writes alerted=true, producing duplicate alerts.
                if !self.is_standalone && !self.is_notif_surface {
                    let now = chrono::Local::now().timestamp();
                    let mut entries =
                        moon_beauty_shell::overlays::reminders::RemindersOverlay::load();
                    let mut changed = false;
                    for e in entries.iter_mut() {
                        if e.target_epoch <= now && !e.alerted {
                            e.alerted = true;
                            changed = true;
                            let text = if e.text.is_empty() {
                                "Timer Finished!".into()
                            } else {
                                e.text.clone()
                            };
                            let _ = std::process::Command::new("notify-send")
                                .args([
                                    "-a",
                                    "Alarm",
                                    "-u",
                                    "critical",
                                    "⏰ Timer Complete!",
                                    &text,
                                ])
                                .spawn();

                            if e.play_sound {
                                if let Some(sound) = &self.settings.alarm_sound {
                                    play_alarm_sound(
                                        sound,
                                        self.settings.alarm_sound_volume,
                                        self.settings.alarm_sound_duration_sec,
                                    );
                                }
                            }
                        }
                    }
                    if changed {
                        moon_beauty_shell::overlays::reminders::save_reminders(&entries);
                    }
                }
            }
            Message::TickToasts => {
                self.notifs.tick_toasts(
                    self.settings.notif_hold_duration_sec,
                    self.settings.notif_volume,
                );
            }
            Message::ComposeTabPressed(shift) => {
                if self.email.is_composing {
                    if !self.email.get_autocomplete_matches().is_empty() {
                        self.email.update(EmailMessage::ConfirmAutocomplete);
                        return Command::none();
                    }
                    // Iced 0.14 only has focus_next, so Shift+Tab will just go forward.
                    let _ = shift;
                    return iced::widget::operation::focus_next();
                }
            }
            Message::FocusComposeField => {
                self.email.active_compose_field = (self.email.active_compose_field + 1) % 3;
                let id = match self.email.active_compose_field {
                    0 => moon_beauty_shell::overlays::email::COMPOSE_TO_ID,
                    1 => moon_beauty_shell::overlays::email::COMPOSE_SUBJ_ID,
                    _ => moon_beauty_shell::overlays::email::COMPOSE_BODY_ID,
                };
                return iced::widget::operation::focus(id).map(Message::Email);
            }
            Message::TickEmailSync => {
                // Only the primary bar and the standalone email overlay
                // sync IMAP. The notif-surface doesn't need email data and
                // duplicating the sync doubles IMAP traffic + log noise.
                if self.is_notif_surface {
                    return Command::none();
                }
                if moon_beauty_shell::libraries::backend::email_client::config_exists() {
                    eprintln!("[sync] Boot/Timer triggered email sync");
                    let folder = "INBOX".to_string();
                    return Command::perform(
                        async move {
                            let old_envs =
                                moon_beauty_shell::libraries::backend::email_client::load_index(
                                    &folder,
                                );
                            if let Ok(new_envs) = moon_beauty_shell::libraries::backend::email_client::fetch_from_network(&folder).await {
                                moon_beauty_shell::libraries::backend::email_client::save_index(&folder, &new_envs);
                                moon_beauty_shell::libraries::backend::email_client::sync_bodies_background(folder).await;
                                Ok::<_, String>((old_envs, new_envs))
                            } else {
                                Err("fetch failed".into())
                            }
                        },
                        |res| match res {
                            Ok((old, new)) => Message::EmailSyncedWithDiff(old, new),
                            Err(_) => Message::EmailSynced,
                        },
                    );
                }
            }
            Message::EmailSyncedWithDiff(old, new) => {
                eprintln!(
                    "[sync] EmailSyncedWithDiff: old={} new={}",
                    old.len(),
                    new.len()
                );

                // Cross-process dedup file: prevent two processes from notifying the same email
                let dedup_path = dirs::runtime_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join("mbs-email-notified.txt");
                let mut notified: std::collections::HashSet<String> =
                    std::fs::read_to_string(&dedup_path)
                        .ok()
                        .map(|s| {
                            s.lines()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();

                let old_ids: std::collections::HashSet<String> =
                    old.iter().map(|e| e.id.clone()).collect();
                for env in new {
                    if !old_ids.contains(&env.id) && !notified.contains(&env.id) {
                        eprintln!("[sync] New email from {}: {}", env.sender, env.subject);

                        // Visual notification
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Email", &env.sender, &env.subject])
                            .spawn();

                        // Spoken announcement
                        moon_beauty_shell::overlays::notifications::NotificationOverlay::speak_tts(
                            "Email",
                            &env.sender,
                            &env.subject,
                            Some("New email from {sender}: {body}"),
                            self.settings.tts_command.clone(),
                            self.settings.notif_volume,
                        );

                        notified.insert(env.id.clone());
                    }
                }

                // Persist dedup list (cap at 200 most recent)
                let mut sorted: Vec<_> = notified.into_iter().collect();
                sorted.sort();
                sorted.truncate(200);
                let _ = std::fs::write(&dedup_path, sorted.join("\n"));
            }
            Message::EmailSynced => {}
            Message::TickWeather => {
                return Command::perform(
                    async {
                        moon_beauty_shell::bar::weather::WeatherBackend::fetch()
                            .await
                            .ok()
                    },
                    Message::WeatherLoaded,
                );
            }
            Message::WeatherLoaded(w) => {
                self.telemetry.weather = w;
            }
            Message::TickHardware => {
                self.telemetry.gpu_selected_idx = self
                    .detected_gpus
                    .iter()
                    .position(|g| g.card_id == self.settings.gpu_selected_id)
                    .unwrap_or(0);
                let bat = SystemCollector::collect_battery();
                self.telemetry.battery = bat;
                self.telemetry.net = self.system_collector.update_net();

                let gpu_id = self.settings.gpu_selected_id.clone();

                return Command::perform(
                    async move {
                        let cpu = SystemCollector::collect_cpu().await;
                        let ram = SystemCollector::collect_ram().await;
                        let gpu = SystemCollector::collect_gpu_by_card_id(&gpu_id).await;
                        let audio = AudioBackend::get_sink().await;
                        let mic = AudioBackend::get_source().await;
                        let mpris = MprisBackend::poll().await;
                        (cpu, ram, gpu, audio, mic, mpris)
                    },
                    |(cpu, ram, gpu, audio, mic, mpris)| {
                        Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris)
                    },
                );
            }
            Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris) => {
                self.telemetry.cpu = cpu;
                self.telemetry.ram = ram;
                self.telemetry.gpu = gpu;
                self.telemetry.audio = audio;
                self.telemetry.mic = mic;
                self.telemetry.mpris = mpris;
            }
            Message::IpcReceived(cmd) => match cmd {
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
                        if let Some(OverlayTarget::Lockscreen) = self.standalone_running() {
                            eprintln!("[ipc] lockscreen already active; ignoring duplicate toggle");
                            return Command::none();
                        }
                        eprintln!("[ipc] lockscreen spawn (cannot be toggled off via IPC)");
                        self.spawn_standalone(OverlayTarget::Lockscreen);
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
                        // Lockscreen MUST be a standalone process so it can
                        // spawn the multi-monitor coordinator. Idempotent: if
                        // it's already active, ignore. Respawn = kill+spawn =
                        // unlocked window. Only auth success removes the lock.
                        if let Some(OverlayTarget::Lockscreen) = self.standalone_running() {
                            eprintln!("[ipc] lockscreen already active; ignoring duplicate open");
                            return Command::none();
                        }
                        self.spawn_standalone(target);
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
                    if self.active_overlay == Some(target) {
                        self.active_overlay = None;
                    }
                }
                IpcCommand::CloseAll => {
                    self.active_overlay = None;
                }
                IpcCommand::DismissLatestNotif => {
                    // Bar's active_toasts is empty — the notif-surface
                    // process owns the actual toast stack. Route through the
                    // same file-based command channel that --dismiss-notif
                    // already uses. Without this routing, DismissLatestNotif
                    // was a guaranteed no-op (Vec::first() on an empty Vec).
                    let runtime = std::env::var("XDG_RUNTIME_DIR")
                        .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
                    let cmd_path =
                        std::path::PathBuf::from(runtime).join("moon-beauty-shell-notif-cmd");
                    match std::fs::write(&cmd_path, b"dismiss") {
                        Ok(_) => eprintln!("[ipc] DismissLatestNotif -> notif-surface"),
                        Err(e) => eprintln!("[ipc] DismissLatestNotif write failed: {e}"),
                    }
                }
                IpcCommand::JumpToLatestNotif => {
                    self.spawn_standalone(OverlayTarget::NotificationsHistory);
                }
                IpcCommand::ToggleHistory => {
                    self.toggle_standalone(OverlayTarget::NotificationsHistory);
                }
                IpcCommand::ReloadSettings => {
                    self.settings = ShellSettings::load();
                    self.theme = ShellTheme::from_settings(&self.settings);
                }
                IpcCommand::TestNotification => {
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
                }
                IpcCommand::IncomingNotif { .. } => {}
                IpcCommand::Ping => {}
            },
            Message::PollIpc => {
                while let Ok(cmd) = self.ipc_rx.try_recv() {
                    let _ = self.update(Message::IpcReceived(cmd));
                }
            }
            Message::TopBar(msg) => {
                eprintln!("[topbar-msg] {:?}", msg);
                if let TopBarMessage::Action(act) = msg {
                    match act {
                        CapsuleAction::OpenOverlay(id) => {
                            eprintln!("[topbar-action] OpenOverlay: id = {:?}", id);

                            // Recent click => use cursor position for dynamic placement.
                            // Fallback to capsule_center_x if no click in last 5s (e.g. IPC trigger).
                            let recent_click =
                                self.last_cursor_time.elapsed() < std::time::Duration::from_secs(5);

                            // 1. ALARM — standalone reminders overlay.
                            // Handled BEFORE the tooltip-open/toggle block so:
                            //   (a) an open tooltip can't hijack the click,
                            //   (b) id="alarm" never enters the tooltip spawn
                            //       selection and falls through to --sys-tooltip
                            //       (which defaults unknown targets to RAM).
                            if id == "alarm" {
                                if let Some(mut c) = self.weather_tooltip_child.take() {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                }
                                self.toggle_standalone(OverlayTarget::Reminders);
                                return Command::none();
                            }

                            // 2. DROPDOWN TOOLTIPS (CPU, GPU, RAM, Tray)
                            if matches!(
                                id.as_str(),
                                "weather"
                                    | "cpu"
                                    | "gpu"
                                    | "ram"
                                    | "calendar"
                                    | "music"
                                    | "unified"
                                    | "net"
                                    | "custom"
                                    | "clock"
                            ) {
                                // Toggle off if already open
                                let running = self
                                    .weather_tooltip_child
                                    .as_mut()
                                    .and_then(|c| c.try_wait().ok())
                                    .is_some_and(|status| status.is_none());
                                if running {
                                    eprintln!("[tooltip] Already open; toggling off");
                                    if let Some(mut c) = self.weather_tooltip_child.take() {
                                        let _ = c.kill();
                                        let _ = c.wait();
                                    }
                                    return Command::none();
                                }
                                self.weather_tooltip_child = None;

                                let target_output = if self.settings.notif_screen_name != "Auto"
                                    && self.settings.notif_screen_name != "Auto (Primary)"
                                    && !self.settings.notif_screen_name.is_empty()
                                {
                                    Some(self.settings.notif_screen_name.as_str())
                                } else {
                                    None
                                };
                                let sw = get_screen_width_for(target_output);
                                let _cx = if recent_click && self.last_cursor_x > 0.0 {
                                    self.last_cursor_x as i32
                                } else {
                                    capsule_center_x(&self.settings, sw, &id)
                                };
                                let target_output = if self.settings.notif_screen_name != "Auto"
                                    && self.settings.notif_screen_name != "Auto (Primary)"
                                    && !self.settings.notif_screen_name.is_empty()
                                {
                                    Some(self.settings.notif_screen_name.as_str())
                                } else {
                                    None
                                };
                                let sw = get_screen_width_for(target_output);
                                let cx = if id == "clock" {
                                    capsule_center_x(&self.settings, sw, &id)
                                } else if recent_click && self.last_cursor_x > 0.0 {
                                    self.last_cursor_x as i32
                                } else {
                                    capsule_center_x(&self.settings, sw, &id)
                                };
                                let in_left = self.settings.bar_left.contains(&id);
                                let in_center = self.settings.bar_center.contains(&id);
                                let in_right = self.settings.bar_right.contains(&id);
                                let (slant_l, slant_r): (&str, &str) =
                                    match self.settings.slant_mode.as_str() {
                                        "left" => ("L", "L"),
                                        "right" => ("R", "R"),
                                        _ => {
                                            if in_center {
                                                let vis: Vec<&String> = self
                                                    .settings
                                                    .bar_center
                                                    .iter()
                                                    .filter(|x| {
                                                        self.settings
                                                            .visible_capsules
                                                            .get(*x)
                                                            .copied()
                                                            .unwrap_or(true)
                                                    })
                                                    .collect();
                                                let k = vis.len() / 2;
                                                match vis.iter().position(|x| **x == id) {
                                                    Some(i) if i == k => ("L", "R"),
                                                    Some(i) if i < k => ("L", "L"),
                                                    Some(_) => ("R", "R"),
                                                    None => ("L", "R"),
                                                }
                                            } else if in_right {
                                                ("R", "R")
                                            } else {
                                                ("L", "L")
                                            }
                                        }
                                    };
                                let cap_w = capsule_actual_width(&id);
                                let grow_right = if in_left {
                                    true
                                } else if in_right {
                                    false
                                } else {
                                    cx < (sw as i32) / 2
                                };
                                let (tip_w, _tip_h) = self.settings.tooltip_size(&id);
                                let tip_output = if self.settings.bar_monitor != "All"
                                    && !self.settings.bar_monitor.is_empty()
                                {
                                    Some(self.settings.bar_monitor.clone())
                                } else {
                                    std::env::var("MBS_OUTPUT")
                                        .ok()
                                        .filter(|s| !s.is_empty())
                                        .or_else(resolve_primary_output)
                                };

                                let req = moon_beauty_shell::bar::tooltip_spawn::SpawnRequest {
                                    id: &id,
                                    exe: resolve_exe(),
                                    output: tip_output.as_deref(),
                                    tooltip_w: tip_w,
                                    tooltip_h:
                                        moon_beauty_shell::bar::tooltip_spawn::compute_height(
                                            &id,
                                            &self.settings,
                                            &self.telemetry,
                                        ),
                                    drop_ms: self.settings.tooltip_drop_ms,
                                    expand_ms: self.settings.tooltip_expand_ms,
                                    capsule_cx: cx,
                                    capsule_w: cap_w,
                                    screen_w: sw,
                                    slant_l,
                                    slant_r,
                                    grow_right,
                                };

                                match moon_beauty_shell::bar::tooltip_spawn::spawn(&req) {
                                    Ok(c) => {
                                        eprintln!("[ipc] spawned tooltip pid={}", c.id());
                                        self.weather_tooltip_child = Some(c);
                                    }
                                    Err(e) => eprintln!("[tooltip] Failed to spawn: {e}"),
                                }
                                return Command::none();
                            }

                            // 2. FULLSCREEN / STANDALONE OVERLAYS (Settings, History, Power)
                            let mapped: Option<OverlayTarget> = match id.as_str() {
                                "notify" => Some(OverlayTarget::NotificationsHistory),
                                "battery" => Some(OverlayTarget::Power),
                                _ => OverlayTarget::from_str(&id),
                            };
                            if let Some(target) = mapped {
                                if matches!(
                                    target,
                                    OverlayTarget::Launcher
                                        | OverlayTarget::Clipboard
                                        | OverlayTarget::NotificationsHistory
                                        | OverlayTarget::Settings
                                ) {
                                    self.spawn_standalone(target);
                                } else {
                                    self.active_overlay = Some(target);
                                }
                            }
                        }
                        CapsuleAction::TogglePlayPause => {
                            return Command::perform(MprisBackend::toggle_play_pause(), |_| {
                                Message::TopBar(TopBarMessage::Noop)
                            });
                        }
                        CapsuleAction::NextTrack => {
                            return Command::perform(MprisBackend::next(), |_| {
                                Message::TopBar(TopBarMessage::Noop)
                            });
                        }
                        CapsuleAction::PreviousTrack => {
                            return Command::perform(MprisBackend::previous(), |_| {
                                Message::TopBar(TopBarMessage::Noop)
                            });
                        }
                        CapsuleAction::CycleAudioSink => {
                            return Command::perform(AudioBackend::cycle_sinks(), |_| {
                                Message::TopBar(TopBarMessage::Noop)
                            });
                        }
                        CapsuleAction::ToggleSinkMute => {
                            return Command::perform(AudioBackend::toggle_sink_mute(), |_| {
                                Message::TopBar(TopBarMessage::Noop)
                            });
                        }
                        CapsuleAction::ToggleNotificationDnd => {
                            let paused = moon_beauty_shell::overlays::notifications::toggle_dnd();
                            self.settings.notifications_enabled = !paused;
                            let _ = self.settings.save();
                            eprintln!("[dnd] paused={}", paused);

                            // The notif-surface owns org.freedesktop.Notifications
                            // and writes held-notifs.json. It must stay alive
                            // while paused to receive + hold. Only respawn if
                            // it died for some unrelated reason.
                            let running = std::process::Command::new("pgrep")
                                .args(["-f", "--", "moon-beauty-shell --notif-surface"])
                                .output()
                                .map(|o| !o.stdout.is_empty())
                                .unwrap_or(false);
                            if !running {
                                let exe = resolve_exe();
                                let _ = std::process::Command::new(exe)
                                    .arg("--notif-surface")
                                    .spawn();
                                eprintln!("[dnd] respawned notif-surface");
                            }
                        }
                        CapsuleAction::CancelTooltip(id) => {
                            eprintln!("[topbar] CancelTooltip: {}", id);
                            if let Some(mut c) = self.weather_tooltip_child.take() {
                                let _ = c.kill();
                                let _ = c.wait();
                            }
                            // Right-click on alarm capsule: wipe active timers
                            if id == "alarm" {
                                moon_beauty_shell::overlays::reminders::save_reminders(&[]);
                                eprintln!("[topbar] cleared active timers");
                            }
                        }
                        CapsuleAction::ToggleSourceMute => {
                            return Command::perform(AudioBackend::toggle_source_mute(), |_| {
                                Message::TopBar(TopBarMessage::Noop)
                            });
                        }
                        CapsuleAction::ToggleTrayExpanded => {
                            if !self.tray_expanded {
                                // OPENING
                                self.tray_timeout_gen = self.tray_timeout_gen.wrapping_add(1);
                                let _my_gen = self.tray_timeout_gen;

                                self.tray_opened_at = std::time::Instant::now();
                                self.tray_expanded = true;
                                self.tray_anim_start_width = self.tray_anim_width;
                                self.tray_anim_started_at = std::time::Instant::now();
                                let expanded_w = moon_beauty_shell::bar::capsules::compute_expanded_visible_width(
                                    &self.settings, self.tray_items.len()
                                );
                                self.tray_target_width = expanded_w;
                                self.tray_anim_duration_ms = 220;

                                // Auto-close is handled by the tray_watchdog_sub
                                // subscription (see subscription()), which polls
                                // tray_opened_at.elapsed() every 500ms while
                                // tray_expanded is true. No per-open timer needed.
                                return Command::perform(
                                    moon_beauty_shell::bar::tray::watcher::fetch_tray_items(),
                                    Message::TrayItemsLoaded,
                                );
                                return Command::perform(
                                    moon_beauty_shell::bar::tray::watcher::fetch_tray_items(),
                                    Message::TrayItemsLoaded,
                                );
                            } else {
                                // CLOSING — bump gen to invalidate any pending timer
                                self.close_tray();
                            }
                        }
                        CapsuleAction::TrayActivate(item) => {
                            self.close_tray();
                            return Command::perform(
                                async move {
                                    let _ =
                                        moon_beauty_shell::bar::tray::watcher::activate_tray_item(
                                            &item.bus, &item.path,
                                        )
                                        .await;
                                },
                                |_| Message::TopBar(TopBarMessage::Noop),
                            );
                        }
                        CapsuleAction::TrayContextMenu(item) => {
                            let item_clone = item.clone();
                            return Command::perform(
                                async move {
                                    if let Some((menu_path, items)) =
                                        moon_beauty_shell::bar::tray::watcher::get_menu_tree(
                                            &item_clone.bus,
                                            &item_clone.path,
                                        )
                                        .await
                                    {
                                        let json = serde_json::to_string(&items).unwrap();
                                        let tmp_path = format!(
                                            "/tmp/mbs-tray-menu-{}.json",
                                            std::process::id()
                                        );
                                        std::fs::write(&tmp_path, json).unwrap();
                                        let exe = resolve_exe();
                                        let h = (items.len() as u32 * 36).clamp(100, 600) + 40;
                                        let _ = std::process::Command::new(exe)
                                            .args([
                                                "--tray-context-menu",
                                                &tmp_path,
                                                &item_clone.bus,
                                                &menu_path,
                                                "250",
                                                &h.to_string(),
                                            ])
                                            .spawn();
                                    }
                                },
                                |_| Message::TopBar(TopBarMessage::Noop),
                            );
                        }
                        CapsuleAction::CycleGpuTarget => {
                            let gpus = &self.detected_gpus;
                            if !gpus.is_empty() {
                                let current = &self.settings.gpu_selected_id;
                                let next_idx = gpus
                                    .iter()
                                    .position(|g| &g.card_id == current)
                                    .map(|i| (i + 1) % gpus.len())
                                    .unwrap_or(0);
                                self.settings.gpu_selected_id = gpus[next_idx].card_id.clone();
                                let _ = self.settings.save();
                                eprintln!(
                                    "[bar] right-click GPU: switched to {}",
                                    self.settings.gpu_selected_id
                                );
                                return Command::done(Message::TickHardware);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::Launcher(msg) => match msg {
                LauncherMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                LauncherMessage::ExecuteApp(cmd) => {
                    // Tab pressed on math row — open calculator with same expression
                    if cmd == "__tab__" {
                        // Tab = Enter on currently selected item.
                        // For a math row, this copies the result.
                        // For an app row, this launches the app.
                        if let Some(selected) = self
                            .launcher
                            .filtered_apps
                            .get(self.launcher.selected_index)
                        {
                            let resolved = selected.exec.clone();
                            return self
                                .update(Message::Launcher(LauncherMessage::ExecuteApp(resolved)));
                        }
                        return Command::none();
                    }
                    // Math result — copy to clipboard and close
                    if let Some(result) = cmd.strip_prefix("math:") {
                        let result = result.to_string();
                        eprintln!("[launcher] math result -> wl-copy: {}", result);
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Launcher", "Copied", &result])
                            .spawn();
                        if self.is_standalone {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            let r = result.clone();
                            rt.block_on(async move {
                                let _ = tokio::process::Command::new("wl-copy")
                                    .arg(&r)
                                    .status()
                                    .await;
                            });
                            std::process::exit(0);
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
                            .arg(r#"pass -c "$1" 2>&1 && notify-send -a 'Password Store' 'Copied' "$1""#)
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
                _ => return self.launcher.update(msg).map(Message::Launcher),
            },
            Message::Calc(msg) => match msg {
                CalcMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                CalcMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::calc::CALC_INPUT_ID,
                    )
                    .map(Message::Calc);
                }
                CalcMessage::CopyResult(val) => {
                    if self.is_standalone {
                        // Block on the copy so it completes before exit.
                        let v = val.clone();
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            CalculatorOverlay::copy_to_clipboard(&v).await;
                        });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move {
                            CalculatorOverlay::copy_to_clipboard(&val).await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                _ => self.calc.update(msg),
            },
            Message::Clipboard(msg) => match msg {
                ClipboardMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                ClipboardMessage::CopyEntry(id) => {
                    if self.is_standalone {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            ClipboardOverlay::copy(id).await;
                        });
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move {
                            ClipboardOverlay::copy(id).await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                ClipboardMessage::DeleteEntry(id) => {
                    return Command::perform(
                        async move {
                            ClipboardOverlay::delete(id).await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                ClipboardMessage::WipeAll => {
                    return Command::perform(
                        async move {
                            ClipboardOverlay::wipe().await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                ClipboardMessage::SearchChanged(q) => {
                    self.clipboard.search_query = q;
                }
                ClipboardMessage::Reloaded(entries) => {
                    self.clipboard.entries = entries;
                }
                ClipboardMessage::SelectNext => {
                    let ids = self.clipboard.filtered_ids();
                    if ids.is_empty() {
                        return Command::none();
                    }
                    let next_idx = self
                        .clipboard
                        .selected_id
                        .and_then(|sel| ids.iter().position(|&id| id == sel))
                        .map(|p| (p + 1) % ids.len())
                        .unwrap_or(0);
                    let next_id = ids[next_idx];
                    eprintln!(
                        "[debug-nav] Down pressed. next_idx: {}, next_id: {}",
                        next_idx, next_id
                    );
                    self.clipboard.selected_id = Some(next_id);
                    return Command::batch([
                        Command::perform(
                            ClipboardOverlay::load_preview(next_id),
                            |(text, image_path)| {
                                Message::Clipboard(ClipboardMessage::PreviewLoaded {
                                    text,
                                    image_path,
                                })
                            },
                        ),
                        iced::widget::operation::scroll_to(
                            iced::widget::Id::new("clip_list"),
                            iced::widget::scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: (next_idx as f32 * 64.0 - 200.0).max(0.0),
                            },
                        ),
                    ]);
                }
                ClipboardMessage::SelectPrev => {
                    let ids = self.clipboard.filtered_ids();
                    if ids.is_empty() {
                        return Command::none();
                    }
                    let prev_idx = self
                        .clipboard
                        .selected_id
                        .and_then(|sel| ids.iter().position(|&id| id == sel))
                        .map(|p| (p + ids.len() - 1) % ids.len())
                        .unwrap_or(0);
                    let prev_id = ids[prev_idx];
                    eprintln!(
                        "[debug-nav] Up pressed. prev_idx: {}, prev_id: {}",
                        prev_idx, prev_id
                    );
                    self.clipboard.selected_id = Some(prev_id);
                    return Command::batch([
                        Command::perform(
                            ClipboardOverlay::load_preview(prev_id),
                            |(text, image_path)| {
                                Message::Clipboard(ClipboardMessage::PreviewLoaded {
                                    text,
                                    image_path,
                                })
                            },
                        ),
                        iced::widget::operation::scroll_to(
                            iced::widget::Id::new("clip_list"),
                            iced::widget::scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: (prev_idx as f32 * 64.0 - 200.0).max(0.0),
                            },
                        ),
                    ]);
                }
                ClipboardMessage::DeleteSelected => {
                    eprintln!(
                        "[clip] Delete key pressed. Selected ID: {:?}",
                        self.clipboard.selected_id
                    );
                    if let Some(id) = self.clipboard.selected_id {
                        let pos = self.clipboard.entries.iter().position(|e| e.id == id);

                        // Matches Lua logic: pipe ID + tab into cliphist delete
                        if let Ok(mut child) = std::process::Command::new("cliphist")
                            .arg("delete")
                            .stdin(std::process::Stdio::piped())
                            .spawn()
                        {
                            use std::io::Write;
                            if let Some(mut stdin) = child.stdin.take() {
                                let _ = writeln!(stdin, "{}\t", id);
                            }
                            let _ = child.wait();
                            eprintln!("[clip] Deleted ID {}", id);
                        }

                        if let Some(idx) = pos {
                            self.clipboard.entries.remove(idx);
                            if !self.clipboard.entries.is_empty() {
                                let next_idx = if idx >= self.clipboard.entries.len() {
                                    self.clipboard.entries.len() - 1
                                } else {
                                    idx
                                };
                                let next_id = self.clipboard.entries[next_idx].id;
                                self.clipboard.selected_id = Some(next_id);
                                return Command::batch([
                                    Command::perform(
                                        ClipboardOverlay::load_preview(next_id),
                                        |(text, image_path)| {
                                            Message::Clipboard(ClipboardMessage::PreviewLoaded {
                                                text,
                                                image_path,
                                            })
                                        },
                                    ),
                                    iced::widget::operation::scroll_to(
                                        iced::widget::Id::new("clip_list"),
                                        iced::widget::scrollable::AbsoluteOffset {
                                            x: 0.0,
                                            y: (next_idx as f32 * 64.0 - 200.0).max(0.0),
                                        },
                                    ),
                                ]);
                            } else {
                                self.clipboard.selected_id = None;
                                self.clipboard.preview_text.clear();
                                self.clipboard.preview_image_path = None;
                            }
                        }
                    }
                    return Command::none();
                }
                ClipboardMessage::RunOcr(id) => {
                    eprintln!("[ocr] RunOcr triggered for id {}", id);
                    let is_img = self
                        .clipboard
                        .entries
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| e.is_image)
                        .unwrap_or(false);
                    eprintln!("[ocr] entry found, is_image={}", is_img);
                    let tmp_path = std::env::temp_dir().join(format!("mbs-ocr-{}.png", id));

                    // Do OCR synchronously — it's fast (~1s) and we're about to close anyway
                    if let Ok(out) = std::process::Command::new("cliphist")
                        .args(["decode", &id.to_string()])
                        .output()
                    {
                        if !out.stdout.is_empty() && std::fs::write(&tmp_path, &out.stdout).is_ok()
                        {
                            // Clear LD_LIBRARY_PATH so tesseract uses its own
                            // nix-store glibc (2.43) instead of the system glibc (2.42).
                            // This prevents the 5.5.2-binary / 5.5.3-library mismatch.
                            let tess_out = std::process::Command::new("tesseract")
                                .args([tmp_path.to_str().unwrap(), "stdout"])
                                .env_remove("LD_LIBRARY_PATH")
                                .output();

                            if let Ok(tess_out) = tess_out {
                                let text =
                                    String::from_utf8_lossy(&tess_out.stdout).trim().to_string();
                                if !text.is_empty() {
                                    eprintln!("[ocr] {} chars extracted", text.len());
                                    if let Ok(mut child) = std::process::Command::new("wl-copy")
                                        .stdin(std::process::Stdio::piped())
                                        .spawn()
                                    {
                                        use std::io::Write;
                                        if let Some(mut stdin) = child.stdin.take() {
                                            let _ = stdin.write_all(text.as_bytes());
                                        }
                                        let _ = child.wait();
                                    }
                                } else {
                                    eprintln!(
                                        "[ocr] no text found — stderr: {}",
                                        String::from_utf8_lossy(&tess_out.stderr)
                                    );
                                }
                            }
                        }
                    }
                    let _ = std::fs::remove_file(&tmp_path);

                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::none();
                }
                ClipboardMessage::CopySelected => {
                    if let Some(id) = self.clipboard.selected_id {
                        if let Ok(out) = std::process::Command::new("cliphist")
                            .args(["decode", &id.to_string()])
                            .output()
                        {
                            if let Ok(mut child) = std::process::Command::new("wl-copy")
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                            {
                                use std::io::Write;
                                if let Some(mut stdin) = child.stdin.take() {
                                    let _ = stdin.write_all(&out.stdout);
                                }
                            }
                        }
                        if self.is_standalone {
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                    }
                }
                ClipboardMessage::SelectEntry(id) => {
                    self.clipboard.selected_id = Some(id);
                    return Command::perform(
                        ClipboardOverlay::load_preview(id),
                        |(text, image_path)| {
                            Message::Clipboard(ClipboardMessage::PreviewLoaded { text, image_path })
                        },
                    );
                }
                ClipboardMessage::PreviewLoaded { text, image_path } => {
                    self.clipboard.preview_text = text;
                    self.clipboard.preview_image_path = image_path;
                }
            },
            Message::Notes(msg) => match msg {
                NotesMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                NotesMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::notes::NOTES_INPUT_ID,
                    )
                    .map(Message::Notes);
                }
                _ => self.notes.update(msg),
            },
            Message::Pass(msg) => match msg {
                PassMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                PassMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::pass::PASS_SEARCH_ID,
                    )
                    .map(Message::Pass);
                }
                PassMessage::CopyKey(k) => {
                    if self.is_standalone {
                        // Block on copy so it completes before exit (no orphan tokio task)
                        let k = k.clone();
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            PassOverlay::copy_password(&k).await;
                        });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move {
                            PassOverlay::copy_password(&k).await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                _ => self.pass.update(msg),
            },
            Message::Power(msg) => match msg {
                PowerMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                _ => {
                    self.power.update(msg);
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
            },
            Message::Todo(msg) => match msg {
                TodoMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                TodoMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::todo::TODO_INPUT_ID,
                    )
                    .map(Message::Todo);
                }
                TodoMessage::EditSelected | TodoMessage::EditTask(_) => {
                    self.todo.update(msg);
                    // Refocus the input so the user can type immediately
                    // after clicking ✎ or pressing F2.
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::todo::TODO_INPUT_ID,
                    )
                    .map(Message::Todo);
                }
                TodoMessage::TaskSelectPrev | TodoMessage::TaskSelectNext => {
                    self.todo.update(msg);
                    if let Some(sel_id) = self.todo.selected_task_id {
                        // Scroll to selected task using row height of ~40px.
                        let visible: Vec<u64> = self
                            .todo
                            .db
                            .todos
                            .iter()
                            .filter(|t| t.category == self.todo.active_category)
                            .filter(|t| match self.todo.filter {
                                moon_beauty_shell::overlays::todo::TodoFilter::All => true,
                                moon_beauty_shell::overlays::todo::TodoFilter::Active => {
                                    !t.completed
                                }
                                moon_beauty_shell::overlays::todo::TodoFilter::Completed => {
                                    t.completed
                                }
                            })
                            .map(|t| t.id)
                            .collect();
                        let idx = visible.iter().position(|v| *v == sel_id).unwrap_or(0);
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("todo_list"),
                            iced::widget::operation::AbsoluteOffset {
                                x: 0.0,
                                y: (idx as f32 * 40.0 - 100.0).max(0.0),
                            },
                        );
                    }
                }
                _ => self.todo.update(msg),
            },
            Message::Reminders(msg) => match msg {
                ReminderMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                ReminderMessage::InitialFocus => {
                    // 100ms delayed retry — start on the task input
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID,
                    )
                    .map(Message::Reminders);
                }
                ReminderMessage::Refocus => {
                    // Enter in task input → jump to when input
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::reminders::REMINDER_WHEN_ID,
                    )
                    .map(Message::Reminders);
                }
                ReminderMessage::FocusNextField => {
                    // Tab: cycle Task → When → Task based on current field
                    self.reminders.cycle_focus_forward();
                    let target = if self.reminders.active_field == 0 {
                        moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID
                    } else {
                        moon_beauty_shell::overlays::reminders::REMINDER_WHEN_ID
                    };
                    return iced::widget::operation::focus(target).map(Message::Reminders);
                }
                ReminderMessage::FocusPrevField => {
                    self.reminders.cycle_focus_backward();
                    let target = if self.reminders.active_field == 0 {
                        moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID
                    } else {
                        moon_beauty_shell::overlays::reminders::REMINDER_WHEN_ID
                    };
                    return iced::widget::operation::focus(target).map(Message::Reminders);
                }
                ReminderMessage::ScrollUp
                | ReminderMessage::ScrollDown
                | ReminderMessage::SelectFirst
                | ReminderMessage::SelectLast
                | ReminderMessage::DeleteSelected => {
                    self.reminders.update(msg);
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("reminders_list"),
                        iced::widget::operation::AbsoluteOffset {
                            x: 0.0,
                            y: self.reminders.scroll_y,
                        },
                    );
                }
                _ => self.reminders.update(msg),
            },
            Message::Dict(msg) => match msg {
                DictMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                DictMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::dictionary::DICT_SEARCH_ID,
                    )
                    .map(Message::Dict);
                }
                DictMessage::SearchWord => {
                    let w = self.dict.word_query.clone();
                    // Check cache first
                    if let Some(cached) = self.dict.cache.get(&w.to_lowercase()).cloned() {
                        self.dict.update(DictMessage::DefinitionsLoaded(w, cached));
                        return Command::none();
                    }
                    return Command::perform(
                        async move { (w.clone(), DictionaryOverlay::lookup_word(&w).await) },
                        |(w, defs)| Message::Dict(DictMessage::DefinitionsLoaded(w, defs)),
                    );
                }
                DictMessage::CopyDefinition(def) => {
                    if self.is_standalone {
                        let def = def.clone();
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            let _ = tokio::process::Command::new("wl-copy")
                                .arg(&def)
                                .status()
                                .await;
                        });
                        std::process::exit(0);
                    }
                    return Command::perform(
                        async move {
                            let _ = tokio::process::Command::new("wl-copy")
                                .arg(&def)
                                .status()
                                .await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                _ => self.dict.update(msg),
            },
            Message::Unicode(msg) => match msg {
                UnicodeMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                UnicodeMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::unicode::UNICODE_INPUT_ID,
                    )
                    .map(Message::Unicode);
                }
                UnicodeMessage::CopySymbol(sym) => {
                    if self.is_standalone {
                        tokio::spawn(async move {
                            UnicodeOverlay::copy(sym).await;
                        });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move {
                            UnicodeOverlay::copy(sym).await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                UnicodeMessage::SelectNext
                | UnicodeMessage::SelectPrev
                | UnicodeMessage::SelectNextRow
                | UnicodeMessage::SelectPrevRow => {
                    self.unicode.update(msg);
                    // Auto-scroll to selected item. Grid is 5 columns,
                    // item height = 66 + 8 gap = 74. Scroll so selected
                    // is roughly centered vertically.
                    if let Some(idx) = self.unicode.selected_idx {
                        let row = idx / 5;
                        let y = (row as f32) * 74.0 - 200.0;
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("unicode_grid"),
                            iced::widget::operation::AbsoluteOffset {
                                x: 0.0,
                                y: y.max(0.0),
                            },
                        );
                    }
                }
                UnicodeMessage::CopySelected => {
                    self.unicode.update(msg);
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                _ => self.unicode.update(msg),
            },
            Message::Web(msg) => match msg {
                WebSearchMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                WebSearchMessage::ExecuteSearch => {
                    if let Some(url) = WebSearchOverlay::build_url(&self.web.query) {
                        if self.is_standalone {
                            tokio::spawn(async move {
                                WebSearchOverlay::launch_browser(&url).await;
                            });
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                        return Command::perform(
                            async move {
                                WebSearchOverlay::launch_browser(&url).await;
                            },
                            |_| Message::DismissActiveOverlay,
                        );
                    }
                }
                _ => self.web.update(msg),
            },
            Message::Annotator(msg) => {
                let is_text_press = matches!(msg, AnnotatorMessage::Pressed(_))
                    && self.annotator.tool == Tool::Text;
                match msg {
                    AnnotatorMessage::ToggleReveal => {
                        self.annotator.reveal_mode = !self.annotator.reveal_mode;
                        if !self.annotator.reveal_mode {
                            self.annotator.status =
                                "Draw · Enter saves & closes · ESC cancels".into();
                        } else if self.annotator.reveal_bytes.is_some() {
                            self.annotator.status = "Reveal: watermark amplified".into();
                        } else if let Some(source) = self.annotator.source_path.clone() {
                            match std::fs::read(&source)
                                .map_err(|e| e.to_string())
                                .and_then(|b| plate_watermark::reveal(&b))
                            {
                                Ok(bytes) => {
                                    self.annotator.reveal_handle = Some(
                                        iced::widget::image::Handle::from_bytes(bytes.clone()),
                                    );
                                    self.annotator.reveal_bytes = Some(bytes);
                                    self.annotator.status = "Reveal: watermark amplified".into();
                                }
                                Err(e) => {
                                    self.annotator.status = format!("Reveal failed: {e}");
                                    self.annotator.reveal_mode = false;
                                }
                            }
                        } else {
                            self.annotator.status = "Reveal: no source image".into();
                            self.annotator.reveal_mode = false;
                        }
                    }
                    AnnotatorMessage::Save => {
                        if self.annotator.reveal_mode {
                            if let Some(bytes) = self.annotator.reveal_bytes.clone() {
                                let save_dir = dirs::picture_dir()
                                    .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
                                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                                    .join("Screenshots");
                                let _ = std::fs::create_dir_all(&save_dir);
                                let time_str =
                                    chrono::Local::now().format("%Y-%m-%d %H-%M").to_string();
                                let out = save_dir.join(format!("Revealed {}.png", time_str));

                                match std::fs::write(&out, &bytes) {
                                    Ok(_) => {
                                        eprintln!("[annotator] saved revealed proof -> {:?}", out);

                                        // Copy the revealed image to clipboard and map its name
                                        if let Ok(mut child) = std::process::Command::new("wl-copy")
                                            .args(["-t", "image/png"])
                                            .stdin(std::process::Stdio::piped())
                                            .spawn()
                                        {
                                            use std::io::Write;
                                            if let Some(mut stdin) = child.stdin.take() {
                                                let _ = stdin.write_all(&bytes);
                                            }
                                            let _ = child.wait();

                                            let cache_dir = dirs::cache_dir()
                                                .unwrap_or_else(std::env::temp_dir);
                                            let map_path = cache_dir.join("mbs-clip-names.txt");
                                            let filename = out
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string();

                                            let mut existing_clip_ids =
                                                std::collections::HashSet::new();
                                            if let Ok(list_out) =
                                                std::process::Command::new("cliphist")
                                                    .args(["list"])
                                                    .output()
                                            {
                                                for line in
                                                    String::from_utf8_lossy(&list_out.stdout)
                                                        .lines()
                                                {
                                                    if let Some((id_str, _)) = line.split_once('\t')
                                                    {
                                                        if let Ok(id) = id_str.parse::<u64>() {
                                                            existing_clip_ids.insert(id);
                                                        }
                                                    }
                                                }
                                            }

                                            for _ in 0..20 {
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(50),
                                                );
                                                if let Ok(list_out) =
                                                    std::process::Command::new("cliphist")
                                                        .args(["list"])
                                                        .output()
                                                {
                                                    let text =
                                                        String::from_utf8_lossy(&list_out.stdout);
                                                    for line in text.lines() {
                                                        if let Some((id_str, _)) =
                                                            line.split_once('\t')
                                                        {
                                                            if let Ok(id) =
                                                                id_str.trim().parse::<u64>()
                                                            {
                                                                if !existing_clip_ids.contains(&id)
                                                                {
                                                                    let entry = format!(
                                                                        "{}\t{}\n",
                                                                        id, filename
                                                                    );
                                                                    if let Ok(mut f) =
                                                                        std::fs::OpenOptions::new()
                                                                            .create(true)
                                                                            .append(true)
                                                                            .open(&map_path)
                                                                    {
                                                                        let _ = f.write_all(
                                                                            entry.as_bytes(),
                                                                        );
                                                                    }
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        if self.is_standalone {
                                            std::process::exit(0);
                                        }
                                        self.active_overlay = None;
                                    }
                                    Err(e) => {
                                        self.annotator.status = format!("Reveal save failed: {e}")
                                    }
                                }
                            } else {
                                self.annotator.status = "Reveal not ready".into();
                            }
                        } else {
                            let source = self.annotator.source_path.clone();
                            let annotations = self.annotator.annotations.clone();
                            let watermark = if self.annotator.watermark_input.is_empty() {
                                None
                            } else {
                                Some(self.annotator.watermark_input.clone())
                            };
                            match source {
                                Some(p) => match save_annotated_image(p, annotations, watermark) {
                                    Ok(paths) => {
                                        eprintln!("[annotator] saved {} file(s)", paths.len());
                                        if self.is_standalone {
                                            std::process::exit(0);
                                        }
                                        self.active_overlay = None;
                                    }
                                    Err(e) => self.annotator.status = format!("Save failed: {e}"),
                                },
                                None => {
                                    self.annotator.status = "Save failed: no source image".into()
                                }
                            }
                        }
                    }
                    AnnotatorMessage::Cancel => {
                        if self.is_standalone {
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                    }
                    AnnotatorMessage::Saved(_) | AnnotatorMessage::RevealLoaded(_) => {}
                    other => self.annotator.update(other),
                }
                if is_text_press {
                    return Command::none();
                }
            }
            Message::Amogus(msg) => match msg {
                AmogusMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                AmogusMessage::TriggerScan => {
                    self.amogus.update(AmogusMessage::TriggerScan);
                    return Command::perform(AmogusOverlay::run_scan(), |cols| {
                        Message::Amogus(AmogusMessage::ScanFinished(cols))
                    });
                }
                _ => self.amogus.update(msg),
            },
            Message::Rng(msg) => match msg {
                RngMessage::Dismiss => {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
                _ => self.rng.update(msg),
            },
            Message::Email(msg) => match msg {
                EmailMessage::Dismiss => {
                    if self.email.is_composing {
                        let to = self.email.compose_to.clone();
                        let sub = self.email.compose_subject.clone();
                        let body = self.email.compose_body.text();
                        self.email.update(EmailMessage::CancelCompose);
                        return Command::perform(
                            async move {
                                moon_beauty_shell::libraries::backend::email_client::save_draft(
                                    &to, &sub, &body,
                                )
                                .await
                            },
                            |_| Message::Email(EmailMessage::DraftSaved),
                        );
                    } else {
                        if self.is_standalone {
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                    }
                }
                EmailMessage::SelectFolder(f) => {
                    self.email.update(EmailMessage::SelectFolder(f.clone()));
                    return Command::batch(vec![
                        Command::done(Message::Email(EmailMessage::SetLoading)),
                        Command::perform(
                            async move {
                                moon_beauty_shell::libraries::backend::email_client::fetch_envelopes(&f).await
                            },
                            |res| match res {
                                Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(envs)),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            },
                        ),
                    ]);
                }
                EmailMessage::SelectMail(id) => {
                    let folder = self.email.active_folder.clone();
                    self.email.update(EmailMessage::SelectMail(id.clone()));
                    return Command::perform(
                        async move {
                            moon_beauty_shell::libraries::backend::email_client::read_message(
                                &id, &folder,
                            )
                            .await
                        },
                        |body| Message::Email(EmailMessage::MessageLoaded(body)),
                    );
                }
                EmailMessage::SendMail => {
                    let to = self.email.compose_to.clone();
                    let sub = self.email.compose_subject.clone();
                    let body = self.email.compose_body.text().to_string();

                    let mut attachments = Vec::new();
                    for path in &self.email.compose_attachments {
                        eprintln!("[attach] reading {:?}", path);
                        if let (Some(name), Ok(data)) = (path.file_name(), std::fs::read(path)) {
                            eprintln!(
                                "[attach] attached {} ({} bytes)",
                                name.to_string_lossy(),
                                data.len()
                            );
                            attachments.push((name.to_string_lossy().to_string(), data));
                        } else {
                            eprintln!("[attach] FAILED to read {:?}", path);
                        }
                    }

                    eprintln!(
                        "[send] sending to={:?} subject={:?} attachments={}",
                        to,
                        sub,
                        attachments.len()
                    );
                    self.email.update(EmailMessage::SendMail);
                    return Command::perform(
                        async move {
                            moon_beauty_shell::libraries::backend::email_client::send_message(
                                &to,
                                &sub,
                                &body,
                                attachments,
                            )
                            .await
                        },
                        |res| match res {
                            Ok(_) => Message::Email(EmailMessage::MessageLoaded(
                                moon_beauty_shell::libraries::backend::email_client::ParsedEmail {
                                    text: "✓ Message sent.".into(),
                                    images: Vec::new(),
                                    attachments: Vec::new(),
                                },
                            )),
                            Err(e) => Message::Email(EmailMessage::MessageLoaded(
                                moon_beauty_shell::libraries::backend::email_client::ParsedEmail {
                                    text: format!("✗ Send failed: {}", e),
                                    images: Vec::new(),
                                    attachments: Vec::new(),
                                },
                            )),
                        },
                    );
                }
                EmailMessage::EnvelopesLoaded(envs) => {
                    self.email.update(EmailMessage::EnvelopesLoaded(envs));
                    let folder = self.email.active_folder.clone();
                    return Command::perform(
                        async move {
                            moon_beauty_shell::libraries::backend::email_client::fetch_from_network(
                                &folder,
                            )
                            .await
                        },
                        |res| match res {
                            Ok(envs) => Message::Email(EmailMessage::NetworkEnvelopesLoaded(envs)),
                            Err(_) => Message::Email(EmailMessage::SyncDone),
                        },
                    );
                }
                EmailMessage::NetworkEnvelopesLoaded(envs) => {
                    self.email
                        .update(EmailMessage::EnvelopesLoaded(envs.clone()));
                    moon_beauty_shell::libraries::backend::email_client::save_index(
                        &self.email.active_folder,
                        &envs,
                    );
                    let folder = self.email.active_folder.clone();
                    return Command::perform(
                        async move {
                            moon_beauty_shell::libraries::backend::email_client::sync_bodies_background(folder).await
                        },
                        |_| Message::Email(EmailMessage::SyncDone),
                    );
                }
                EmailMessage::SyncDone => {}
                EmailMessage::SelectMailNext | EmailMessage::SelectMailPrev => {
                    if self.email.is_composing && !self.email.get_autocomplete_matches().is_empty()
                    {
                        let target = match msg {
                            EmailMessage::SelectMailNext => EmailMessage::SelectAutocompleteNext,
                            _ => EmailMessage::SelectAutocompletePrev,
                        };
                        self.email.update(target);
                    } else if !self.email.is_composing && !self.email.is_logging_in {
                        self.email.update(msg);
                        return self.update(Message::Email(EmailMessage::OpenSelected));
                    }
                }
                EmailMessage::SelectFolderNext | EmailMessage::SelectFolderPrev => {
                    if !self.email.is_composing && !self.email.is_logging_in {
                        let old_folder = self.email.active_folder.clone();
                        self.email.update(msg);
                        if self.email.active_folder != old_folder {
                            let f = self.email.active_folder.clone();
                            return Command::batch(vec![
                                Command::done(Message::Email(EmailMessage::SetLoading)),
                                Command::perform(
                                    async move {
                                        moon_beauty_shell::libraries::backend::email_client::fetch_envelopes(&f).await
                                    },
                                    |res| match res {
                                        Ok(envs) => {
                                            Message::Email(EmailMessage::EnvelopesLoaded(envs))
                                        }
                                        Err(e) => Message::Email(EmailMessage::Error(e)),
                                    },
                                ),
                            ]);
                        }
                    }
                }
                EmailMessage::OpenSelected => {
                    if !self.email.is_composing {
                        if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                            let id = mail.id.clone();
                            let folder = self.email.active_folder.clone();
                            self.email.update(EmailMessage::SelectMail(id.clone()));
                            return Command::perform(
                                async move {
                                    moon_beauty_shell::libraries::backend::email_client::read_message(&id, &folder).await
                                },
                                |body| Message::Email(EmailMessage::MessageLoaded(body)),
                            );
                        }
                    }
                }
                EmailMessage::ToggleStar => {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        let id = mail.id.clone();
                        let folder = self.email.active_folder.clone();
                        let star = !mail.is_starred;
                        self.email.update(EmailMessage::ToggleStar);
                        return Command::perform(
                            async move {
                                moon_beauty_shell::libraries::backend::email_client::toggle_star(
                                    &id, &folder, star,
                                )
                                .await
                            },
                            |res| match res {
                                Ok(_) => Message::Email(EmailMessage::SyncDone),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            },
                        );
                    }
                }
                EmailMessage::DeleteSelected => {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        if mail.is_starred {
                            self.email.imap_error =
                                Some("Cannot delete a starred email. Unstar it first.".into());
                        } else {
                            let id = mail.id.clone();
                            let mail_clone = mail.clone();
                            let folder = self.email.active_folder.clone();

                            self.email.envelopes.remove(self.email.selected_index);
                            if self.email.selected_index >= self.email.envelopes.len()
                                && !self.email.envelopes.is_empty()
                            {
                                self.email.selected_index = self.email.envelopes.len() - 1;
                            }
                            self.email.parsed_email = None;
                            self.email.active_mail_id = None;

                            moon_beauty_shell::libraries::backend::email_client::save_index(
                                &folder,
                                &self.email.envelopes,
                            );

                            let is_trash = folder == "Trash";

                            if !is_trash {
                                let mut trash_envs =
                                    moon_beauty_shell::libraries::backend::email_client::load_index(
                                        "Trash",
                                    );
                                trash_envs.insert(0, mail_clone);
                                moon_beauty_shell::libraries::backend::email_client::save_index(
                                    "Trash",
                                    &trash_envs,
                                );
                            }

                            return Command::perform(
                                async move {
                                    if is_trash {
                                        moon_beauty_shell::libraries::backend::email_client::delete_message(&id, &folder).await
                                    } else {
                                        moon_beauty_shell::libraries::backend::email_client::move_to_trash(&id, &folder).await
                                    }
                                },
                                |res| match res {
                                    Ok(_) => Message::Email(EmailMessage::SyncDone),
                                    Err(e) => Message::Email(EmailMessage::Error(e)),
                                },
                            );
                        }
                    }
                }
                EmailMessage::SaveContact => {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        let email = mail.sender_email.clone();
                        if !email.is_empty() {
                            return Command::perform(
                                async move {
                                    moon_beauty_shell::libraries::backend::email_client::save_contact(&email).await
                                },
                                |res| match res {
                                    Ok(_) => Message::Email(EmailMessage::DraftSaved),
                                    Err(e) => Message::Email(EmailMessage::Error(e)),
                                },
                            );
                        }
                    }
                }
                EmailMessage::OpenInBrowser
                | EmailMessage::GetAppPassword
                | EmailMessage::SelectContact(_)
                | EmailMessage::SelectAutocompleteNext
                | EmailMessage::SelectAutocompletePrev
                | EmailMessage::ConfirmAutocomplete
                | EmailMessage::StartAddFolder
                | EmailMessage::AddFolderChanged(_)
                | EmailMessage::SubmitAddFolder
                | EmailMessage::RemoveFolder(_)
                | EmailMessage::AttachmentsPicked(_)
                | EmailMessage::RemoveAttachment(_)
                | EmailMessage::ManualAttachChanged(_)
                | EmailMessage::SubmitManualAttach => {
                    self.email.update(msg);
                }
                EmailMessage::AddAttachment => {
                    return Command::perform(
                        async {
                            let files = rfd::AsyncFileDialog::new()
                                .add_filter("All Files", &["*"])
                                .pick_files()
                                .await;
                            let paths = files
                                .map(|f| {
                                    f.into_iter()
                                        .map(|file| file.path().to_path_buf())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            eprintln!("[attach] rfd picked: {:?}", paths);
                            paths
                        },
                        |paths| Message::Email(EmailMessage::AttachmentsPicked(paths)),
                    );
                }
                EmailMessage::OpenComposer => {
                    self.email.update(msg);
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::email::COMPOSE_TO_ID,
                    )
                    .map(Message::Email);
                }
                EmailMessage::SaveLogin => {
                    let cfg = moon_beauty_shell::libraries::backend::email_client::EmailConfig {
                        email: self.email.login_email.clone(),
                        password: Some(self.email.login_password.clone()),
                        password_cmd: None,
                        imap_host: self.email.login_imap.clone(),
                        imap_port: 993,
                        smtp_host: self.email.login_smtp.clone(),
                        smtp_port: 465,
                        sops_key: if self.email.login_sops_key.trim().is_empty() {
                            None
                        } else {
                            Some(self.email.login_sops_key.clone())
                        },
                    };
                    let use_sops = self.email.use_sops;
                    match moon_beauty_shell::libraries::backend::email_client::save_config(
                        &cfg, use_sops,
                    ) {
                        Ok(_) => {
                            self.email.is_logging_in = false;
                            self.email.login_error = None;
                            return Command::batch(vec![
                                Command::done(Message::Email(EmailMessage::SetLoading)),
                                Command::perform(
                                    async move {
                                        moon_beauty_shell::libraries::backend::email_client::fetch_envelopes("INBOX").await
                                    },
                                    |res| match res {
                                        Ok(envs) => {
                                            Message::Email(EmailMessage::EnvelopesLoaded(envs))
                                        }
                                        Err(e) => Message::Email(EmailMessage::Error(e)),
                                    },
                                ),
                            ]);
                        }
                        Err(e) => {
                            self.email.login_error = Some(e);
                        }
                    }
                }
                _ => self.email.update(msg),
            },
            Message::Notif(msg) => match msg {
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
                    // Use the accurate scroll offset (computed from actual
                    // row heights via history_row_height) rather than a
                    // fixed-pixel guess.
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
                NotifMessage::ActivateToast(id) => {
                    if let Some(t) = self
                        .notifs
                        .active_toasts
                        .iter()
                        .find(|t| t.id == id)
                        .cloned()
                    {
                        let app = t.app_name.clone();
                        let entry = t.desktop_entry.clone();
                        let nid = t.id;
                        self.notifs.update(NotifMessage::DismissToast(id));
                        std::thread::spawn(move || {
                            moon_beauty_shell::overlays::notifications::emit_action_invoked(
                                nid, "default",
                            );
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            let matches = notification_candidates(&app, entry.as_deref());
                            if !is_app_focused(&matches) {
                                let ok = focus_notification_app(&app, entry.as_deref());
                                eprintln!(
                                    "[activate-toast] fallback app={:?} entry={:?} ok={}",
                                    app, entry, ok
                                );
                            } else {
                                eprintln!("[activate-toast] client self-focused app={:?}", app);
                            }
                        });
                    }
                }
                _ => self.notifs.update(msg),
            },
            Message::Settings(msg) => {
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
            }
            Message::Lock(msg) => match msg {
                LockMessage::KeyPress(key) => {
                    self.lockscreen.forward_key(&key);
                    return Command::none();
                }
                LockMessage::PollIpc => {
                    if let Some(effect) = self.lockscreen.drain_ipc() {
                        if let LockMessage::AuthSuccess = effect {
                            eprintln!("[lock] coordinator signaled success — exiting");
                            std::process::exit(0);
                        }
                    }
                    return Command::none();
                }
                _ => {}
            },
            Message::CollapseTray => {
                self.close_tray();
            }
            Message::TrayWatchdogTick => {
                // 1) Timeout check
                if self.tray_expanded
                    && self.settings.tray_timeout_sec > 0
                    && self.tray_opened_at.elapsed().as_secs()
                        >= self.settings.tray_timeout_sec as u64
                {
                    eprintln!(
                        "[tray-watchdog] elapsed={}s >= timeout={}s, closing (settings.tray_width={})",
                        self.tray_opened_at.elapsed().as_secs(),
                        self.settings.tray_timeout_sec,
                        self.settings.tray_width
                    );
                    self.close_tray();
                }

                // 2) Animation step. Note: the completion check is OUTSIDE
                //    the `elapsed < duration` guard so the snap-to-target
                //    ALWAYS runs on the first tick past duration. Previous
                //    version froze at width=89.7 because once elapsed
                //    reached 180ms, the guard `elapsed < duration` became
                //    false and the inner `if t >= 1.0 { snap }` never ran.
                if (self.tray_anim_width - self.tray_target_width).abs() > 0.01 {
                    let elapsed_ms = self.tray_anim_started_at.elapsed().as_millis() as u64;
                    let dur = self.tray_anim_duration_ms.max(1);
                    let t = (elapsed_ms as f32 / dur as f32).clamp(0.0, 1.0);
                    let expanding = self.tray_target_width > self.tray_anim_start_width;
                    let eased = if expanding {
                        1.0 - (1.0 - t).powi(3)
                    } else {
                        t * t * t
                    };
                    let delta = self.tray_target_width - self.tray_anim_start_width;
                    self.tray_anim_width = self.tray_anim_start_width + delta * eased;
                    // Force-snap when past duration OR t is at ceiling.
                    if elapsed_ms >= dur || t >= 1.0 {
                        self.tray_anim_width = self.tray_target_width;
                    }
                    eprintln!(
                        "[tray-anim] elapsed={}ms t={:.2} width={:.1} target={:.1}",
                        elapsed_ms, t, self.tray_anim_width, self.tray_target_width
                    );
                }
            }
            Message::TrayAnimTick => {
                let elapsed_ms = self.tray_anim_started_at.elapsed().as_millis() as f32;
                let t = (elapsed_ms / self.tray_anim_duration_ms.max(1) as f32).clamp(0.0, 1.0);

                // Expand: ease-out-cubic (fast start, smooth settle)
                // Collapse: ease-in-cubic (slow start, snap finish)
                let expanding = self.tray_target_width > self.tray_anim_start_width;
                let eased = if expanding {
                    1.0 - (1.0 - t).powi(3)
                } else {
                    t * t * t
                };

                let delta = self.tray_target_width - self.tray_anim_start_width;
                self.tray_anim_width = self.tray_anim_start_width + delta * eased;

                if t >= 1.0 {
                    self.tray_anim_width = self.tray_target_width;
                }
            }
            Message::FetchTrayItems => {
                // Background refresh so tray_items is populated before
                // the user ever clicks the tray capsule. Eliminates the
                // 0-item → N-item re-animation on first open.
                return Command::perform(
                    moon_beauty_shell::bar::tray::watcher::fetch_tray_items(),
                    Message::TrayItemsLoaded,
                );
            }
            Message::TrayItemsLoaded(items) => {
                self.tray_items = items;
                if self.tray_expanded {
                    let new_target =
                        moon_beauty_shell::bar::capsules::compute_expanded_visible_width(
                            &self.settings,
                            self.tray_items.len(),
                        );
                    // Re-anchor if target changed mid-animation
                    if (new_target - self.tray_target_width).abs() > 1.0 {
                        self.tray_anim_start_width = self.tray_anim_width;
                        self.tray_anim_started_at = std::time::Instant::now();
                        self.tray_target_width = new_target;
                    }
                }
            }
            Message::DismissActiveOverlay => {
                // Todo edit mode: Escape cancels the edit rather than
                // closing the overlay.
                if matches!(self.active_overlay, Some(OverlayTarget::Todo))
                    && self.todo.editing_task_id.is_some()
                {
                    self.todo
                        .update(moon_beauty_shell::overlays::todo::TodoMessage::CancelEdit);
                    return Command::none();
                }
                // Reminders: Escape clears the current input field, then the
                // list selection, then finally dismisses. Prevents a mistyped
                // reminder from wiping out the whole overlay on Esc.
                if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
                    if !self.reminders.task_text.is_empty() {
                        self.reminders.task_text.clear();
                        return Command::none();
                    }
                    if !self.reminders.when_text.is_empty() {
                        self.reminders.when_text.clear();
                        return Command::none();
                    }
                    if self.reminders.selected_idx.is_some() {
                        self.reminders.selected_idx = None;
                        return Command::none();
                    }
                }
                // Persist if we're closing the Settings overlay via Esc.
                if matches!(self.active_overlay, Some(OverlayTarget::Settings)) {
                    let _ = self.settings.save();
                }
                if self.is_standalone {
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    });
                    return Command::none();
                }
                self.active_overlay = None;
            }

            Message::CheckLockscreenAlive => {
                // Only the primary bar watches the lock.
                if self.is_notif_surface || self.is_standalone {
                    return Command::none();
                }

                // Pause detection: session was suspended (TTY switch, sleep).
                // Layer surfaces held by lock children are dropped by sway
                // on output detach WITHOUT sending Closed to the client —
                // from the child's perspective nothing happened, but its
                // wl_surface is no longer composited and keyboard grab is
                // gone. Detect via elapsed-time gap and force a full respawn.
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
                //   success() == true  → coordinator exited cleanly. The only
                //     code path that does this is auth success: coordinator
                //     broadcasts Success, kills children, exits(0). Respawning
                //     here would immediately re-lock the screen — that's the
                //     "locks as soon as I unlock it" bug.
                //   success() == false → crash (nonzero exit or signal kill).
                //     Respawn to keep the screen locked.
                match self.standalone_child.as_mut() {
                    Some((_, child)) => match child.try_wait() {
                        Ok(None) => {
                            // Still running.
                            return Command::none();
                        }
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
                            return Command::none();
                        }
                        Err(e) => {
                            eprintln!("[lockscreen-watchdog] try_wait error ({e}), respawning");
                            self.standalone_child = None;
                            self.spawn_standalone(OverlayTarget::Lockscreen);
                            return Command::none();
                        }
                    },
                    None => return Command::none(),
                }
            }
            Message::Noop => {}
            Message::SetToastInputRegion(_) => {
                // Intercepted by `try_into` — never reaches update.
            }
            Message::CloseShell => {
                std::process::exit(0);
            }
        }
        Command::none()
    }
}
