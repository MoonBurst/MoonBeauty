//! Clipboard-related update handlers, extracted from `app/update.rs`.
//!
//! Covers `Message::Clipboard(msg)` — the entire clipboard overlay dispatch,
//! including image preview (JXL via `handle_for_path`), OCR, and reveal.

#![allow(clippy::collapsible_match)]

use crate::app::{Message, MoonBeautyShellApp};
use iced::Task as Command;
use moonbeauty::overlays::clipboard::{ClipboardMessage, ClipboardOverlay};

impl MoonBeautyShellApp {
    pub(crate) fn handle_clipboard(&mut self, msg: ClipboardMessage) -> Command<Message> {
        match msg {
            ClipboardMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            ClipboardMessage::CopyEntry(id) => {
                if self.is_standalone {
                    crate::app::helpers::run_then_exit(async move {
                        ClipboardOverlay::copy(id).await;
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    });
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
                ClipboardOverlay::delete(id);
                return Command::none();
            }
            ClipboardMessage::WipeAll => {
                return Command::perform(
                    async move {
                        ClipboardOverlay::wipe().await;
                    },
                    |_| Message::DismissActiveOverlay,
                );
            }
            ClipboardMessage::SetFilter(f) => {
                self.clipboard.filter = f;
                self.clipboard.selected_id = None;
                self.clipboard.preview_text.clear();
                self.clipboard.preview_image_data = None;
                return Command::none();
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
                        move |(text, image_data)| {
                            Message::Clipboard(ClipboardMessage::PreviewLoaded {
                                id: next_id,
                                text,
                                image_data,
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
                        move |(text, image_data)| {
                            Message::Clipboard(ClipboardMessage::PreviewLoaded {
                                id: prev_id,
                                text,
                                image_data,
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
                    ClipboardOverlay::delete(id);
                    let pos = self.clipboard.entries.iter().position(|e| e.id == id);

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
                                    move |(text, image_data)| {
                                        Message::Clipboard(ClipboardMessage::PreviewLoaded {
                                            id: next_id,
                                            text,
                                            image_data,
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
                            self.clipboard.preview_image_data = None;
                        }
                    }
                }
                return Command::none();
            }
            ClipboardMessage::RevealImage(id) => {
                let dir = dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("moonbeauty")
                    .join("clipboard");
                if let Some(path) = ClipboardOverlay::get_file_path(&dir, id) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if let Ok(revealed) =
                            moonbeauty::overlays::screenshot::watermark::reveal(&bytes)
                        {
                            let out_path = dir.join(format!("revealed_{}.jxl", id));
                            let tmp = dir.join(format!(".revealed-{}.tmp.png", id));
                            let ok = std::fs::write(&tmp, &revealed).is_ok()
                                && moonbeauty::overlays::screenshot::annotator::png_to_jxl(
                                    &tmp, &out_path,
                                )
                                .is_ok();
                            let _ = std::fs::remove_file(&tmp);
                            if ok {
                                let _ = std::process::Command::new("xdg-open")
                                    .arg(&out_path)
                                    .spawn();
                            }
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
                let tmp_path = moonbeauty::libraries::process::mbs_subdir("mbs-clip")
                    .join(format!("mbs-ocr-{}.png", id));

                // Do OCR synchronously — it's fast (~1s) and we're about to close anyway
                // Read directly from our own clipboard store instead of cliphist.
                let ocr_ready = if let Some(src) = ClipboardOverlay::resolve_path(id) {
                    let ext = src
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if ext == "jxl" {
                        std::process::Command::new("djxl")
                            .arg(&src)
                            .arg(&tmp_path)
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false)
                    } else {
                        std::fs::copy(&src, &tmp_path).is_ok()
                    }
                } else {
                    eprintln!("[ocr] could not resolve path for id={}", id);
                    false
                };
                if ocr_ready {
                    let tess_out = std::process::Command::new("tesseract")
                        .args([tmp_path.to_str().unwrap(), "stdout"])
                        .env_remove("LD_LIBRARY_PATH")
                        .output();

                    if let Ok(tess_out) = tess_out {
                        let text = String::from_utf8_lossy(&tess_out.stdout).trim().to_string();
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
                let _ = std::fs::remove_file(&tmp_path);

                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
                return Command::none();
            }
            ClipboardMessage::CopySelected => {
                // Delegate to ClipboardOverlay::copy — it knows about
                // JXL (.preview.png) and picks the right MIME. The
                // previous cliphist-based path silently no-op'd on
                // JXL entries and served the wrong MIME for other
                // images.
                if let Some(id) = self.clipboard.selected_id {
                    if self.is_standalone {
                        let rt = tokio::runtime::Runtime::new()
                            .expect("tokio runtime for clipboard copy");
                        rt.block_on(async move {
                            ClipboardOverlay::copy(id).await;
                        });
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
            }
            ClipboardMessage::SelectEntry(id) => {
                // Only reset the preview if we're actually switching to a different item.
                // This prevents the UI from flashing "Loading..." when clicking the same image.
                if self.clipboard.selected_id != Some(id) {
                    self.clipboard.preview_text = "Loading...".to_string();
                    self.clipboard.preview_image_data = None;
                }
                self.clipboard.selected_id = Some(id);
                return Command::perform(
                    ClipboardOverlay::load_preview(id),
                    move |(text, image_data)| {
                        Message::Clipboard(ClipboardMessage::PreviewLoaded {
                            id,
                            text,
                            image_data,
                        })
                    },
                );
            }
            ClipboardMessage::PreviewLoaded {
                id,
                text,
                image_data,
            } => {
                // Ignore stale async results from older clicks
                if Some(id) == self.clipboard.selected_id {
                    self.clipboard.preview_text = text;
                    self.clipboard.preview_image_data = image_data;
                }
            }
        }
        Command::none()
    }
}
