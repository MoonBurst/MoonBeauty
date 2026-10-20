use chrono::Local;
use iced::widget::{button, column, container, row, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use std::path::PathBuf;
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone)]
pub enum QuickshotMessage {
    WatermarkInputChanged(String),
    CaptureScreen,
    TriggerOcr,
    TriggerReveal,
    Dismiss,
}

pub struct QuickshotOverlay {
    pub watermark_recipient: String,
    pub is_scan_active: bool,
}

impl QuickshotOverlay {
    pub fn new() -> Self {
        Self {
            watermark_recipient: String::new(),
            is_scan_active: false,
        }
    }

    pub fn update(&mut self, message: QuickshotMessage) {
        match message {
            QuickshotMessage::WatermarkInputChanged(txt) => {
                self.watermark_recipient = txt;
            }
            QuickshotMessage::TriggerReveal => {
                self.is_scan_active = !self.is_scan_active;
            }
            _ => {}
        }
    }

    fn scratch_dir() -> PathBuf {
        let base = dirs::runtime_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let p = base.join("moon-beauty-quickshot");
        let _ = std::fs::create_dir_all(&p);
        p
    }

    pub async fn capture_and_save(recipient: &str) {
        eprintln!("[quickshot] capture_and_save recipient={:?}", recipient);
        let stamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let save_dir = dirs::picture_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")))
            .join("Screenshots");
        let _ = std::fs::create_dir_all(&save_dir);

        let out_file = save_dir.join(format!("quickshot_{}.png", stamp));
        let scratch_file = Self::scratch_dir().join(format!("raw_{}.png", stamp));

        let grim_status = Command::new("grim")
            .arg(scratch_file.to_str().unwrap())
            .status()
            .await;
        eprintln!("[quickshot] grim done: {:?}", grim_status.as_ref().map(|s| s.success()));

        if grim_status.map(|s| s.success()).unwrap_or(false) {
            if recipient.trim().is_empty() {
                eprintln!("[quickshot] no recipient -> plain copy");
                let _ = std::fs::copy(&scratch_file, &out_file);
                let _ = Command::new("wl-copy")
                    .args(["-t", "image/png"])
                    .stdin(std::fs::File::open(&out_file).unwrap())
                    .status()
                    .await;
                let _ = Command::new("notify-send")
                    .args(["-a", "Quickshot", "Screenshot Saved (no watermark)", out_file.to_str().unwrap()])
                    .spawn();
            } else {
                // Detect ImageMagick by exit status, not by spawn success.
                let magick_bin = if Command::new("magick").arg("--version").output().await
                    .map(|o| o.status.success()).unwrap_or(false) { Some("magick") }
                    else if Command::new("convert").arg("--version").output().await
                    .map(|o| o.status.success()).unwrap_or(false) { Some("convert") }
                    else { None };
                eprintln!("[quickshot] magick_bin={:?}", magick_bin);

                if let Some(bin) = magick_bin {
                    // One file per comma-separated recipient. Five 64-pt
                    // 55%-alpha red copies of the name at NW, NE, Center,
                    // SW, SE — impossible to miss on any background.
                    let recipients: Vec<String> = recipient
                        .split(',')
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .map(String::from)
                        .collect();

                    if recipients.is_empty() {
                        eprintln!("[quickshot] recipient-only-ws -> plain copy");
                        let _ = std::fs::copy(&scratch_file, &out_file);
                        let _ = Command::new("notify-send")
                            .args(["-a", "Quickshot", "Screenshot Saved (no watermark)", out_file.to_str().unwrap()])
                            .spawn();
                    } else {
                        for name in &recipients {
                            let safe: String = name
                                .chars()
                                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                                .collect();
                            let out = save_dir.join(format!("quickshot_{}_{}.png", stamp, safe));
                            // ImageMagick's default font on NixOS may not be
                            // registered; without -font, annotate silently
                            // produces no text. Liberation-Sans-Bold is
                            // confirmed present on this system.
                            let output = Command::new(bin)
                                .args([
                                    scratch_file.to_str().unwrap(),
                                    "-font", "Liberation-Sans-Bold",
                                    "-fill", "rgba(255,40,40,0.55)",
                                    "-pointsize", "64",
                                    "-gravity", "NorthWest", "-annotate", "+80+80", name.as_str(),
                                    "-gravity", "NorthEast", "-annotate", "+80+80", name.as_str(),
                                    "-gravity", "Center", "-annotate", "+0+0", name.as_str(),
                                    "-gravity", "SouthWest", "-annotate", "+80+80", name.as_str(),
                                    "-gravity", "SouthEast", "-annotate", "+80+80", name.as_str(),
                                    out.to_str().unwrap(),
                                ])
                                .output()
                                .await;
                            match output {
                                Ok(o) if o.status.success() => {
                                    eprintln!("[quickshot] watermark for {:?} -> {:?}", name, out);
                                    if !o.stderr.is_empty() {
                                        eprintln!("[quickshot] magick stderr: {}",
                                            String::from_utf8_lossy(&o.stderr).trim());
                                    }
                                    let _ = Command::new("notify-send")
                                        .args(["-a", "Quickshot", "Watermarked Screenshot", name])
                                        .spawn();
                                }
                                Ok(o) => {
                                    eprintln!("[quickshot] watermark {:?} FAILED (exit {:?}): {}",
                                        name, o.status.code(),
                                        String::from_utf8_lossy(&o.stderr).trim());
                                    let _ = Command::new("notify-send")
                                        .args(["-a", "Quickshot", "-u", "low", "Watermark failed for", name])
                                        .spawn();
                                }
                                Err(e) => eprintln!("[quickshot] magick spawn error: {e}"),
                            }
                        }
                    }
                } else {
                    eprintln!("[quickshot] ImageMagick MISSING -> plain copy");
                    let _ = std::fs::copy(&scratch_file, &out_file);
                    let _ = Command::new("notify-send")
                        .args(["-a", "Quickshot", "Saved without watermark (no ImageMagick)", out_file.to_str().unwrap()])
                        .spawn();
                }
            }
            let _ = std::fs::remove_file(&scratch_file);
        } else {
            eprintln!("[quickshot] grim failed; nothing captured");
            let _ = Command::new("notify-send")
                .args(["-a", "Quickshot", "-u", "critical", "Screenshot Failed", "grim returned nonzero"])
                .spawn();
        }
    }

