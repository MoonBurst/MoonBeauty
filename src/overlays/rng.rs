use chrono::Local;
use iced::widget::{button, column, container, row, text, Stack};
use iced::{Alignment, Color, Element, Length};
use std::fs::File;
use std::io::Read;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone)]
pub struct RollHistoryItem {
    pub time_str: String,
    pub spec: String,
    pub total: i32,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub enum RngMessage {
    DiceCountChanged(String),
    StrengthChanged(String),
    FlatModChanged(String),
    SelectSides(u32),
    SelectMode(String),
    ExecuteRoll,
    Dismiss,
}

pub struct RngOverlay {
    pub dice_count: u32,
    pub selected_sides: u32,
    pub strength: i32,
    pub flat_mod: i32,
    pub keep_mode: String,
    pub last_total: i32,
    pub last_rolls: Vec<(i32, bool)>,
    pub last_type_str: String,
    pub history: Vec<RollHistoryItem>,
}

impl Default for RngOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl RngOverlay {
    pub fn new() -> Self {
        Self {
            dice_count: 1,
            selected_sides: 20,
            strength: 0,
            flat_mod: 0,
            keep_mode: "all".into(),
            last_total: 0,
            last_rolls: Vec::new(),
            last_type_str: String::new(),
            history: Vec::new(),
        }
    }

    fn random_u32() -> u32 {
        let mut buf = [0u8; 4];
        if let Ok(mut f) = File::open("/dev/urandom") {
            if f.read_exact(&mut buf).is_ok() {
                return u32::from_ne_bytes(buf);
            }
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    }

    pub fn roll(&mut self) {
        let count = self.dice_count.clamp(1, 100);
        let sides = self.selected_sides.clamp(2, 1000);

        if sides == 2 {
            let mut heads = 0;
            let mut tails = 0;
            let mut rolls = Vec::new();

            for _ in 0..count {
                if Self::random_u32().is_multiple_of(2) {
                    heads += 1;
                    rolls.push((1, true));
                } else {
                    tails += 1;
                    rolls.push((0, true));
                }
            }

            self.last_total = heads;
            self.last_rolls = rolls;
            self.last_type_str = format!("{} Coin Flips", count);
            let summary = format!("{} 🪙 Heads / {} 🪙 Tails", heads, tails);

            self.history.insert(
                0,
                RollHistoryItem {
                    time_str: Local::now().format("%H:%M:%S").to_string(),
                    spec: self.last_type_str.clone(),
                    total: heads,
                    summary,
                },
            );
            return;
        }

        let mut rolls: Vec<(i32, bool)> = (0..count)
            .map(|_| {
                let r = ((Self::random_u32() % sides) + 1) as i32;
                let adjusted = (r + self.strength).clamp(1, sides as i32) + self.flat_mod;
                (adjusted, true)
            })
            .collect();

        if self.keep_mode == "kh" {
            rolls.sort_by_key(|r| std::cmp::Reverse(r.0));
            for r in rolls.iter_mut().skip(1) {
                r.1 = false;
            }
        } else if self.keep_mode == "kl" {
            rolls.sort_by_key(|a| a.0);
            for r in rolls.iter_mut().skip(1) {
                r.1 = false;
            }
        }

        let total: i32 = rolls.iter().filter(|r| r.1).map(|r| r.0).sum();
        let summary: Vec<String> = rolls
            .iter()
            .map(|r| {
                if r.1 {
                    r.0.to_string()
                } else {
                    format!("({})", r.0)
                }
            })
            .collect();

        self.last_total = total;
        self.last_rolls = rolls;
        self.last_type_str = format!("{}d{}", count, sides);

        self.history.insert(
            0,
            RollHistoryItem {
                time_str: Local::now().format("%H:%M:%S").to_string(),
                spec: self.last_type_str.clone(),
                total,
                summary: summary.join(", "),
            },
        );
    }

    pub fn update(&mut self, message: RngMessage) {
        match message {
            RngMessage::DiceCountChanged(val) => {
                self.dice_count = val.parse().unwrap_or(1).clamp(1, 100);
            }
            RngMessage::StrengthChanged(val) => {
                self.strength = val.parse().unwrap_or(0);
            }
            RngMessage::FlatModChanged(val) => {
                self.flat_mod = val.parse().unwrap_or(0);
            }
            RngMessage::SelectSides(s) => {
                self.selected_sides = s;
            }
            RngMessage::SelectMode(m) => {
                self.keep_mode = m;
            }
            RngMessage::ExecuteRoll => {
                self.roll();
            }
            RngMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, RngMessage> {
        let dice_types = [
            ("🪙 Coin", 2),
            ("d4", 4),
            ("d6", 6),
            ("d8", 8),
            ("d10", 10),
            ("d12", 12),
            ("d20", 20),
            ("d100", 100),
        ];

        let mut dice_buttons_row = row![].spacing(8);
        for (label, sides) in dice_types {
            let is_sel = self.selected_sides == sides;
            let col = if is_sel { theme.success } else { theme.text };
            let btn_item = button(text(label).size(14).bold().color(col))
                .on_press(RngMessage::SelectSides(sides))
                .padding([8, 14])
                .style(|_t, _s| button::Style { background: None, ..Default::default() });
            dice_buttons_row = dice_buttons_row.push(btn_item);
        }

        let mode_row = row![
            text("Mode:").size((theme.font_size_normal) as f32).bold().color(theme.text),
            button(text("Keep All").size(12).color(if self.keep_mode == "all" { theme.success } else { theme.text }))
                .on_press(RngMessage::SelectMode("all".into()))
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("Advantage").size(12).color(if self.keep_mode == "kh" { theme.success } else { theme.text }))
                .on_press(RngMessage::SelectMode("kh".into()))
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("Disadvantage").size(12).color(if self.keep_mode == "kl" { theme.success } else { theme.text }))
                .on_press(RngMessage::SelectMode("kl".into()))
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let roll_btn = button(
            text("🎲 ROLL OUTCOME")
                .size((theme.font_size_overlay) as f32)
                .bold()
                .color(theme.success)
                .align_x(Alignment::Center),
        )
        .on_press(RngMessage::ExecuteRoll)
        .padding(14)
        .width(Length::Fill)
        .style(|_t, _s| button::Style { background: None, ..Default::default() });

        let result_box = row![
            text(format!("{}: ", self.last_type_str)).size(16).color(theme.muted),
            text(format!("TOTAL: {}", self.last_total)).size(22).bold().color(theme.success),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("🎲 DICE, COIN & RNG ROLLER")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.text),
                dice_buttons_row,
                mode_row,
                roll_btn,
                result_box,
                text("ESC to dismiss  •  Enter to roll")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(16)
            .padding(24),
        )
        .width(Length::Fixed(680.0))
        .height(Length::Fixed(420.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(RngMessage::Dismiss)
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
