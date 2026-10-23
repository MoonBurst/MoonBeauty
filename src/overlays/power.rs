use iced::widget::{button, column, container, row, text, Stack};
use iced::{Alignment, Color, Element, Length};
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Sleep,
    Logout,
    Reboot,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum PowerMessage {
    Select(PowerAction),
    Confirm(PowerAction),
    Dismiss,
}

pub struct PowerOverlay {
    pub confirming: Option<PowerAction>,
}

impl Default for PowerOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerOverlay {
    pub fn new() -> Self {
        Self { confirming: None }
    }

    pub fn update(&mut self, message: PowerMessage) {
        match message {
            PowerMessage::Select(act) => {
                match act {
                    PowerAction::Lock | PowerAction::Sleep => {
                        Self::execute(act);
                    }
                    PowerAction::Logout | PowerAction::Reboot | PowerAction::Shutdown => {
                        if self.confirming == Some(act) {
                            Self::execute(act);
                            self.confirming = None;
                        } else {
                            self.confirming = Some(act);
                        }
                    }
                }
            }
            PowerMessage::Confirm(act) => {
                Self::execute(act);
                self.confirming = None;
            }
            PowerMessage::Dismiss => {
                self.confirming = None;
            }
        }
    }

    pub fn execute(action: PowerAction) {
        tokio::spawn(async move {
            match action {
                PowerAction::Lock => {
                    // Triggers session lockscreen
                }
                PowerAction::Sleep => {
                    let _ = Command::new("systemctl").arg("suspend").status().await;
                }
                PowerAction::Reboot => {
                    let _ = Command::new("systemctl").arg("reboot").status().await;
                }
                PowerAction::Shutdown => {
                    let _ = Command::new("systemctl").arg("poweroff").status().await;
                }
                PowerAction::Logout => {
                    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
                        let _ = Command::new("hyprctl").args(["dispatch", "exit"]).status().await;
                    } else if std::env::var("SWAYSOCK").is_ok() {
                        let _ = Command::new("swaymsg").arg("exit").status().await;
                    } else {
                        let _ = Command::new("loginctl").args(["terminate-session", "self"]).status().await;
                    }
                }
            }
        });
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, PowerMessage> {
        let actions = [
            (PowerAction::Lock, "🔒", "Lock Session", "Lock display with PAM authentication"),
            (PowerAction::Sleep, "🌙", "Sleep", "Suspend system into standby"),
            (PowerAction::Logout, "🚪", "Log Out", "Exit current desktop compositor"),
            (PowerAction::Reboot, "🔄", "Reboot", "Restart the computer and OS"),
            (PowerAction::Shutdown, "⏻", "Power Off", "Safely shut down power"),
        ];

        let mut list_col = column![].spacing(10);
        for (action, icon, title, desc) in actions {
            let is_confirming = self.confirming == Some(action);
            let display_title = if is_confirming {
                format!("⚠️ Confirm {}?", title)
            } else {
                title.to_string()
            };

            let title_col = if is_confirming { theme.danger } else { theme.text };

            let row_item = button(
                row![
                    text(icon).size(26),
                    column![
                        text(display_title).size((theme.font_size_overlay) as f32).bold().color(title_col),
                        text(desc).size((theme.font_size_normal - 3) as f32).color(theme.muted),
                    ]
                    .spacing(2),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            )
            .on_press(PowerMessage::Select(action))
            .padding(14)
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
                text("⚡ POWER & SESSION")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.text),
                list_col,
                text("Press action to execute  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(24),
        )
        .width(Length::Fixed(680.0))
        .height(Length::Fixed(520.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(PowerMessage::Dismiss)
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
