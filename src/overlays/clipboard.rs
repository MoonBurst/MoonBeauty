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
    SelectNext,
    SelectPrev,
    CopySelected,
    CopyEntry(u64),
    DeleteEntry(u64),
    DeleteSelected,
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

impl Default for ClipboardOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardOverlay {
    pub fn filtered_ids(&self) -> Vec<u64> {
        self.entries.iter()
            .filter(|e| self.search_query.is_empty() || e.text.to_lowercase().contains(&self.search_query.to_lowercase()))
            .map(|e| e.id)
            .collect()
    }

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
        let cache_dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
        let map_path = cache_dir.join("mbs-clip-names.txt");
        let mut name_map = std::collections::HashMap::new();
        
        if let Ok(map_content) = std::fs::read_to_string(&map_path) {
            for line in map_content.lines() {
                if let Some((id_str, name)) = line.split_once('\t') {
                    if let Ok(id) = id_str.parse::<u64>() {
                        name_map.insert(id, name.to_string());
                    }
                }
            }
        }

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

                    // Use mapped filename if it exists
                    let text_val = if is_image {
                        if let Some(name) = name_map.get(&id) {
                            format!("📸 {}", name)
                        } else {
                            let clean = content.replace("[[ binary data ", "").replace(" ]]", "");
                            format!("📸 Screenshot {}", clean.trim())
                        }
                    } else {
                        content.to_string()
                    };

