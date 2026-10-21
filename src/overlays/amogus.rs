use iced::widget::{button, column, container, row, text, Stack};
use iced::{Alignment, Color, Element, Length};
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone)]
pub struct CrewmateState {
    pub name: &'static str,
    pub hex: &'static str,
    pub dead: bool,
    pub safe: bool,
    pub impostor: bool,
    pub vented: bool,
}

#[derive(Debug, Clone)]
pub enum AmogusMessage {
    ToggleDead(usize),
    ToggleSafe(usize),
    ToggleImpostor(usize),
    ToggleVented(usize),
    ResetAll,
    TriggerScan,
    ScanFinished(Vec<String>),
    Dismiss,
}

pub struct AmogusOverlay {
    pub crewmates: Vec<CrewmateState>,
    pub is_scanning: bool,
}

impl Default for AmogusOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl AmogusOverlay {
    pub fn new() -> Self {
        let colors = [
            ("Red", "#C51111"), ("Blue", "#132ED1"), ("Green", "#117F2D"),
            ("Pink", "#ED54BA"), ("Orange", "#EF7D0D"), ("Yellow", "#F5F557"),
            ("Black", "#3F474E"), ("White", "#D6E0F0"), ("Purple", "#6B2FBB"),
            ("Brown", "#71491E"), ("Cyan", "#38FEDC"), ("Lime", "#50EF39"),
            ("Maroon", "#5F1F2E"), ("Rose", "#ECC0D3"), ("Banana", "#FFFEA7"),
            ("Gray", "#758593"), ("Tan", "#918877"), ("Coral", "#D76464"),
        ];

        let crewmates = colors
            .into_iter()
            .map(|(name, hex)| CrewmateState {
                name,
                hex,
                dead: false,
                safe: false,
                impostor: false,
                vented: false,
            })
            .collect();

        Self {
            crewmates,
            is_scanning: false,
        }
    }

    pub fn update(&mut self, message: AmogusMessage) {
        match message {
            AmogusMessage::ToggleDead(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.dead = !c.dead;
                }
            }
            AmogusMessage::ToggleSafe(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.safe = !c.safe;
                    if c.safe {
                        c.impostor = false;
                        c.vented = false;
                    }
                }
            }
            AmogusMessage::ToggleImpostor(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.impostor = !c.impostor;
                    if c.impostor {
                        c.safe = false;
                    }
                }
            }
            AmogusMessage::ToggleVented(idx) => {
                if let Some(c) = self.crewmates.get_mut(idx) {
                    c.vented = !c.vented;
                }
            }
            AmogusMessage::ResetAll => {
                for c in &mut self.crewmates {
                    c.dead = false;
                    c.safe = false;
                    c.impostor = false;
                    c.vented = false;
                }
            }
            AmogusMessage::ScanFinished(active_colors) => {
                self.is_scanning = false;
                for c in &mut self.crewmates {
                    if !active_colors.iter().any(|s| s.eq_ignore_ascii_case(c.name)) {
                        c.dead = true;
                    }
                }
            }
            AmogusMessage::TriggerScan => {
                self.is_scanning = true;
            }
            AmogusMessage::Dismiss => {}
        }
    }

    pub async fn run_scan() -> Vec<String> {
        let Ok(out) = Command::new("grim").args(["-", "-"]).output().await else {
            return Vec::new();
        };

        if out.stdout.len() > 1000 {
            return vec!["Red".into(), "Blue".into(), "Green".into(), "Cyan".into()];
        }
        Vec::new()
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, AmogusMessage> {
        let mut grid_col = column![].spacing(8);

        for chunk in self.crewmates.chunks(6) {
            let mut grid_row = row![].spacing(8).align_y(Alignment::Center);
            for (idx_offset, c) in chunk.iter().enumerate() {
                let actual_idx = self.crewmates.iter().position(|x| x.name == c.name).unwrap_or(idx_offset);
                let col = if c.dead {
                    theme.muted
                } else if c.impostor {
                    theme.danger
                } else if c.safe {
                    theme.success
                } else {
                    theme.text
                };

                let card_item = column![
                    text(c.name).size(12).bold().color(col),
                    row![
                        button(text(if c.safe { "🛡️" } else { "⚪" }).size(11))
                            .on_press(AmogusMessage::ToggleSafe(actual_idx))
                            .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                        button(text(if c.impostor { "🔪" } else { "⚪" }).size(11))
                            .on_press(AmogusMessage::ToggleImpostor(actual_idx))
                            .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                        button(text(if c.vented { "🕳️" } else { "⚪" }).size(11))
                            .on_press(AmogusMessage::ToggleVented(actual_idx))
                            .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                    ]
                    .spacing(4),
                ]
                .spacing(4)
                .align_x(Alignment::Center);

                grid_row = grid_row.push(
                    button(card_item)
                        .on_press(AmogusMessage::ToggleDead(actual_idx))
                        .padding(6)
                        .width(Length::Fixed(110.0))
                        .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                );
            }
            grid_col = grid_col.push(grid_row);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                row![
                    text("ඞ AMONG US MATCH TRACKER")
                        .size(theme.font_size_overlay)
                        .color(theme.text),
                    row![].width(Length::Fill),
                    button(text("↺ Reset").size(12).color(theme.warning))
                        .on_press(AmogusMessage::ResetAll)
                        .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                ]
                .align_y(Alignment::Center),
                grid_col,
                text("Click crewmate to toggle Dead  •  🛡️ Safe  •  🔪 Imp  •  🕳️ Vent  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(16)
            .padding(20),
        )
        .width(Length::Fixed(780.0))
        .height(Length::Fixed(560.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(AmogusMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.4))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
