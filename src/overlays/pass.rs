use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding};
use std::path::PathBuf;
use tokio::process::Command;
use walkdir::WalkDir;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};

pub const PASS_SEARCH_ID: &str = "pass_search";

#[derive(Debug, Clone)]
pub enum PassMessage {
    SearchChanged(String),
    CopyKey(String),
    Dismiss,
    Refocus,
}

pub struct PassOverlay {
    pub search_query: String,
    pub all_keys: Vec<String>,
    pub filtered_keys: Vec<String>,
}

impl Default for PassOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl PassOverlay {
    pub fn new() -> Self {
        let all_keys = Self::scan_passwords();
        Self {
            search_query: String::new(),
            filtered_keys: all_keys.clone(),
            all_keys,
        }
    }

    pub fn scan_passwords() -> Vec<String> {
        let store_dir = std::env::var("PASSWORD_STORE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join(".password-store")
            });

        let mut keys = Vec::new();
        if !store_dir.exists() {
            return keys;
        }

        for entry in WalkDir::new(&store_dir).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("gpg") {
                if let Ok(rel) = path.strip_prefix(&store_dir) {
                    let mut s = rel.to_string_lossy().to_string();
                    if s.ends_with(".gpg") {
                        s.truncate(s.len() - 4);
                        if !s.starts_with(".git") {
                            keys.push(s);
                        }
                    }
                }
            }
        }

        keys.sort();
        keys
    }

    pub fn update(&mut self, message: PassMessage) {
        if let PassMessage::SearchChanged(q) = message {
            self.search_query = q.clone();
            let lower = q.trim().to_lowercase();
            if lower.is_empty() {
                self.filtered_keys = self.all_keys.clone();
            } else {
                self.filtered_keys = self
                    .all_keys
                    .iter()
                    .filter(|k| k.to_lowercase().contains(&lower))
                    .cloned()
                    .collect();
            }
        }
    }

    pub async fn copy_password(key: &str) {
        let clean = key.trim();
        if clean.is_empty() {
            return;
        }
        let _ = Command::new("pass").args(["-c", clean]).status().await;
        let _ = Command::new("notify-send")
            .args(["-a", "Password Store", "Credential Copied", clean])
            .spawn();
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, PassMessage> {
        let (win_w, win_h) = settings.get_window_size("pass", 820, 600);

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

        // Themed search input
        let input = text_input("Search password store...", &self.search_query)
            .id(PASS_SEARCH_ID)
            .on_input(PassMessage::SearchChanged)
            .on_submit(if let Some(first) = self.filtered_keys.first() {
                PassMessage::CopyKey(first.clone())
            } else {
                PassMessage::Dismiss
            })
            .padding(Padding {
                top: 12.0,
                right: 24.0,
                bottom: 12.0,
                left: 24.0,
            })
            .size((theme.font_size_overlay) as f32)
            .width(Length::Fill)
            .style(move |_t, status| text_input::Style {
                background: iced::Background::Color(theme.surface_alt),
                border: iced::Border {
                    color: match status {
                        text_input::Status::Focused { .. } => theme.focus,
                        text_input::Status::Hovered => theme.border,
                        _ => theme.border,
                    },
                    width: theme.control_border_width,
                    radius: 8.0.into(),
                },
                icon: theme.muted,
                placeholder: theme.muted,
                value: theme.text,
                selection: theme.focus,
            });

        let input_row = container(input).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: search_inward,
            bottom: 0.0,
            left: search_inward,
        });

        // Slanted list of pass entries
        let mut list_col = column![].spacing(6).width(Length::Fill);
        for key in self.filtered_keys.iter().take(50) {
            let k_clone = key.clone();

            let item_bg = ShapeBox::new()
                .shape(ShapeType::Slant {
                    left: SlantSide::Left,
                    right: SlantSide::Left,
                })
                .background(theme.background)
                .border(theme.border, theme.control_border_width)
                .slant_width(8.0)
                .size(Length::Fill, Length::Fixed(48.0));

            let item_content = container(
                row![
                    text(">").size(16).color(theme.success),
                    text(key)
                        .size((theme.font_size_overlay) as f32)
                        .color(theme.text),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .padding(Padding {
                    top: 0.0,
                    right: 24.0,
                    bottom: 0.0,
                    left: 24.0,
                }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center);

            let item = button(Stack::new().push(item_bg.view()).push(item_content))
                .on_press(PassMessage::CopyKey(k_clone))
                .padding(0)
                .width(Length::Fill)
                .height(Length::Fixed(48.0))
                .style(|_t, status| {
                    let is_hovered =
                        matches!(status, button::Status::Hovered | button::Status::Pressed);
                    button::Style {
                        background: None,
                        border: iced::Border {
                            color: if is_hovered {
                                Color::WHITE
                            } else {
                                Color::TRANSPARENT
                            },
                            width: 0.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    }
                });

            list_col = list_col.push(item);
        }

        let list_container = container(
            scrollable(list_col)
                .height(Length::Fill)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: list_top_inset,
            right: list_side_inset,
            bottom: list_bottom_inset,
            left: list_side_inset,
        });

        let footer = container(
            text("Enter to copy  •  ESC to dismiss")
                .size(11)
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![input_row, list_container, footer]
                .spacing(12)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .padding(Padding {
            top: pad_top,
            right: 0.0,
            bottom: pad_bottom,
            left: 0.0,
        })
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
