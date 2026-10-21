use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use std::path::PathBuf;
use tokio::process::Command;
use walkdir::WalkDir;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone)]
pub enum PassMessage {
    SearchChanged(String),
    CopyKey(String),
    Dismiss,
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

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, PassMessage> {
        let input = text_input("Search password store... [Enter to copy]", &self.search_query)
            .on_input(PassMessage::SearchChanged)
            .on_submit(
                if let Some(first) = self.filtered_keys.first() {
                    PassMessage::CopyKey(first.clone())
                } else {
                    PassMessage::Dismiss
                },
            )
            .padding(14)
            .size(theme.font_size_overlay);

        let mut list_col = column![].spacing(6);
        for key in self.filtered_keys.iter().take(40) {
            let k_clone = key.clone();
            let row_item = button(
                row![
                    text("🔒").size(16),
                    text(key).size(theme.font_size_overlay).color(theme.text),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .on_press(PassMessage::CopyKey(k_clone))
            .padding([8, 14])
            .width(Length::Fill)
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

            list_col = list_col.push(row_item);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("🔑 PASSWORD STORE")
                    .size(theme.font_size_overlay)
                    .color(theme.text),
                input,
                scrollable(list_col).height(Length::Fill),
                text("Click or Enter to copy credential  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(820.0))
        .height(Length::Fixed(600.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(PassMessage::Dismiss)
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
