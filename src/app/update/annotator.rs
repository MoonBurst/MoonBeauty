use super::*;
use moonbeauty::overlays::screenshot;
use moonbeauty::overlays::screenshot::annotator::AnnotatorMessage;
use moonbeauty::overlays::screenshot::annotator::Tool;
use moonbeauty::overlays::screenshot::save_annotated_image;

impl MoonBeautyShellApp {
    pub(crate) fn handle_annotator(&mut self, msg: AnnotatorMessage) -> Command<Message> {
        let is_text_press =
            matches!(msg, AnnotatorMessage::Pressed(_)) && self.annotator.tool == Tool::Text;
        match msg {
            AnnotatorMessage::ToggleReveal => {
                self.annotator.reveal_mode = !self.annotator.reveal_mode;
                if !self.annotator.reveal_mode {
                    self.annotator.status = "Draw · Enter saves & closes · ESC cancels".into();
                } else if self.annotator.reveal_bytes.is_some() {
                    self.annotator.status = "Reveal: watermark amplified".into();
                } else if let Some(source) = self.annotator.source_path.clone() {
                    match std::fs::read(&source)
                        .map_err(|e| e.to_string())
                        .and_then(|b| screenshot::watermark::reveal(&b))
                    {
                        Ok(bytes) => {
                            self.annotator.reveal_handle =
                                Some(iced::widget::image::Handle::from_bytes(bytes.clone()));
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
                        let time_str = chrono::Local::now().format("%Y-%m-%d %H-%M").to_string();
                        let out = save_dir.join(format!("Revealed {}.jxl", time_str));

                        // reveal() emits PNG bytes; encode to JXL for disk.
                        let tmp_reveal =
                            save_dir.join(format!(".mbs-reveal-{}.png", std::process::id()));
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

                                    let cache_dir =
                                        dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
                                    let map_path = cache_dir.join("mbs-clip-names.txt");
                                    let filename = out
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();

                                    let mut existing_clip_ids = std::collections::HashSet::new();
                                    if let Ok(list_out) = std::process::Command::new("cliphist")
                                        .args(["list"])
                                        .output()
                                    {
                                        for line in
                                            String::from_utf8_lossy(&list_out.stdout).lines()
                                        {
                                            if let Some((id_str, _)) = line.split_once('\t') {
                                                if let Ok(id) = id_str.parse::<u64>() {
                                                    existing_clip_ids.insert(id);
                                                }
                                            }
                                        }
                                    }

                                    for _ in 0..20 {
                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                        if let Ok(list_out) = std::process::Command::new("cliphist")
                                            .args(["list"])
                                            .output()
                                        {
                                            let text = String::from_utf8_lossy(&list_out.stdout);
                                            for line in text.lines() {
                                                if let Some((id_str, _)) = line.split_once('\t') {
                                                    if let Ok(id) = id_str.trim().parse::<u64>() {
                                                        if !existing_clip_ids.contains(&id) {
                                                            let entry =
                                                                format!("{}\t{}\n", id, filename);
                                                            if let Ok(mut f) =
                                                                std::fs::OpenOptions::new()
                                                                    .create(true)
                                                                    .append(true)
                                                                    .open(&map_path)
                                                            {
                                                                let _ =
                                                                    f.write_all(entry.as_bytes());
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
                                self.annotator.status = "Reveal save failed: JXL encode".to_string()
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
                        None => self.annotator.status = "Save failed: no source image".into(),
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

        Command::none()
    }
}
