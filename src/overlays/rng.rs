use chrono::Local;
use iced::widget::{button, column, container, row, scrollable, text, Stack};
use iced::{Alignment, Element, Length, Padding};
use std::fs::File;
use std::io::Read;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepMode { All, Highest, Lowest }

#[derive(Debug, Clone)]
pub struct RollHistoryItem {
    pub time_str: String,
    pub type_str: String,
    pub total: i32,
    pub avg: f32,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub enum RngMessage {
    SetDiceCount(u32),
    SetStrength(i32),
    SetFlatMod(i32),
    SetKeepMode(KeepMode),
    SetKeepN(u32),
    SelectSides(u32),
    ExecuteRoll,
    ToggleHistory,
    CycleFieldPrev,
    CycleFieldNext,
    AdjustField(bool), // true = up, false = down
    Dismiss,
}

pub struct RngOverlay {
    pub dice_count: u32,
    pub selected_sides: u32,
    pub strength: i32,
    pub flat_mod: i32,
    pub keep_mode: KeepMode,
    pub keep_n: u32,
    pub show_history: bool,
    pub active_field: usize, // 0=Qty, 1=Str, 2=Mod, 3=KeepN
    pub last_total: i32,
    pub last_rolls: Vec<(String, bool)>, // (value, is_kept)
    pub last_type_str: String,
    pub last_avg: f32,
    pub history: Vec<RollHistoryItem>,
}

impl Default for RngOverlay {
    fn default() -> Self { Self::new() }
}

impl RngOverlay {
    pub fn new() -> Self {
        Self {
            dice_count: 1,
            selected_sides: 6,
            strength: 0,
            flat_mod: 0,
            keep_mode: KeepMode::All,
            keep_n: 1,
            show_history: false,
            active_field: 0,
            last_total: 0,
            last_rolls: Vec::new(),
            last_type_str: "None".into(),
            last_avg: 0.0,
            history: Vec::new(),
        }
    }

    fn random_u32() -> u32 {
        let mut buf = [0u8; 4];
        if let Ok(mut f) = File::open("/dev/urandom") {
            if f.read_exact(&mut buf).is_ok() { return u32::from_ne_bytes(buf); }
        }
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos()
    }

    pub fn roll(&mut self) {
        let count = self.dice_count.clamp(1, 100);
        let sides = self.selected_sides.clamp(2, 1000);

        if sides == 2 {
            let mut heads = 0;
            let mut rolls = Vec::new();
            for _ in 0..count {
                if Self::random_u32().is_multiple_of(2) {
                    heads += 1;
                    rolls.push(("Heads".to_string(), true));
                } else {
                    rolls.push(("Tails".to_string(), true));
                }
            }
            self.last_total = heads;
            self.last_avg = (heads as f32) / (count as f32);
            self.last_rolls = rolls;
            self.last_type_str = format!("{} Coin Flip{}", count, if count == 1 { "" } else { "s" });
        } else {
            let mut rolls: Vec<(i32, bool)> = (0..count).map(|_| {
                let raw = (Self::random_u32() % sides) as i32 + 1;
                let adjusted = (raw + self.strength).clamp(1, sides as i32);
                (adjusted, true)
            }).collect();

            let keep_n = (self.keep_n.min(count)).max(1) as usize;
            match self.keep_mode {
                KeepMode::All => {}
                KeepMode::Highest => {
                    rolls.sort_by_key(|r| std::cmp::Reverse(r.0));
                    for r in rolls.iter_mut().skip(keep_n) { r.1 = false; }
                }
                KeepMode::Lowest => {
                    rolls.sort_by_key(|r| r.0);
                    for r in rolls.iter_mut().skip(keep_n) { r.1 = false; }
                }
            }

            // Sort naturally for display
            rolls.sort_by_key(|r| r.0);

            let kept_sum: i32 = rolls.iter().filter(|r| r.1).map(|r| r.0).sum();
            let kept_num = rolls.iter().filter(|r| r.1).count();

            self.last_total = kept_sum + self.flat_mod;
            self.last_avg = if kept_num > 0 { (kept_sum as f32) / (kept_num as f32) } else { 0.0 };
            self.last_type_str = format!("{}d{}{}", count, sides, match self.keep_mode {
                KeepMode::All => "".to_string(),
                KeepMode::Highest => format!(" (kh{})", keep_n),
                KeepMode::Lowest => format!(" (kl{})", keep_n),
            });
            self.last_rolls = rolls.into_iter().map(|(v, k)| (v.to_string(), k)).collect();
        }

        let summary: Vec<String> = self.last_rolls.iter()
            .map(|(v, k)| if *k { v.clone() } else { format!("({})", v) })
            .collect();

        self.history.insert(0, RollHistoryItem {
            time_str: Local::now().format("%H:%M:%S").to_string(),
            type_str: self.last_type_str.clone(),
            total: self.last_total,
            avg: self.last_avg,
            summary: summary.join(", "),
        });
        if self.history.len() > 50 { self.history.truncate(50); }
    }