    pub async fn run_ocr_screen() {
        let scratch_file = Self::scratch_dir().join("ocr_tmp.png");
        let _ = Command::new("grim").arg(scratch_file.to_str().unwrap()).status().await;

        if let Ok(out) = Command::new("tesseract")
            .args([scratch_file.to_str().unwrap(), "stdout"])
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let _ = std::fs::remove_file(&scratch_file);
            if !text.is_empty() {
                let _ = Command::new("wl-copy").arg(&text).status().await;
                let _ = Command::new("notify-send")
                    .args(["-a", "Quickshot", "OCR Text Copied", &format!("{} characters", text.len())])
                    .spawn();
            }
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, QuickshotMessage> {
        let input = text_input("Recipients (e.g. alice, bob) for invisible watermark...", &self.watermark_recipient)
            .on_input(QuickshotMessage::WatermarkInputChanged)
            .padding(14)
            .size(theme.font_size_overlay);

        let action_row = row![
            button(text("📸 Full Capture").size(theme.font_size_normal).bold().color(theme.success))
                .on_press(QuickshotMessage::CaptureScreen)
                .padding([10, 16])
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("🔤 Extract Text (OCR)").size(theme.font_size_normal).bold().color(theme.text))
                .on_press(QuickshotMessage::TriggerOcr)
                .padding([10, 16])
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text(if self.is_scan_active { "👁 Hide Reveal" } else { "🔍 Reveal Proof" }).size(theme.font_size_normal).bold().color(theme.warning))
                .on_press(QuickshotMessage::TriggerReveal)
                .padding([10, 16])
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("📷 QUICKSHOT SCREEN CAPTURE & STEGANOGRAPHY")
                    .size(theme.font_size_overlay)
                    .color(theme.text),
                input,
                action_row,
                text("ESC to dismiss  •  Enter to capture & copy")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(16)
            .padding(24),
        )
        .width(Length::Fixed(780.0))
        .height(Length::Fixed(320.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(QuickshotMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.6))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