                    list.push(ClipItem {
                        id,
                        text: text_val,
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

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &ShellSettings) -> Element<'a, ClipboardMessage> {
        let is_image_selected = self.selected_id.and_then(|id| self.entries.iter().find(|e| e.id == id)).map(|e| e.is_image).unwrap_or(false);

        let (win_w, win_h) = settings.get_window_size("clipboard", 1000, 700);
        let cut = theme.overlay_cut as u16;

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let pad_top = 18.0_f32;
        let pad_bottom = 16.0_f32;

        let search_inward = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        let list_top_inset = if dy > 72.0 {
            (dx * (1.0 - (72.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_bottom_inset = if dy > 45.0 {
            (dx * (1.0 - (45.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_side_inset = list_top_inset.max(list_bottom_inset).max(20.0);

        // Header
        let search_input = text_input("Search clipboard...", &self.search_query)
            .on_input(ClipboardMessage::SearchChanged)
            .padding(Padding { top: 12.0, right: 24.0, bottom: 12.0, left: 24.0 })
            .size((theme.font_size_overlay) as f32)
            .width(Length::Fill)
            .style(move |_t, _s| text_input::Style {
                background: iced::Background::Color(Color::TRANSPARENT),
                border: iced::Border::default(),
                icon: theme.muted,
                placeholder: theme.muted,
                value: theme.text,
                selection: theme.surface_alt,
            });

        let search_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.border, theme.control_border_width)
            .size(Length::Fill, Length::Fixed(48.0));

        let search_bar = container(
            Stack::new()
                .push(search_bg.view())
                .push(search_input)
        ).height(Length::Fixed(48.0));

        let wipe_btn = button(
            container(text("Wipe All").size((theme.font_size_overlay) as f32).color(theme.danger))
                .center_y(Length::Fill)
                .center_x(Length::Fill)
        )
        .on_press(ClipboardMessage::WipeAll)
        .height(Length::Fixed(48.0))
        .width(Length::Fixed(100.0))
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            border: iced::Border::default(),
            ..Default::default()
        });

        let wipe_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.danger, theme.control_border_width)
            .size(Length::Fixed(100.0), Length::Fixed(48.0));

        let ocr_btn = button(
            container(text("OCR").size((theme.font_size_overlay) as f32).color(if is_image_selected { theme.text } else { theme.muted }))
                .center_y(Length::Fill)
                .center_x(Length::Fill)
        )
        .on_press_maybe(Some(ClipboardMessage::RunOcr(
            self.selected_id.unwrap_or(0)
        )))
        .height(Length::Fixed(48.0))
        .width(Length::Fixed(80.0))
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            border: iced::Border::default(),
            ..Default::default()
        });

        let ocr_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(if is_image_selected { theme.text } else { theme.border }, theme.control_border_width)
            .size(Length::Fixed(80.0), Length::Fixed(48.0));

        let ocr_bar = container(
            Stack::new()
                .push(ocr_bg.view())
                .push(ocr_btn)
        ).height(Length::Fixed(48.0)).width(Length::Fixed(80.0));

        let wipe_bar = container(
            Stack::new()
                .push(wipe_bg.view())
                .push(wipe_btn)
        ).height(Length::Fixed(48.0)).width(Length::Fixed(100.0));

        let input_row = container(
            row![search_bar, ocr_bar, wipe_bar].spacing(12).align_y(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding { top: 0.0, right: search_inward, bottom: 0.0, left: search_inward });

        // Left Pane (List)
        let mut list_col = column![].spacing(8);
        for item in &self.entries {
            if !self.search_query.is_empty() && !item.text.to_lowercase().contains(&self.search_query.to_lowercase()) {
                continue;
            }
            let is_sel = self.selected_id == Some(item.id);
            let border_color = if is_sel { theme.danger } else { theme.border }; // Base08 outline
            let text_color = if is_sel { theme.text } else { theme.text }; // Keep text normal

            let raw_text = item.text.lines().next().unwrap_or("").to_string();
            
            let display_text = if raw_text.chars().count() > 30 {
                let truncated: String = raw_text.chars().take(30).collect();
                format!("{}...", truncated)
            } else {
                raw_text
            };

            let mut item_row = row![].spacing(12).align_y(Alignment::Center);
            if let Some(thumb) = &item.thumb_path {
                item_row = item_row.push(
                    iced::widget::image(iced::widget::image::Handle::from_path(thumb.clone()))
                        .width(Length::Fixed(40.0))
                        .height(Length::Fixed(40.0))
                        .content_fit(iced::ContentFit::Contain)
                );
            } else {
                item_row = item_row.push(text("■").size(10).color(theme.muted));
            }
            
            // Force the text to stay strictly within the box
            let text_container = container(text(display_text).size((theme.font_size_overlay) as f32).color(text_color))
                .width(Length::Fill)
                .clip(true);
            item_row = item_row.push(text_container);

            let btn_content = container(item_row)
                .padding(Padding { top: 0.0, right: 24.0, bottom: 0.0, left: 24.0 })
                .width(Length::Fill)
                .height(Length::Fixed(56.0))
                .align_y(Alignment::Center);

            let item_bg = ShapeBox::new()
                .shape(ShapeType::Hexagon { cut })
                .background(theme.background) // Always dark background
                .border(border_color, theme.control_border_width)
                .size(Length::Fill, Length::Fixed(56.0));

            let item_btn = button(
                Stack::new()
                    .push(item_bg.view())
                    .push(btn_content)
            )
            .on_press(ClipboardMessage::SelectEntry(item.id))
            .width(Length::Fill)
            .height(Length::Fixed(56.0))
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                border: iced::Border::default(),
                ..Default::default()
            });

            list_col = list_col.push(item_btn);
        }

        let left_pane = container(
            scrollable(list_col).id(iced::widget::Id::new("clip_list")).width(Length::Fill).height(Length::Fill)
        )
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .clip(true)
        .padding(Padding {
            top: list_top_inset,
            right: 24.0, // Increased to keep scrollbar away from right pane
            bottom: list_bottom_inset,
            left: list_side_inset,
        });

        // Right Pane (Preview)
        // Dynamic padding based on the chamfer cut to prevent spilling out of the octagon
        let inner_pad = cut_f + 4.0; // Tight padding: just enough to clear the chamfer
        let inner_padding = Padding { top: inner_pad, right: inner_pad, bottom: inner_pad, left: inner_pad };

        let preview_content: Element<'a, ClipboardMessage> = if let Some(img_path) = &self.preview_image_path {
            container(
                container(
                    iced::widget::image(iced::widget::image::Handle::from_path(img_path.clone()))
                        .width(Length::Fill)
                        .content_fit(iced::ContentFit::Contain)
                )
                .padding(inner_padding)
                .center_x(Length::Fill).center_y(Length::Fill).width(Length::Fill).height(Length::Fill)
            ).into()
        } else if !self.preview_text.is_empty() {
            container(
                scrollable(
                    container(
                        text(&self.preview_text)
                            .size((theme.font_size_overlay) as f32)
                            .font(iced::Font::MONOSPACE)
                            .color(theme.text)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    ).width(Length::Fill)
                ).width(Length::Fill).height(Length::Fill)
            ).padding(inner_padding).width(Length::Fill).height(Length::Fill).into()
        } else {
            container(text("Select an entry to preview").size((theme.font_size_overlay) as f32).color(theme.muted))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let right_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.border, theme.control_border_width)
            .size(Length::Fill, Length::Fill);

        // Wrap the Stack in a container with .clip(true) so the text inside the right pane doesn't bleed out
        let right_pane = container(
            Stack::new()
                .push(right_bg.view())
                .push(preview_content)
        )
        .width(Length::FillPortion(2))
        .clip(true)
        .height(Length::Fill)
        .clip(true);

        let content = column![
            input_row,
            row![left_pane, right_pane].spacing(16).height(Length::Fill).width(Length::Fill)
        ]
        .spacing(16)
        .padding(Padding { top: pad_top, right: 0.0, bottom: pad_bottom, left: 0.0 })
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let bg_shape = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.border, theme.control_border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        container(
            Stack::new()
                .push(bg_shape.view())
                .push(content)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}