    pub fn update(&mut self, message: RngMessage) {
        match message {
            RngMessage::SetDiceCount(n) => { self.dice_count = n.clamp(1, 100); if self.keep_n > self.dice_count { self.keep_n = self.dice_count; } }
            RngMessage::SetStrength(n) => self.strength = n.clamp(-100, 100),
            RngMessage::SetFlatMod(n) => self.flat_mod = n.clamp(-100, 100),
            RngMessage::SetKeepMode(m) => self.keep_mode = m,
            RngMessage::SetKeepN(n) => self.keep_n = n.clamp(1, self.dice_count.max(1)),
            RngMessage::SelectSides(s) => self.selected_sides = s,
            RngMessage::ExecuteRoll => self.roll(),
            RngMessage::ToggleHistory => self.show_history = !self.show_history,
            RngMessage::CycleFieldPrev => {
                let max = if self.keep_mode == KeepMode::All { 3 } else { 4 };
                self.active_field = if self.active_field == 0 { max - 1 } else { self.active_field - 1 };
            }
            RngMessage::CycleFieldNext => {
                let max = if self.keep_mode == KeepMode::All { 3 } else { 4 };
                self.active_field = (self.active_field + 1) % max;
            }
            RngMessage::AdjustField(up) => {
                let delta = if up { 1 } else { -1 };
                match self.active_field {
                    0 => self.update(RngMessage::SetDiceCount((self.dice_count as i32 + delta).max(1) as u32)),
                    1 => self.update(RngMessage::SetStrength(self.strength + delta)),
                    2 => self.update(RngMessage::SetFlatMod(self.flat_mod + delta)),
                    3 => self.update(RngMessage::SetKeepN((self.keep_n as i32 + delta).max(1) as u32)),
                    _ => {}
                }
            }
            RngMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &ShellSettings) -> Element<'a, RngMessage> {
        let (win_w, win_h) = settings.get_window_size("rng", 680, 720);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top { (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0 } else { 16.0 };
        let footer_inset = if dy > pad_bottom { (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0 } else { 16.0 };
        let body_inset = header_inset.min(footer_inset) + 4.0;

        let fs = theme.font_size_overlay as f32;
        let fs_sm = fs * 0.85;

        // Stepper helper
        let stepper = |label: &'static str, val: i32, active: bool, idx: usize| -> Element<'a, RngMessage> {
            let border = if active { theme.success } else { theme.border };
            let bg = if active { theme.surface_alt } else { theme.background };
            column![
                text(label).size(fs_sm).bold().color(theme.text),
                container(
                    row![
                        button(text("−").size(fs).color(theme.text))
                            .on_press(if idx==0 {RngMessage::SetDiceCount((val-1).max(1) as u32)} else if idx==1 {RngMessage::SetStrength(val-1)} else if idx==2 {RngMessage::SetFlatMod(val-1)} else {RngMessage::SetKeepN((val-1).max(1) as u32)})
                            .padding([4, 8]).style(|_t, _s| button::Style { background: None, ..Default::default() }),
                        container(text(val.to_string()).size(fs).bold().color(theme.text)).width(Length::Fill).center_x(Length::Fill),
                        button(text("+").size(fs).color(theme.text))
                            .on_press(if idx==0 {RngMessage::SetDiceCount((val+1).min(100) as u32)} else if idx==1 {RngMessage::SetStrength(val+1)} else if idx==2 {RngMessage::SetFlatMod(val+1)} else {RngMessage::SetKeepN((val+1).min(self.dice_count as i32) as u32)})
                            .padding([4, 8]).style(|_t, _s| button::Style { background: None, ..Default::default() }),
                    ].align_y(Alignment::Center)
                )
                .height(Length::Fixed(36.0))
                .width(Length::Fill)
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { color: border, width: 1.5, radius: 4.0.into() },
                    ..Default::default()
                })
            ].spacing(4).width(Length::Fill).into()
        };

        let mut steppers = row![
            stepper("Quantity", self.dice_count as i32, self.active_field == 0, 0),
            stepper("Strength", self.strength, self.active_field == 1, 1),
            stepper("Flat Mod", self.flat_mod, self.active_field == 2, 2),
        ].spacing(10);

        if self.keep_mode != KeepMode::All {
            steppers = steppers.push(stepper("Keep N", self.keep_n as i32, self.active_field == 3, 3));
        }

        let mode_btn = |m: KeepMode, label: &'static str| -> Element<'a, RngMessage> {
            let is_sel = self.keep_mode == m;
            let bg = if is_sel { theme.surface_alt } else { theme.background };
            let border = if is_sel { theme.text } else { theme.border };
            button(text(label).size(fs_sm).bold().color(theme.text))
                .on_press(RngMessage::SetKeepMode(m))
                .padding([8, 12])
                .width(Length::FillPortion(1))
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border { color: border, width: 1.0, radius: 4.0.into() },
                    ..Default::default()
                }).into()
        };

