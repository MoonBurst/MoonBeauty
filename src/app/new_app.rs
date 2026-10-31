use super::*;

impl MoonBeautyShellApp {
    pub fn new_app() -> (Self, Command<Message>) {
        moonbeauty::libraries::process::tighten_existing_runtime_state();
        let args: Vec<String> = std::env::args().collect();
        let standalone_target = detect_standalone_overlay(&args);
        if let Some(idx) = args.iter().position(|a| a == "--tray-context-menu") {
            let json_path = args.get(idx + 1).cloned().unwrap_or_default();
            let bus = args.get(idx + 2).cloned().unwrap_or_default();
            let menu_path = args.get(idx + 3).cloned().unwrap_or_default();
            let w = args
                .get(idx + 4)
                .and_then(|s| s.parse().ok())
                .unwrap_or(250u32);
            let h = args
                .get(idx + 5)
                .and_then(|s| s.parse().ok())
                .unwrap_or(400u32);
            if let Ok(json) = std::fs::read_to_string(&json_path) {
                if let Ok(items) =
                    serde_json::from_str::<Vec<moonbeauty::bar::tray::TrayMenuItem>>(&json)
                {
                    let _ = std::fs::remove_file(&json_path);
                    std::process::exit(
                        match moonbeauty::bar::tray::context::run(
                            items, bus, menu_path, w, h,
                        ) {
                            Ok(_) => 0,
                            Err(_) => 1,
                        },
                    );
                }
            }
            std::process::exit(1);
        }

        let is_notif_surface = detect_notif_surface(&args);
        let is_standalone = standalone_target.is_some();
        let settings = ShellSettings::load();
        let _tray_width_px = settings.tray_width as f32;
        eprintln!(
            "[startup] tray_width={} tray_timeout={}s",
            settings.tray_width, settings.tray_timeout_sec
        );
        let theme = ShellTheme::from_settings(&settings);

        let (ipc_tx, ipc_rx) = tokio::sync::mpsc::unbounded_channel::<IpcCommand>();
        let _ = crate::app::subscription::IPC_RX
            .set(std::sync::Arc::new(tokio::sync::Mutex::new(ipc_rx)));
        if standalone_target.is_none() && !is_notif_surface {
            std::thread::spawn(move || {
                use std::io::Read;
                use std::os::unix::net::UnixListener;
                let path = moonbeauty::libraries::ipc::get_socket_path();
                let _ = std::fs::remove_file(&path);
                let listener = match UnixListener::bind(&path) {
                    Ok(l) => {
                        eprintln!("[ipc] listener bound (sync thread)");
                        l
                    }
                    Err(e) => {
                        eprintln!("[ipc] bind failed: {e}");
                        return;
                    }
                };
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // Finding 49: 1s timeout bounds per-connection
                            // accept-loop block. Was 3s; a stalled client
                            // held the accept loop for 3s per connection.
                            let _ =
                                stream.set_read_timeout(Some(std::time::Duration::from_secs(1)));
                            // Finding 37: cap read at 4 MB.
                            let mut buf = Vec::new();
                            let mut limited = std::io::Read::take(&mut stream, 4 * 1024 * 1024);
                            if let Err(e) = limited.read_to_end(&mut buf) {
                                eprintln!("[ipc] read error: {e}");
                                continue;
                            }
                            match serde_json::from_slice::<IpcCommand>(&buf) {
                                Ok(cmd) => {
                                    eprintln!("[ipc] accepted: {:?}", cmd);
                                    if ipc_tx.send(cmd).is_err() {
                                        eprintln!("[ipc] channel closed");
                                        break;
                                    }
                                }
                                Err(e) => eprintln!("[ipc] deserialize error: {e}"),
                            }
                        }
                        Err(e) => eprintln!("[ipc] accept error: {e}"),
                    }
                }
            });
        }

        // Spawn exactly one wl-paste watcher. Kill any orphaned watcher
        // from previous runs first — the shell pkill pattern anchors on
        // "moon-beauty-shell$", but the watcher cmdline ends with
        // "--clipboard-ingest" and so escapes it, causing N-duplicate
        // clipboard entries after N bar restarts.
        if standalone_target.is_none() && !is_notif_surface {
            let _ = std::process::Command::new("pkill")
                .args(["-f", "wl-paste --watch.*moon-beauty-shell"])
                .status();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let exe = crate::resolve_exe();
            let exe_str = exe.to_string_lossy().to_string();
            let _ = std::process::Command::new("wl-paste")
                .args(["--watch", &exe_str, "--clipboard-ingest"])
                .spawn();
        }

        let notif_rx = if is_notif_surface {
            // Finding 55: bounded (256). notify-send can be called at
            // arbitrary rate by any same-user process; unbounded channel
            // OOMs the shell under load. 256 is 64× the visible toast cap.
            let (tx, rx) = std::sync::mpsc::sync_channel(256);
            moonbeauty::overlays::notifications::spawn(tx);
            moonbeauty::bar::tray::watcher::spawn();
            Some(rx)
        } else {
            None
        };

        let notif_screen_at_startup = settings.notif_screen_name.clone();
        let app = Self {
            settings_ui: if is_notif_surface
                || (is_standalone && !matches!(standalone_target, Some(OverlayTarget::Settings)))
            {
                SettingsOverlay::empty()
            } else {
                SettingsOverlay::new(&settings)
            },
            settings,
            theme,
            telemetry: UnifiedTelemetry {
                cpu: Default::default(),
                gpu: Default::default(),
                gpu_selected_idx: 0,
                ram: Default::default(),
                net: Default::default(),
                battery: Default::default(),
                audio: Default::default(),
                mic: Default::default(),
                mpris: None,
                weather: None,
                podman: Default::default(),
                borg: Default::default(),
                bar_time: ClockBackend::get_bar_time(),
            },
            active_overlay: standalone_target.clone(),
            weather_tooltip_child: None,
            is_standalone,
            system_collector: SystemCollector::new(),
            standalone_child: None,
            is_notif_surface,
            notif_rx,
            settings_json_mtime: std::fs::metadata(ShellSettings::config_path())
                .ok()
                .and_then(|m| m.modified().ok()),
            // window_w removed (problem 07)
            notif_screen_at_startup,
            notif_surface_size: if is_notif_surface { (1, 1) } else { (0, 0) },
            tray_expanded: false,
            tray_items: Vec::new(),
            tray_anim_width: _tray_width_px,
            tray_target_width: _tray_width_px,
            tray_anim_start_width: _tray_width_px,
            tray_anim_started_at: std::time::Instant::now(),
            tray_anim_duration_ms: 220,
            tray_timeout_gen: 0,
            tray_opened_at: std::time::Instant::now(),
            detected_gpus: if is_notif_surface || is_standalone {
                Vec::new()
            } else {
                SystemCollector::list_gpus_sync()
            },

            launcher: if is_standalone
                && !matches!(standalone_target, Some(OverlayTarget::Launcher))
            {
                // Cheap constructor: no desktop file scan. Saves 200-400ms
                // on every non-launcher overlay spawn (clipboard, notes,
                // email, todo, reminders, calc, pass, unicode, websearch,
                // annotator, rng, amogus, settings, notifications, etc).
                AppLauncher::new_empty()
            } else {
                AppLauncher::new()
            },
            calc: {
                if let Ok(expr) = std::env::var("MBS_CALC_EXPR") {
                    CalculatorOverlay::new_with_input(&expr)
                } else {
                    CalculatorOverlay::new()
                }
            },
            clipboard: ClipboardOverlay::new(),
            notes: if is_standalone && !matches!(standalone_target, Some(OverlayTarget::Notes)) {
                NotesOverlay::empty()
            } else {
                NotesOverlay::new()
            },
            pass: if is_standalone && !matches!(standalone_target, Some(OverlayTarget::Pass)) {
                PassOverlay::empty()
            } else {
                PassOverlay::new()
            },
            power: PowerOverlay::new(),
            todo: if is_standalone && !matches!(standalone_target, Some(OverlayTarget::Todo)) {
                TodoOverlay::empty()
            } else {
                TodoOverlay::new()
            },
            reminders: if is_standalone
                && !matches!(standalone_target, Some(OverlayTarget::Reminders))
            {
                RemindersOverlay::empty()
            } else {
                RemindersOverlay::new()
            },
            dict: if is_standalone && !matches!(standalone_target, Some(OverlayTarget::Dictionary))
            {
                DictionaryOverlay::empty()
            } else {
                DictionaryOverlay::new()
            },
            unicode: UnicodeOverlay::new(),
            web: WebSearchOverlay::new(),
            amogus: AmogusOverlay::new(),
            annotator: {
                let mut a = AnnotatorOverlay::new();
                if let Ok(p) = std::env::var("MBS_ANNOTATOR_PATH") {
                    // Finding 46: reject non-absolute / traversal paths
                    // before reading. Same-user IPC can set this env;
                    // path validation + the byte cap in load_image
                    // together bound the exposure.
                    let pb = std::path::PathBuf::from(&p);
                    let safe = pb.is_absolute() && !p.contains("..");
                    if !safe {
                        eprintln!("[annotator] rejecting unsafe MBS_ANNOTATOR_PATH: {:?}", p);
                    } else {
                        match std::fs::read(&pb) {
                            Ok(bytes) => {
                                a.load_image(pb, bytes);
                                eprintln!("[annotator] loaded capture from {p}");
                            }
                            Err(e) => eprintln!("[annotator] failed to read {p}: {e}"),
                        }
                    }
                }
                a
            },
            rng: RngOverlay::new(),
            email: if is_standalone && !matches!(standalone_target, Some(OverlayTarget::Email)) {
                EmailOverlay::empty()
            } else {
                EmailOverlay::new()
            },
            notifs: if is_notif_surface
                || matches!(standalone_target, Some(OverlayTarget::NotificationsHistory))
            {
                NotificationOverlay::new()
            } else {
                NotificationOverlay::empty()
            },
            lockscreen: if matches!(standalone_target, Some(OverlayTarget::Lockscreen)) {
                LockscreenOverlay::new()
            } else {
                LockscreenOverlay::empty()
            },

            music_cycle_state: Default::default(),
            is_cycling: false,

            last_cursor_x: 0.0,
            last_cursor_time: std::time::Instant::now() - std::time::Duration::from_secs(30),
            last_toast_count: 0,
            last_active_pending_written: None,
            last_lockscreen_check: std::time::Instant::now(),
        };

        // Bar-only init tasks. Overlays (--notes, --clipboard, --settings,
        // …) and the notif-surface don't render weather/tray/email — the
        // bar's TopBar::view is the only consumer. Firing them per-spawn
        // was costing 2 curl subprocesses (~20 ms each) plus a potential
        // feedback loop through the weather subscription.
        let is_bar = !is_standalone && !is_notif_surface;
        let mut init_tasks: Vec<Command<Message>> = Vec::new();
        if is_bar {
            init_tasks.push(Command::done(Message::FetchTrayItems));
            init_tasks.push(Command::done(Message::TickHardware));
            init_tasks.push(Command::perform(
                async {
                    moonbeauty::bar::weather::WeatherBackend::fetch()
                        .await
                        .ok()
                },
                Message::WeatherLoaded,
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Launcher)) {
            // Immediate focus attempt
            init_tasks
                .push(iced::widget::operation::focus("launcher_search").map(Message::Launcher));
            // Delayed re-focus — catches compositor focus arriving after map
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Launcher(LauncherMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Calc)) {
            init_tasks.push(
                iced::widget::operation::focus(moonbeauty::overlays::calc::CALC_INPUT_ID)
                    .map(Message::Calc),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Calc(CalcMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Pass)) {
            init_tasks.push(
                iced::widget::operation::focus(moonbeauty::overlays::pass::PASS_SEARCH_ID)
                    .map(Message::Pass),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Pass(PassMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Dictionary)) {
            init_tasks.push(
                iced::widget::operation::focus(
                    moonbeauty::overlays::dictionary::DICT_SEARCH_ID,
                )
                .map(Message::Dict),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Dict(DictMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Lockscreen)) {
            // No widget to focus — keyboard capture is via subscription
        } else if matches!(app.active_overlay, Some(OverlayTarget::Clipboard)) {
            init_tasks.push(Command::perform(ClipboardOverlay::reload(), |entries| {
                Message::Clipboard(ClipboardMessage::Reloaded(entries))
            }));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Notes)) {
            init_tasks.push(
                iced::widget::operation::focus(moonbeauty::overlays::notes::NOTES_INPUT_ID)
                    .map(Message::Notes),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Notes(NotesMessage::Refocus),
            ));
        }

        // Todo autofocus: same pattern as launcher/calc/pass — schedule
        // the input focus, then retry after 100ms in case the compositor
        // delivers window focus late (standalone layer surfaces sometimes
        // need this nudge).
        if matches!(app.active_overlay, Some(OverlayTarget::Todo)) {
            init_tasks.push(
                iced::widget::operation::focus(moonbeauty::overlays::todo::TODO_INPUT_ID)
                    .map(Message::Todo),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Todo(TodoMessage::Refocus),
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Unicode)) {
            init_tasks.push(
                iced::widget::operation::focus(
                    moonbeauty::overlays::unicode::UNICODE_INPUT_ID,
                )
                .map(Message::Unicode),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Unicode(UnicodeMessage::Refocus),
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Email)) {
            init_tasks.push(Command::done(Message::Email(EmailMessage::SetLoading)));
            init_tasks.push(Command::perform(
                async {
                    moonbeauty::libraries::backend::email::client::fetch_folder_list().await
                },
                |res| match res {
                    Ok(folders) => Message::Email(EmailMessage::FoldersLoaded(folders)),
                    Err(_) => Message::Email(EmailMessage::SyncDone),
                },
            ));
            init_tasks.push(Command::perform(
                async {
                    moonbeauty::libraries::backend::email::client::fetch_envelopes("INBOX")
                        .await
                },
                |res| match res {
                    Ok(envs) => {
                        Message::Email(EmailMessage::EnvelopesLoaded("INBOX".to_string(), envs))
                    }
                    Err(e) => Message::Email(EmailMessage::Error(e)),
                },
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Reminders)) {
            init_tasks.push(
                iced::widget::operation::focus(
                    moonbeauty::overlays::reminders::REMINDER_TASK_ID,
                )
                .map(Message::Reminders),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Reminders(ReminderMessage::InitialFocus),
            ));
        }

        // History: on open, scroll to the auto-selected newest (bottom) row.
        // Immediate pass is usually enough; the delayed retry catches the
        // case where the scrollable hasn't been laid out yet when the
        // first scroll_to fires.
        if matches!(
            app.active_overlay,
            Some(OverlayTarget::NotificationsHistory)
        ) {
            if let Some(y) = app
                .notifs
                .scroll_offset_for_selected(&app.theme, &app.settings)
            {
                init_tasks.push(iced::widget::operation::scroll_to(
                    iced::widget::Id::new("history_list"),
                    iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                ));
                init_tasks.push(Command::perform(
                    async move {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        y
                    },
                    |y| Message::Notif(NotifMessage::HistoryScrollTo(y)),
                ));
            }
        }

        (app, Command::batch(init_tasks))
    }
}
