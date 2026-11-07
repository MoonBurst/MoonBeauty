#![allow(clippy::collapsible_match)]
use super::*;

mod clipboard;
mod dismiss;
mod email;
mod email_sync;
mod hardware;
mod ipc;
mod launcher;
mod lockscreen;
mod music;
mod notifications;
mod overlays;
mod reminders;
mod settings;
mod top_bar;
mod tray;

impl MoonBeautyShellApp {
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            // Message::WindowResized removed (problem 07)
            Message::ResizeNotifSurface(w, h) => {
                // Intercepted by `try_into`; never reached. Arm kept only
                // to keep the match exhaustive.
                let _ = (w, h);
            }
            Message::IdleTick => return self.handle_idle_tick(),

            Message::PollNotifs => return self.handle_poll_notifs(),
            Message::TickClock => return self.handle_tick_clock(),

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
            Message::TickEmailSync => return self.handle_tick_email_sync(),

            Message::EmailSyncedWithDiff(old, new) => {
                return self.handle_email_synced_with_diff(old, new)
            }

            Message::EmailSynced => return self.handle_email_synced(),

            Message::TickWeather => return self.handle_tick_weather(),

            Message::WeatherLoaded(w) => return self.handle_weather_loaded(w),

            Message::TickHardware => return self.handle_tick_hardware(),

            Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris, net) => {
                return self.handle_metrics_collected(cpu, ram, gpu, audio, mic, mpris, net)
            }

            Message::IpcReceived(cmd) => return self.handle_ipc(cmd),
            Message::TopBar(msg) => return self.handle_top_bar(msg),
            Message::Launcher(msg) => return self.handle_launcher(msg),
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
                                let tmp_reveal = save_dir
                                    .join(format!(".mbs-reveal-{}.png", std::process::id()));
                                let write_ok = std::fs::write(&tmp_reveal, &bytes).is_ok();
                                let convert_ok = write_ok
                                    && moonbeauty::overlays::screenshot::annotator::png_to_jxl(
                                        &tmp_reveal,
                                        &out,
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
                                Some(p) => match save_annotated_image(
                                    p,
                                    annotations,
                                    watermark,
                                    self.settings.screenshot_save_format,
                                ) {
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
            Message::TrayWatchdogTick => return self.handle_tray_watchdog(),
            Message::TrayAnimTick => return self.handle_tray_anim(),
            Message::FetchTrayItems => return self.handle_fetch_tray_items(),
            Message::TrayItemsLoaded(items) => return self.handle_tray_items_loaded(items),

            Message::DismissActiveOverlay => return self.handle_dismiss_active_overlay(),

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