        let mode_row = row![
            mode_btn(KeepMode::All, "Keep All"),
            mode_btn(KeepMode::Highest, "Advantage"),
            mode_btn(KeepMode::Lowest, "Disadvantage"),
        ].spacing(8).width(Length::Fill);

        let dice_types = [("🪙 Coin", 2), ("d4", 4), ("d6", 6), ("d8", 8), ("d10", 10), ("d12", 12), ("d20", 20), ("d100", 100)];
        let mut dice_grid = column![].spacing(8);
        for chunk in dice_types.chunks(4) {
            let mut r = row![].spacing(8).width(Length::Fill);
            for &(label, sides) in chunk {
                let is_sel = self.selected_sides == sides;
                let bg = if is_sel { theme.surface_alt } else { theme.background };
                let border = if is_sel { theme.text } else { theme.border };
                let col = if is_sel { theme.text } else { theme.success };
                r = r.push(
                    button(text(label).size(fs).bold().color(col))
                        .on_press(RngMessage::SelectSides(sides))
                        .padding([8, 0])
                        .width(Length::FillPortion(1))
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(bg)),
                            border: iced::Border { color: border, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        })
                );
            }
            // Fill missing columns
            for _ in chunk.len()..4 { r = r.push(container(text("")).width(Length::FillPortion(1))); }
            dice_grid = dice_grid.push(r);
        }

        let roll_btn = button(text("🎲 ROLL OUTCOME").size(fs * 1.2).bold().color(theme.success))
            .on_press(RngMessage::ExecuteRoll)
            .padding(12)
            .width(Length::Fill)
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(theme.background)),
                border: iced::Border { color: theme.success, width: 2.0, radius: 6.0.into() },
                text_color: theme.success,
                ..Default::default()
            });

        // Results view
        let results_ui: Element<'a, RngMessage> = if self.last_rolls.is_empty() {
            container(text("Press Roll to Generate Results").size(fs).color(theme.muted))
                .width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).into()
        } else {
            let mut results_col = column![].spacing(10);
            results_col = results_col.push(
                row![
                    text(&self.last_type_str).size(fs * 1.2).color(theme.text),
                    iced::widget::space::horizontal(),
                    if self.selected_sides == 2 {
                        text(format!("{} Heads", self.last_total)).size(fs * 1.5).bold().color(theme.success)
                    } else {
                        text(format!("TOTAL: {}", self.last_total)).size(fs * 1.5).bold().color(theme.success)
                    }
                ].align_y(Alignment::End)
            );

            let mut flow = column![].spacing(6);
            for chunk in self.last_rolls.chunks(10) {
                let mut r = row![].spacing(6);
                for (val, kept) in chunk {
                    let bg = if *kept { theme.background } else { theme.surface_alt };
                    let border = if *kept { theme.success } else { theme.muted };
                    let col = if *kept { theme.text } else { theme.muted };
                    r = r.push(
                        container(text(val).size(fs).bold().color(col))
                            .padding([4, 8])
                            .style(move |_t| container::Style {
                                background: Some(iced::Background::Color(bg)),
                                border: iced::Border { color: border, width: 1.0, radius: 4.0.into() },
                                ..Default::default()
                            })
                    );
                }
                flow = flow.push(r);
            }
            results_col = results_col.push(scrollable(flow).height(Length::Fill));
            container(results_col).width(Length::Fill).height(Length::Fill).into()
        };

        // History view
        let history_ui: Element<'a, RngMessage> = {
            let mut h_col = column![].spacing(6);
            for item in &self.history {
                h_col = h_col.push(
                    container(
                        row![
                            text(format!("{} • {}", item.time_str, item.type_str)).size(fs_sm).bold().color(theme.text),
                            iced::widget::space::horizontal(),
                            text(&item.summary).size(fs_sm).color(theme.success).bold()
                        ]
                    ).padding([8, 12]).width(Length::Fill)
                    .style(move |_t| container::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border { color: theme.border, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    })
                );
            }
            if self.history.is_empty() {
                h_col = h_col.push(container(text("No history yet").size(fs).color(theme.muted)).padding(20));
            }
            container(scrollable(h_col).height(Length::Fill)).width(Length::Fill).height(Length::Fill).into()
        };

        let header_btn = |label: String| -> Element<'a, RngMessage> {
            button(text(label).size(fs_sm).bold().color(theme.text))
                .on_press(RngMessage::ToggleHistory)
                .padding([6, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.surface_alt)),
                    border: iced::Border { color: theme.border, width: 1.0, radius: 4.0.into() },
                    ..Default::default()
                }).into()
        };

        let header_row = container(
            row![
                text("🎲 Dice, Coin & RNG").size(fs * 1.2).bold().color(theme.text),
                iced::widget::space::horizontal(),
                header_btn(if self.show_history { "🎲 Roller".into() } else { "📜 History".into() }),
            ].align_y(Alignment::Center)
        ).width(Length::Fill).padding(Padding { top: 0.0, right: header_inset, bottom: 0.0, left: header_inset });

        let main_body: Element<'a, RngMessage> = if self.show_history {
            container(history_ui).padding(Padding { top: 0.0, right: body_inset, bottom: 0.0, left: body_inset }).into()
        } else {
            container(
                column![
                    steppers,
                    mode_row,
                    dice_grid,
                    roll_btn,
                    container(results_ui).width(Length::Fill).height(Length::Fill).padding([10, 0])
                ].spacing(16).width(Length::Fill)
            ).padding(Padding { top: 0.0, right: body_inset, bottom: 0.0, left: body_inset }).into()
        };

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background).border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![
                header_row,
                main_body,
                container(text("←/→ Change Field  •  ↑/↓ Adjust Value  •  Enter Roll  •  ESC Dismiss").size(fs_sm * 0.85).color(theme.muted).align_x(Alignment::Center))
                    .width(Length::Fill).padding(Padding { top: 12.0, right: footer_inset, bottom: 0.0, left: footer_inset })
            ].spacing(14).padding(Padding { top: pad_top, right: 0.0, bottom: pad_bottom, left: 0.0 })
        ).width(Length::Fixed(win_w)).height(Length::Fixed(win_h)).style(|_t| iced::widget::container::Style::default());

        let card = Stack::new().push(card_bg.view()).push(card_content);
        container(card).center_x(Length::Fill).center_y(Length::Fill).style(|_t| iced::widget::container::Style::default()).into()
    }
}
