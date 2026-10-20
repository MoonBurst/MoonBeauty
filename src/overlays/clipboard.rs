use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding};
use std::path::PathBuf;
use tokio::process::Command;

use crate::config::settings::ShellSettings;
use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone)]
pub struct ClipItem {
    pub id: u64,
    pub text: String,
    pub is_image: bool,
    pub thumb_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ClipboardMessage {
    SearchChanged(String),
    SelectEntry(u64),
    CopyEntry(u64),
    DeleteEntry(u64),
    WipeAll,
    RunOcr(u64),
    Reloaded(Vec<ClipItem>),
    PreviewLoaded { text: String, image_path: Option<String> },
    Dismiss,
}

pub struct ClipboardOverlay {
    pub search_query: String,
    pub entries: Vec<ClipItem>,
    pub selected_id: Option<u64>,
    pub preview_text: String,
    pub preview_image_path: Option<String>,
}

impl ClipboardOverlay {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            entries: Vec::new(),
            selected_id: None,
            preview_text: String::new(),
            preview_image_path: None,
        }
    }

    pub async fn reload() -> Vec<ClipItem> {
        let Ok(out) = Command::new("cliphist").arg("list").output().await else {
            return Vec::new();
        };

        let raw = String::from_utf8_lossy(&out.stdout);
        let mut list = Vec::new();
        let tmp_dir = std::env::temp_dir();

        for line in raw.lines() {
            if let Some((id_str, content)) = line.split_once('\t') {
                if let Ok(id) = id_str.trim().parse::<u64>() {
                    let is_image = content.contains("[[ binary data") || content.contains("binary data");
                    let thumb_path = if is_image {
                        let thumb = tmp_dir.join(format!("mbs-clip-thumb-{}.png", id));
                        let decode = Command::new("cliphist")
                            .args(["decode", &id.to_string()])
                            .output()
                            .await;
                        match decode {
                            Ok(d) if !d.stdout.is_empty() => {
                                if std::fs::write(&thumb, &d.stdout).is_ok() {
                                    Some(thumb.to_string_lossy().into_owned())
                                } else { None }
                            }
                            _ => None,
                        }
                    } else { None };
                    list.push(ClipItem {
                        id,
                        text: content.to_string(),
                        is_image,
                        thumb_path,
                    });
                }
            }
        }
        list
    }

    pub async fn load_preview(id: u64) -> (String, Option<String>) {
        let out = match Command::new("cliphist")
            .args(["decode", &id.to_string()])
            .output()
            .await
        {
            Ok(o) => o,
            Err(_) => return (String::new(), None),
        };
        if out.stdout.is_empty() { return (String::new(), None); }
        if out.stdout.len() > 8 * 1024 * 1024 {
            return ("[entry too large to preview]".into(), None);
        }
        match String::from_utf8(out.stdout.clone()) {
            Ok(text) => (text, None),
            Err(_) => {
                let tmp = std::env::temp_dir().join(format!("mbs-clip-preview-{}.png", id));
                if std::fs::write(&tmp, &out.stdout).is_ok() {
                    (String::new(), Some(tmp.to_string_lossy().into_owned()))
                } else {
                    (String::new(), None)
                }
            }
        }
    }

    pub async fn copy(id: u64) {
        let decode = match Command::new("cliphist")
            .args(["decode", &id.to_string()])
            .output()
            .await
        {
            Ok(d) => d,
            Err(e) => { eprintln!("[clipboard] cliphist decode failed: {e}"); return; }
        };
        if decode.stdout.is_empty() { return; }
        let mut child = match Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => { eprintln!("[clipboard] wl-copy spawn failed: {e}"); return; }
        };
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(&decode.stdout).await;
            drop(stdin);
        }
        let _ = child.wait().await;
    }

    pub async fn delete(id: u64) {
        let child = Command::new("cliphist")
            .arg("delete")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .ok();
        if let Some(mut c) = child {
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = c.stdin.take() {
                let _ = stdin.write_all(format!("{}\t\n", id).as_bytes()).await;
            }
        }
    }

    pub async fn wipe() {
        let _ = Command::new("cliphist").arg("wipe").status().await;
    }

    pub async fn ocr(id: u64) -> String {
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        let tmp_img = PathBuf::from(runtime).join(format!("qs_ocr_{}.png", id));

        if let Ok(decode) = Command::new("cliphist")
            .args(["decode", &id.to_string()])
            .output()
            .await
        {
            if std::fs::write(&tmp_img, &decode.stdout).is_ok() {
                if let Ok(tess) = Command::new("tesseract")
                    .args([tmp_img.to_str().unwrap(), "stdout"])
                    .output()
                    .await
                {
                    let _ = std::fs::remove_file(&tmp_img);
                    return String::from_utf8_lossy(&tess.stdout).trim().to_string();
                }
            }
        }
        String::new()
    }

    fn preview_view<'a>(&'a self, theme: &'a ShellTheme) -> Element<'a, ClipboardMessage> {
        let body: Element<'a, ClipboardMessage> = if let Some(ref path) = self.preview_image_path {
            container(
                iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(8)
            .into()
        } else if !self.preview_text.is_empty() {
            container(
                scrollable(
                    text(&self.preview_text)
                        .size(theme.font_size_normal)
                        .color(theme.text),
                )
                .height(Length::Fill),
            )
            .padding(12)
            .into()
        } else {
            container(
                text("Select an entry to preview")
                    .size(theme.font_size_normal)
                    .color(theme.muted),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        };

        let copy_btn: Element<'a, ClipboardMessage> = if let Some(id) = self.selected_id {
            button(
                text("📋 Copy Selected")
                    .size(theme.font_size_normal)
                    .color(theme.text),
            )
            .on_press(ClipboardMessage::CopyEntry(id))
            .padding([8, 16])
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(theme.surface_alt)),
                ..Default::default()
            })
            .into()
        } else {
            container(text("").size(1)).into()
        };

        container(
            column![body, copy_btn].spacing(8).padding(6),
        )
        .width(Length::FillPortion(55))
        .height(Length::Fill)
        .style(|_t| container::Style {
            background: Some(iced::Background::Color(theme.surface)),
            border: iced::Border { color: theme.border, width: 1.0, radius: 8.0.into() },
            ..Default::default()
        })
        .into()
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &ShellSettings) -> Element<'a, ClipboardMessage> {
        let (win_w, win_h) = settings.get_window_size("clipboard", 1080, 700);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let header_inset = if dy > 22.0 {
            (dx * (1.0 - (22.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let footer_inset = if dy > 20.0 {
            (dx * (1.0 - (20.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        let input = text_input("Search clipboard (e.g. image, text)...", &self.search_query)
            .on_input(ClipboardMessage::SearchChanged)
            .padding(14)
            .size(theme.font_size_overlay);

        let query_lower = self.search_query.trim().to_lowercase();
        let mut list_col = column![].spacing(6);
        for item in self.entries.iter()
            .filter(|item| query_lower.is_empty() || item.text.to_lowercase().contains(&query_lower))
            .take(50)
        {
            let id = item.id;
            let icon = if item.is_image { "🖼 " } else { "📄 " };
            let display_txt = if item.is_image { "[Image Entry]" } else { &item.text };

            let mut content_row = row![].spacing(10).align_y(Alignment::Center);
            if let Some(ref thumb) = item.thumb_path {
                content_row = content_row.push(
                    iced::widget::image(iced::widget::image::Handle::from_path(thumb.clone()))
                        .width(Length::Fixed(48.0))
                        .height(Length::Fixed(48.0))
                );
            }
            content_row = content_row.push(text(icon).size(16));
            content_row = content_row.push(
                text(display_txt)
                    .size(theme.font_size_normal)
                    .color(theme.text)
            );

            let row_item = button(content_row)
            .on_press(ClipboardMessage::SelectEntry(id))
            .padding([8, 12])
            .width(Length::Fill)
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

            list_col = list_col.push(row_item);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let list_pane = container(
            column![
                input,
                scrollable(list_col).height(Length::Fill),
            ]
            .spacing(8)
            .padding(4),
        )
        .width(Length::FillPortion(45))
        .height(Length::Fill);

        let preview_pane = self.preview_view(theme);

        let header_row = container(
            row![
                text("📋 CLIPBOARD MANAGER")
                    .size(theme.font_size_overlay)
                    .color(theme.text),
                row![].width(Length::Fill),
                button(text("🗑 Wipe All").size(12).color(theme.danger))
                    .on_press(ClipboardMessage::WipeAll)
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    }),
            ]
            .align_y(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let footer_text = container(
            text("Click to select  •  Click 📋 to copy  •  ESC to dismiss")
                .size(11)
                .color(theme.muted)
                .align_x(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: footer_inset,
            bottom: 0.0,
            left: footer_inset,
        });

        let card_content = container(
            column![
                header_row,
                row![list_pane, preview_pane].spacing(12).height(Length::Fill),
                footer_text,
            ]
            .spacing(12)
            .padding(20),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(ClipboardMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.65))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
