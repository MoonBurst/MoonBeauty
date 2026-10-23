use chrono::{Duration, Local};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderEntry {
    pub id: u64,
    pub target_epoch: i64,
    pub text: String,
    pub target_str: String,
    pub alerted: bool,
}

#[derive(Debug, Clone)]
pub enum ReminderMessage {
    TaskInputChanged(String),
    WhenInputChanged(String),
    QuickWhen(String),
    AddReminder,
    DeleteReminder(u64),
    Dismiss,
}

pub struct RemindersOverlay {
    pub task_text: String,
    pub when_text: String,
    pub entries: Vec<ReminderEntry>,
}

impl Default for RemindersOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl RemindersOverlay {
    pub fn new() -> Self {
        Self {
            task_text: String::new(),
            when_text: String::new(),
            entries: Self::load(),
        }
    }

    pub fn storage_path() -> PathBuf {
        let base = dirs::runtime_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("moon_reminders.json")
    }

    pub fn load() -> Vec<ReminderEntry> {
        let path = Self::storage_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(items) = serde_json::from_str::<Vec<ReminderEntry>>(&raw) {
                    return items;
                }
            }
        }
        Vec::new()
    }

    pub fn save(&self) {
        let path = Self::storage_path();
        if let Ok(data) = serde_json::to_string_pretty(&self.entries) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, path);
            }
        }
    }

    pub fn parse_when(when_expr: &str) -> (i64, String) {
        let now = Local::now();
        let expr = when_expr.trim().to_lowercase();

        let mut minutes: i64 = 0;
        let mut is_relative = false;

        for part in expr.split_whitespace() {
            if let Some(m) = part.strip_suffix('m') {
                if let Ok(val) = m.parse::<i64>() {
                    minutes += val;
                    is_relative = true;
                }
            } else if let Some(h) = part.strip_suffix('h') {
                if let Ok(val) = h.parse::<i64>() {
                    minutes += val * 60;
                    is_relative = true;
                }
            }
        }

        if is_relative && minutes > 0 {
            let target = now + Duration::minutes(minutes);
            return (
                target.timestamp(),
                target.format("%a, %b %d %I:%M %p").to_string(),
            );
        }

        let is_tomorrow = expr.contains("tomorrow");
        let target_date = if is_tomorrow {
            now.date_naive() + Duration::days(1)
        } else {
            now.date_naive()
        };

        if let Some(time_part) = expr.split_whitespace().find(|s| s.contains(':') || s.ends_with("am") || s.ends_with("pm")) {
            let mut hour = 9;
            let mut min = 0;

            if let Some((h_str, m_str)) = time_part.split_once(':') {
                let is_pm = m_str.ends_with("pm");
                let is_am = m_str.ends_with("am");
                let clean_m = m_str.trim_end_matches("am").trim_end_matches("pm");
                hour = h_str.parse().unwrap_or(9);
                min = clean_m.parse().unwrap_or(0);
                if is_pm && hour < 12 { hour += 12; }
                if is_am && hour == 12 { hour = 0; }
            } else if time_part.ends_with("pm") {
                hour = time_part.trim_end_matches("pm").parse().unwrap_or(9);
                if hour < 12 { hour += 12; }
            } else if time_part.ends_with("am") {
                hour = time_part.trim_end_matches("am").parse().unwrap_or(9);
                if hour == 12 { hour = 0; }
            }

            if let Some(target) = target_date.and_hms_opt(hour, min, 0) {
                let local_target = target.and_local_timezone(Local).unwrap();
                return (
                    local_target.timestamp(),
                    local_target.format("%a, %b %d %I:%M %p").to_string(),
                );
            }
        }

        let fallback = now + Duration::minutes(30);
        (fallback.timestamp(), fallback.format("%a, %b %d %I:%M %p").to_string())
    }

    pub fn update(&mut self, message: ReminderMessage) {
        match message {
            ReminderMessage::TaskInputChanged(txt) => {
                self.task_text = txt;
            }
            ReminderMessage::WhenInputChanged(w) => {
                self.when_text = w;
            }
            ReminderMessage::QuickWhen(w) => {
                self.when_text = w;
            }
            ReminderMessage::AddReminder => {
                let t = self.task_text.trim();
                let w = self.when_text.trim();
                if !t.is_empty() && !w.is_empty() {
                    let (epoch, str_repr) = Self::parse_when(w);
                    let max_id = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
                    self.entries.push(ReminderEntry {
                        id: max_id + 1,
                        target_epoch: epoch,
                        text: t.to_string(),
                        target_str: str_repr,
                        alerted: false,
                    });
                    self.task_text.clear();
                    self.when_text.clear();
                    self.save();
                }
            }
            ReminderMessage::DeleteReminder(id) => {
                self.entries.retain(|e| e.id != id);
                self.save();
            }
            ReminderMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, ReminderMessage> {
        let task_input = text_input("What to be reminded of...", &self.task_text)
            .on_input(ReminderMessage::TaskInputChanged)
            .padding(12)
            .size((theme.font_size_overlay) as f32);

        let when_input = text_input("When (e.g. 15m, 1h 30m, 5pm, tomorrow 9am)...", &self.when_text)
            .on_input(ReminderMessage::WhenInputChanged)
            .on_submit(ReminderMessage::AddReminder)
            .padding(12)
            .size((theme.font_size_overlay) as f32);

        let quick_row = row![
            text("Quick:").size(12).bold().color(theme.muted),
            button(text("+5m").size(11)).on_press(ReminderMessage::QuickWhen("5m".into())).style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("+15m").size(11)).on_press(ReminderMessage::QuickWhen("15m".into())).style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("+30m").size(11)).on_press(ReminderMessage::QuickWhen("30m".into())).style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("+1h").size(11)).on_press(ReminderMessage::QuickWhen("1h".into())).style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("+tomorrow 9am").size(11)).on_press(ReminderMessage::QuickWhen("tomorrow 9am".into())).style(|_t, _s| button::Style { background: None, ..Default::default() }),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut list_col = column![].spacing(8);
        for item in &self.entries {
            let id = item.id;
            let now = Local::now().timestamp();
            let remaining_s = (item.target_epoch - now).max(0);
            let rem_label = if remaining_s > 3600 {
                format!("{}h left", remaining_s / 3600)
            } else if remaining_s > 60 {
                format!("{}m left", remaining_s / 60)
            } else {
                format!("{}s left", remaining_s)
            };

            let row_item = row![
                text("⏰").size(16),
                column![
                    text(&item.text).size((theme.font_size_normal) as f32).bold().color(theme.text),
                    text(&item.target_str).size((theme.font_size_normal - 4) as f32).color(theme.muted),
                ]
                .width(Length::Fill),
                text(rem_label).size(12).color(theme.success).bold(),
                button(text("✕").size(12).color(theme.danger))
                    .on_press(ReminderMessage::DeleteReminder(id))
                    .padding(4)
                    .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            list_col = list_col.push(container(row_item).padding([6, 12]).width(Length::Fill));
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("⏰ SCHEDULED REMINDERS")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.text),
                task_input,
                when_input,
                quick_row,
                scrollable(list_col).height(Length::Fill),
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(780.0))
        .height(Length::Fixed(600.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(ReminderMessage::Dismiss)
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

pub fn save_reminders(entries: &[ReminderEntry]) {
    let path = RemindersOverlay::storage_path();
    if let Ok(data) = serde_json::to_string_pretty(entries) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, data).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}
