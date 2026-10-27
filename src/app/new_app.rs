use super::*;

impl MoonBeautyShellApp {
    pub fn new_app() -> (Self, Command<Message>) {
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
                    serde_json::from_str::<Vec<moon_beauty_shell::bar::tray::TrayMenuItem>>(&json)
                {
                    let _ = std::fs::remove_file(&json_path);
                    std::process::exit(
                        match moon_beauty_shell::bar::tray::context::run(
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

        let (ipc_tx, ipc_rx) = std::sync::mpsc::channel::<IpcCommand>();
        if standalone_target.is_none() && !is_notif_surface {
            std::thread::spawn(move || {
                use std::io::Read;
                use std::os::unix::net::UnixListener;
                let path = moon_beauty_shell::libraries::ipc::get_socket_path();
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
                            let _ =
                                stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                            let mut buf = Vec::new();
                            if let Err(e) = stream.read_to_end(&mut buf) {
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

        let notif_rx = if is_notif_surface {
            let (tx, rx) = std::sync::mpsc::channel();
            moon_beauty_shell::overlays::notifications::spawn(tx);
            moon_beauty_shell::bar::tray::watcher::spawn();
            Some(rx)
        } else {
            None
        };

        let notif_screen_at_startup = settings.notif_screen_name.clone();
        let app = Self {
            settings_ui: SettingsOverlay::new(&settings),
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
            active_overlay: standalone_target,
            weather_tooltip_child: None,
            is_standalone,
            system_collector: SystemCollector::new(),
            standalone_child: None,
            ipc_rx,
            is_notif_surface,
            notif_rx,
            settings_json_mtime: std::fs::metadata(ShellSettings::config_path())
                .ok()
                .and_then(|m| m.modified().ok()),
            window_w: 1920.0,
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

            launcher: AppLauncher::new(),
            calc: {
                if let Ok(expr) = std::env::var("MBS_CALC_EXPR") {
                    CalculatorOverlay::new_with_input(&expr)
                } else {
                    CalculatorOverlay::new()
                }
            },
            clipboard: ClipboardOverlay::new(),
            notes: NotesOverlay::new(),
            pass: PassOverlay::new(),
            power: PowerOverlay::new(),
            todo: TodoOverlay::new(),
            reminders: RemindersOverlay::new(),
            dict: DictionaryOverlay::new(),
            unicode: UnicodeOverlay::new(),
            web: WebSearchOverlay::new(),
            amogus: AmogusOverlay::new(),
            annotator: {
                let mut a = AnnotatorOverlay::new();
                if let Ok(p) = std::env::var("MBS_ANNOTATOR_PATH") {
                    let pb = std::path::PathBuf::from(&p);
                    match std::fs::read(&pb) {
                        Ok(bytes) => {
                            a.load_image(pb, bytes);
                            eprintln!("[annotator] loaded capture from {p}");
                        }
                        Err(e) => eprintln!("[annotator] failed to read {p}: {e}"),
                    }
                }
                a
            },
            rng: RngOverlay::new(),
            email: EmailOverlay::new(),
            notifs: NotificationOverlay::new(),
            lockscreen: LockscreenOverlay::new(),
            weather_tick: false,

            last_cursor_x: 0.0,
            last_cursor_time: std::time::Instant::now() - std::time::Duration::from_secs(30),
            last_toast_count: 0,
            last_active_pending_written: None,
            last_lockscreen_check: std::time::Instant::now(),
        };

        let mut init_tasks = vec![
            Command::done(Message::FetchTrayItems),
            Command::done(Message::TickEmailSync),
            Command::perform(
                async {
                    moon_beauty_shell::bar::weather::WeatherBackend::fetch()
                        .await
                        .ok()
                },
                Message::WeatherLoaded,
            ),
        ];

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
                iced::widget::operation::focus(moon_beauty_shell::overlays::calc::CALC_INPUT_ID)
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
                iced::widget::operation::focus(moon_beauty_shell::overlays::pass::PASS_SEARCH_ID)
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
                    moon_beauty_shell::overlays::dictionary::DICT_SEARCH_ID,
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
                iced::widget::operation::focus(moon_beauty_shell::overlays::notes::NOTES_INPUT_ID)
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
                iced::widget::operation::focus(moon_beauty_shell::overlays::todo::TODO_INPUT_ID)
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
                    moon_beauty_shell::overlays::unicode::UNICODE_INPUT_ID,
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
                    moon_beauty_shell::libraries::backend::email_client::fetch_envelopes("INBOX")
                        .await
                },
                |res| match res {
                    Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(envs)),
                    Err(e) => Message::Email(EmailMessage::Error(e)),
                },
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Reminders)) {
            init_tasks.push(
                iced::widget::operation::focus(
                    moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID,
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
