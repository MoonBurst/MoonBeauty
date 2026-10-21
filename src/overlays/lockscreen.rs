use chrono::Local;
use iced::widget::{column, container, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone)]
pub enum LockMessage {
    PasswordInputChanged(String),
    SubmitUnlock,
    UnlockSuccess,
    UnlockFailed,
}

pub struct LockscreenOverlay {
    pub password_buffer: String,
    pub is_locked: bool,
    pub auth_failed: bool,
    pub is_caps_lock: bool,
}

impl Default for LockscreenOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl LockscreenOverlay {
    pub fn new() -> Self {
        Self {
            password_buffer: String::new(),
            is_locked: false,
            auth_failed: false,
            is_caps_lock: Self::check_caps_lock(),
        }
    }

    pub fn check_caps_lock() -> bool {
        for i in 0..6 {
            let p = format!("/sys/class/leds/input{}::capslock/brightness", i);
            if let Ok(content) = std::fs::read_to_string(p) {
                if content.trim() == "1" {
                    return true;
                }
            }
        }
        false
    }

    pub async fn authenticate_pam(password: &str) -> bool {
        let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
        let clean_pass = password.trim();
        if clean_pass.is_empty() {
            return false;
        }

        let child = Command::new("su")
            .args(["-c", "true", &user])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok();

        if let Some(mut c) = child {
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = c.stdin.take() {
                let _ = stdin.write_all(format!("{}\n", clean_pass).as_bytes()).await;
            }
            if let Ok(st) = c.wait().await {
                return st.success();
            }
        }
        false
    }

    pub async fn set_dpms(on: bool) {
        let state = if on { "on" } else { "off" };
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            let _ = Command::new("hyprctl").args(["dispatch", "dpms", state]).status().await;
        } else if std::env::var("SWAYSOCK").is_ok() {
            let _ = Command::new("swaymsg").args(["output", "*", "dpms", state]).status().await;
        }
    }

    pub fn update(&mut self, message: LockMessage) {
        match message {
            LockMessage::PasswordInputChanged(p) => {
                self.password_buffer = p;
                self.auth_failed = false;
                self.is_caps_lock = Self::check_caps_lock();
            }
            LockMessage::UnlockSuccess => {
                self.is_locked = false;
                self.password_buffer.clear();
                self.auth_failed = false;
                tokio::spawn(async {
                    Self::set_dpms(true).await;
                });
            }
            LockMessage::UnlockFailed => {
                self.password_buffer.clear();
                self.auth_failed = true;
            }
            _ => {}
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, LockMessage> {
        let time_str = Local::now().format("%H:%M").to_string();
        let date_str = Local::now().format("%A, %B %d").to_string();

        let placeholder = if self.auth_failed {
            "Authentication Failed..."
        } else {
            "Enter password..."
        };

        let input = text_input(placeholder, &self.password_buffer)
            .on_input(LockMessage::PasswordInputChanged)
            .on_submit(LockMessage::SubmitUnlock)
            .secure(true)
            .padding(14)
            .size(theme.font_size_overlay)
            .width(Length::Fixed(400.0));

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(if self.auth_failed { theme.danger } else { theme.border }, theme.border_width);

        let content = container(
            column![
                text(time_str).size(56).bold().color(theme.text).align_x(Alignment::Center),
                text(date_str).size(theme.font_size_overlay).color(theme.muted).align_x(Alignment::Center),
                input,
                if self.is_caps_lock {
                    text("⇪ CAPS LOCK ACTIVE").size(12).bold().color(theme.warning).align_x(Alignment::Center)
                } else {
                    text("").size(12)
                },
                text("Press [Enter] to unlock")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(18)
            .padding(32)
            .align_x(Alignment::Center),
        )
        .width(Length::Fixed(480.0));

        let card = Stack::new().push(card_bg.view()).push(content);

        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(10, 10, 12))),
                ..Default::default()
            })
            .into()
    }
}
