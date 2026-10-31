#![allow(clippy::collapsible_match)]
use super::*;

mod music;
mod notifications;
mod reminders;
mod email;
mod lockscreen;
mod clipboard;
mod settings;
mod overlays;

impl MoonBeautyShellApp {
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            // Message::WindowResized removed (problem 07)
            Message::ResizeNotifSurface(w, h) => {
                eprintln!(
                    "[resize] update caught ({}, {}) — try_into NOT called",
                    w, h
                );
            }
            Message::IdleTick => {
                let c1 = self.update(Message::TickClock);
                let c2 = self.update(Message::TickHardware);
                let c3 = self.update(Message::PollNotifs);
                let c4 = self.update(Message::CheckLockscreenAlive);
                return Command::batch(vec![c1, c2, c3, c4]);
            }
            Message::PollNotifs => return self.handle_poll_notifs(),
            Message::TickClock => {
                self.telemetry.bar_time = ClockBackend::get_bar_time();
                return self.check_reminder_alerts();
            }
            Message::TickToasts => return self.handle_tick_toasts(),
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
            Message::TickEmailSync => {
                // Only the primary bar and the standalone email overlay
                // sync IMAP. The notif-surface doesn't need email data and
                // duplicating the sync doubles IMAP traffic + log noise.
                if self.is_notif_surface {
                    return Command::none();
                }
                if moon_beauty_shell::libraries::backend::email::client::config_exists() {
                    eprintln!("[sync] Boot/Timer triggered email sync");
                    let folder = "INBOX".to_string();
                    return Command::perform(
                        async move {
                            let old_envs =
                                moon_beauty_shell::libraries::backend::email::client::load_index(
                                    &folder,
                                );
                            if let Ok(new_envs) = moon_beauty_shell::libraries::backend::email::client::fetch_from_network(&folder).await {
                                moon_beauty_shell::libraries::backend::email::client::save_index(&folder, &new_envs);
                                moon_beauty_shell::libraries::backend::email::client::sync_bodies_background(folder).await;
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
                        let _ = moon_beauty_shell::libraries::process::spawn_and_reap(
                            std::process::Command::new("notify-send").args([
                                "-a",
                                "Email",
                                "--",
                                &env.sender,
                                &env.subject,
                            ]),
                        );

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
                let _ = moon_beauty_shell::libraries::process::private_write_file(
                    &dedup_path,
                    sorted.join("\n").as_bytes(),
                );
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
                        let (cpu, ram) = SystemCollector::collect_all().await;
                        let gpu = SystemCollector::collect_gpu_by_card_id(&gpu_id).await;
                        let audio = AudioBackend::get_sink().await;
                        let mic = AudioBackend::get_source().await;
                        // Finding 74: MPD-priority-over-MPRIS. Prefer the
                        // dedicated music daemon over incidental browser
                        // media when both are Playing.
                        let (mpris, _source) =
                            moon_beauty_shell::libraries::backend::poll_track().await;
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
                IpcCommand::DismissLatestNotif => return self.dismiss_latest_notif(),
                IpcCommand::JumpToLatestNotif => return self.jump_to_latest_notif(),
                IpcCommand::ToggleHistory => return self.toggle_history(),
                IpcCommand::ReloadSettings => {
                    self.settings = ShellSettings::load();
                    self.theme = ShellTheme::from_settings(&self.settings);
                }
                IpcCommand::TestNotification => return self.test_notification(),
                IpcCommand::IncomingNotif { .. } => {}
                IpcCommand::MusicCycle => return self.music_cycle(),
                IpcCommand::MusicNext => return self.music_next_track_ipc(),
                IpcCommand::MusicPrevious => return self.music_previous_track_ipc(),
                IpcCommand::MusicPlayPause => return self.music_toggle_play_pause_ipc(),
                IpcCommand::Ping => {}
            },
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
                                let in_left = self.settings.bar_left.contains(&id);
                                let in_center = self.settings.bar_center.contains(&id);
                                let in_right = self.settings.bar_right.contains(&id);
                                // Finding 78: right-side capsules use capsule_center_x, not
                                // last_cursor_x. Right-anchored math is
                                //   margin_right = sw - cx - half_cap + adjust_right
                                // which assumes cx = capsule center. If cx = click_x (near
                                // capsule right edge), tooltip lands half_cap - adjust past
                                // capsule_right — off-screen at larger font sizes. Left-anchored
                                // works because the clamp catches the (cx - half_cap) overshoot.
                                let cx = if id == "clock" {
                                    capsule_center_x(&self.settings, sw, &id)
                                } else if recent_click && self.last_cursor_x > 0.0 && in_left {
                                    self.last_cursor_x as i32
                                } else {
                                    capsule_center_x(&self.settings, sw, &id)
                                };
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

                                // Finding 76: per-capsule tooltip bias.
                                let (bias_l, bias_r) =
                                    moon_beauty_shell::bar::capsules::tooltip_bias(
                                        &id,
                                        &self.theme,
                                    );
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
                                    offset_left_extra: bias_l,
                                    offset_right_extra: bias_r,
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
                        CapsuleAction::TogglePlayPause => return self.music_toggle_play_pause(),
                        CapsuleAction::NextTrack => return self.music_next_track(),
                        CapsuleAction::PreviousTrack => return self.music_previous_track(),
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
                        CapsuleAction::ToggleNotificationDnd => return self.toggle_notification_dnd(),
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
                                        let tmp_path =
                                            moon_beauty_shell::libraries::process::mbs_unique_tmp(
                                                "mbs-tray-menu",
                                                "json",
                                            );
                                        if let Err(e) =
                                            moon_beauty_shell::libraries::process::private_write_new(
                                                &tmp_path,
                                                json.as_bytes(),
                                            )
                                        {
                                            eprintln!("[tray-menu] write failed: {e}");
                                            return;
                                        }
                                        let exe = resolve_exe();
                                        let h = (items.len() as u32 * 36).clamp(100, 600) + 40;
                                        let _ = std::process::Command::new(exe)
                                            .args([
                                                "--tray-context-menu",
                                                tmp_path.to_str().unwrap_or(""),
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
            Message::Calc(msg) => return self.handle_calc(msg),
            Message::Clipboard(msg) => return self.handle_clipboard(msg),
            Message::Notes(msg) => return self.handle_notes(msg),
            Message::Pass(msg) => return self.handle_pass(msg),
            Message::Power(msg) => return self.handle_power(msg),
            Message::Todo(msg) => return self.handle_todo(msg),
            Message::Reminders(msg) => return self.handle_reminders(msg),
            Message::Dict(msg) => return self.handle_dict(msg),
            Message::Unicode(msg) => return self.handle_unicode(msg),
            Message::Web(msg) => return self.handle_web(msg),
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
                                .and_then(|b| screenshot::watermark::reveal(&b))
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
                                let out = save_dir.join(format!("Revealed {}.jxl", time_str));

                                // reveal() emits PNG bytes; encode to JXL for disk.
                                let tmp_reveal = save_dir.join(format!(
                                    ".mbs-reveal-{}.png",
                                    std::process::id()
                                ));
                                let write_ok = std::fs::write(&tmp_reveal, &bytes).is_ok();
                                let convert_ok = write_ok
                                    && moon_beauty_shell::overlays::screenshot::annotator::png_to_jxl(
                                        &tmp_reveal, &out,
                                    )
                                    .is_ok();
                                let _ = std::fs::remove_file(&tmp_reveal);
                                let jxl_bytes_opt = if convert_ok {
                                    std::fs::read(&out).ok()
                                } else {
                                    None
                                };

                                match jxl_bytes_opt {
                                    Some(_jxl_bytes) => {
                                        eprintln!("[annotator] saved revealed proof -> {:?}", out);

                                        // Copy the revealed JXL bytes and map its name
                                        // Same reasoning as save_annotated_image:
                                        // pipe the reveal PNG (`bytes`), not the JXL.
                                        // PNG is what client apps request on Ctrl+V.
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
                                    None => {
                                        self.annotator.status =
                                            "Reveal save failed: JXL encode".to_string()
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
            Message::Amogus(msg) => return self.handle_amogus(msg),
            Message::Rng(msg) => return self.handle_rng(msg),
            Message::Email(msg) => return self.handle_email(msg),
            Message::Notif(msg) => return self.handle_notif(msg),
            Message::Settings(msg) => return self.handle_settings(msg),
            Message::Lock(msg) => return self.handle_lock(msg),
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

            Message::CheckLockscreenAlive => return self.check_lockscreen_alive(),
            Message::MusicCycleDone(new_state) => return self.handle_music_cycle_done(new_state),
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
