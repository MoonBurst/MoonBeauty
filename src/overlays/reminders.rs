use chrono::{Duration, Local};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Element, Length, Padding};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct ReminderEntry {
    pub id: u64,
    pub target_epoch: i64,
    pub text: String,
    pub target_str: String,
    pub alerted: bool,
    #[serde(default = "default_true")]
    pub play_sound: bool,
}

fn default_true() -> bool {
    true
}

pub const REMINDER_TASK_ID: &str = "rem_task";
pub const REMINDER_WHEN_ID: &str = "rem_when";

#[derive(Debug, Clone)]
pub enum ReminderMessage {
    TaskInputChanged(String),
    WhenInputChanged(String),
    QuickWhen(String),
    ScrollUp,
    ScrollDown,
    SelectFirst,
    SelectLast,
    FocusNextField,
    FocusPrevField,
    DeleteSelected,
    Tick,
    InitialFocus,
    Refocus,
    AddReminder,
    DeleteReminder(u64),
    ToggleSound(u64),
    ToggleSoundSelected,
    Dismiss,
}

pub struct RemindersOverlay {
    pub task_text: String,
    pub when_text: String,
    pub entries: Vec<ReminderEntry>,
    pub scroll_y: f32,
    pub active_field: usize, // 0 = task, 1 = when
    pub selected_idx: Option<usize>,
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
            scroll_y: 0.0,
            active_field: 0,
            selected_idx: None,
        }
    }

    pub fn empty() -> Self {
        Self { task_text: String::new(), when_text: String::new(), entries: Vec::new(), scroll_y: 0.0, active_field: 0, selected_idx: None }
    }

    pub fn storage_path() -> PathBuf {
        let base = crate::libraries::process::mbs_runtime_dir();
        let mbs = base.join("moon_reminders.json");
        let qs = base.join("quickshell_reminders.json");
        // One-time migration from Quickshell. The QML backend wrote to
        // XDG_RUNTIME_DIR/quickshell_reminders.json; if that exists but
        // our file doesn't, copy it over so users don't lose their timers.
        if !mbs.exists() && qs.exists() {
            match fs::copy(&qs, &mbs) {
                Ok(n) => eprintln!("[reminders] migrated {} bytes from quickshell", n),
                Err(e) => eprintln!("[reminders] migration failed: {e}"),
            }
        }
        mbs
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

        if !expr.contains("am") && !expr.contains("pm") {
            let compact = expr.replace(' ', "");

            if let Some(h_pos) = compact.find('h') {
                if let Ok(h) = compact[..h_pos].parse::<i64>() {
                    minutes += h * 60;
                    is_relative = true;
                }
            }

            let start = if let Some(h_pos) = compact.find('h') {
                h_pos + 1
            } else {
                0
            };
            if let Some(m_pos) = compact[start..].find('m') {
                let abs_m_pos = start + m_pos;
                if let Ok(m) = compact[start..abs_m_pos].parse::<i64>() {
                    minutes += m;
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

        if let Some(time_part) = expr
            .split_whitespace()
            .find(|s| s.contains(':') || s.ends_with("am") || s.ends_with("pm"))
        {
            let mut hour = 9;
            let mut min = 0;

            if let Some((h_str, m_str)) = time_part.split_once(':') {
                let is_pm = m_str.ends_with("pm");
                let is_am = m_str.ends_with("am");
                let clean_m = m_str.trim_end_matches("am").trim_end_matches("pm");
                hour = h_str.parse().unwrap_or(9);
                min = clean_m.parse().unwrap_or(0);
                if is_pm && hour < 12 {
                    hour += 12;
                }
                if is_am && hour == 12 {
                    hour = 0;
                }
            } else if time_part.ends_with("pm") {
                hour = time_part.trim_end_matches("pm").parse().unwrap_or(9);
                if hour < 12 {
                    hour += 12;
                }
            } else if time_part.ends_with("am") {
                hour = time_part.trim_end_matches("am").parse().unwrap_or(9);
                if hour == 12 {
                    hour = 0;
                }
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
        (
            fallback.timestamp(),
            fallback.format("%a, %b %d %I:%M %p").to_string(),
        )
    }

    pub fn cycle_focus_forward(&mut self) {
        self.active_field = (self.active_field + 1) % 2;
    }

    pub fn cycle_focus_backward(&mut self) {
        self.active_field = if self.active_field == 0 { 1 } else { 0 };
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
                if t.is_empty() && w.is_empty() {
                    // Both inputs empty → Enter means "toggle sound on the
                    // selected reminder", matching the footer hint.
                    self.update(ReminderMessage::ToggleSoundSelected);
                    return;
                }
                if !t.is_empty() && !w.is_empty() {
                    let (epoch, str_repr) = Self::parse_when(w);
                    let max_id = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
                    self.entries.push(ReminderEntry {
                        id: max_id + 1,
                        target_epoch: epoch,
                        text: t.to_string(),
                        target_str: str_repr,
                        alerted: false,
                        play_sound: true,
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
            ReminderMessage::ToggleSound(id) => {
                if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                    e.play_sound = !e.play_sound;
                    self.save();
                }
            }
            ReminderMessage::ToggleSoundSelected => {
                // Guard: only when the input fields are empty, so typing
                // 's' or pressing Enter while composing doesn't toggle the
                // selected reminder's sound.
                if !self.task_text.is_empty() || !self.when_text.is_empty() {
                    return;
                }
                if let Some(idx) = self.selected_idx {
                    if idx < self.entries.len() {
                        let id = self.entries[idx].id;
                        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                            e.play_sound = !e.play_sound;
                            self.save();
                        }
                    }
                }
            }
            ReminderMessage::FocusNextField => {
                self.cycle_focus_forward();
            }
            ReminderMessage::FocusPrevField => {
                self.cycle_focus_backward();
            }
            ReminderMessage::InitialFocus => {}
            ReminderMessage::Tick => {}
            ReminderMessage::ScrollDown => {
                // Only navigate the list if the input fields are empty.
                // Otherwise arrows should go to text_input for cursor work.
                if !self.task_text.is_empty() || !self.when_text.is_empty() {
                    return;
                }
                if self.entries.is_empty() {
                    self.selected_idx = None;
                    return;
                }
                self.selected_idx = Some(match self.selected_idx {
                    None => 0,
                    Some(i) => (i + 1) % self.entries.len(),
                });
                // Approximate row step: row height (~60) + gap (8)
                let idx = self.selected_idx.unwrap_or(0) as f32;
                self.scroll_y = (idx * 68.0 - 40.0).max(0.0);
            }
            ReminderMessage::SelectFirst => {
                if !self.task_text.is_empty() || !self.when_text.is_empty() {
                    return;
                }
                if self.entries.is_empty() {
                    self.selected_idx = None;
                    return;
                }
                self.selected_idx = Some(0);
                self.scroll_y = 0.0;
            }
            ReminderMessage::SelectLast => {
                if !self.task_text.is_empty() || !self.when_text.is_empty() {
                    return;
                }
                if self.entries.is_empty() {
                    self.selected_idx = None;
                    return;
                }
                self.selected_idx = Some(self.entries.len() - 1);
                self.scroll_y = ((self.entries.len() - 1) as f32 * 68.0 - 40.0).max(0.0);
            }
            ReminderMessage::ScrollUp => {
                if !self.task_text.is_empty() || !self.when_text.is_empty() {
                    return;
                }
                if self.entries.is_empty() {
                    self.selected_idx = None;
                    return;
                }
                self.selected_idx = Some(match self.selected_idx {
                    None => 0,
                    Some(0) => self.entries.len() - 1,
                    Some(i) => i - 1,
                });
                let idx = self.selected_idx.unwrap_or(0) as f32;
                self.scroll_y = (idx * 68.0 - 40.0).max(0.0);
            }
            ReminderMessage::DeleteSelected => {
                eprintln!(
                    "[reminders-delete] fired: task_empty={} when_empty={} sel={:?} count={}",
                    self.task_text.is_empty(),
                    self.when_text.is_empty(),
                    self.selected_idx,
                    self.entries.len()
                );
                if !self.task_text.is_empty() || !self.when_text.is_empty() {
                    return;
                }
                if let Some(idx) = self.selected_idx {
                    if idx < self.entries.len() {
                        let id = self.entries[idx].id;
                        self.entries.retain(|e| e.id != id);
                        self.save();
                        if self.entries.is_empty() {
                            self.selected_idx = None;
                        } else {
                            self.selected_idx = Some(idx.min(self.entries.len() - 1));
                        }
                    }
                }
            }
            ReminderMessage::Refocus => {}
            ReminderMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, ReminderMessage> {
        let (win_w, win_h) = settings.get_window_size("reminders", 780, 600);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let footer_inset = if dy > pad_bottom {
            (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let body_inset = header_inset.min(footer_inset) + 4.0;

        let fs = theme.font_size_overlay as f32;
        let fs_sm = fs * 0.85;

        let task_input = text_input("What to be reminded of...", &self.task_text)
            .id(REMINDER_TASK_ID)
            .on_input(ReminderMessage::TaskInputChanged)
            .on_submit(ReminderMessage::Refocus) // Jumps to When field on Enter
            .padding(14)
            .size(fs);

        let when_input = text_input(
            "When (e.g. 15m, 1h 30m, 5pm, tomorrow 9am)...",
            &self.when_text,
        )
        .id(REMINDER_WHEN_ID)
        .on_input(ReminderMessage::WhenInputChanged)
        .on_submit(ReminderMessage::AddReminder)
        .padding(14)
        .size(fs);

        let sched_btn = button(
            container(text("+ Schedule").size(fs_sm).bold().color(theme.success)).padding([14, 20]),
        )
        .on_press(ReminderMessage::AddReminder)
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(theme.surface_alt)),
            border: iced::Border {
                color: theme.success,
                width: 1.5,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        let mut list_col = column![].spacing(8);
        for (idx, item) in self.entries.iter().enumerate() {
            let is_sel = self.selected_idx == Some(idx);
            let id = item.id;
            let now = Local::now().timestamp();
            let remaining_s = (item.target_epoch - now).max(0);
            // Match Quickshell's formatRemaining exactly:
            //   due now / Xd Yh / Xh Ym / Xm Ys / Xs
            let rem_label = if remaining_s <= 0 {
                "Due now".to_string()
            } else {
                let d = remaining_s / 86400;
                let h = (remaining_s % 86400) / 3600;
                let m = (remaining_s % 3600) / 60;
                let sec = remaining_s % 60;
                if d > 0 {
                    format!("{}d {}h", d, h)
                } else if h > 0 {
                    format!("{}h {}m", h, m)
                } else if m > 0 {
                    format!("{}m {}s", m, sec)
                } else {
                    format!("{}s", sec)
                }
            };

            let row_item = row![
                text("⏰").size(fs),
                column![
                    text(&item.text).size(fs).bold().color(theme.text),
                    text(&item.target_str).size(fs_sm).color(theme.muted),
                ]
                .width(Length::Fill),
                text(rem_label).size(fs_sm).color(theme.success).bold(),
                button(
                    text(if item.play_sound { "🔔" } else { "🔕" })
                        .size(fs_sm)
                        .color(if item.play_sound {
                            theme.success
                        } else {
                            theme.muted
                        })
                )
                .on_press(ReminderMessage::ToggleSound(id))
                .padding(4)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
                button(text("✕").size(fs_sm).color(theme.danger))
                    .on_press(ReminderMessage::DeleteReminder(id))
                    .padding(4)
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    }),
            ]
            .align_y(Alignment::Center);

            let row_bg = if is_sel {
                theme.background
            } else {
                theme.surface_alt
            };
            let row_border = if is_sel { theme.success } else { theme.border };
            let row_border_w = if is_sel {
                theme.control_border_width * 1.5
            } else {
                1.0
            };

            list_col = list_col.push(
                container(row_item)
                    .padding([8, 12])
                    .width(Length::Fill)
                    .style(move |_t| container::Style {
                        background: Some(iced::Background::Color(row_bg)),
                        border: iced::Border {
                            color: row_border,
                            width: row_border_w,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }

        let header_row = container(
            text("⏰ SCHEDULED CRITICAL REMINDERS")
                .size(fs)
                .bold()
                .color(theme.text),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let when_row = row![when_input, sched_btn].align_y(iced::alignment::Vertical::Center);

        let input_area = container(column![task_input, when_row])
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: body_inset,
                bottom: 0.0,
                left: body_inset,
            });

        let list_container = container(
            scrollable(list_col)
                .id(iced::widget::Id::new("reminders_list"))
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: body_inset,
            bottom: 0.0,
            left: body_inset,
        });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let footer = container(
            text("Tab: Next field  •  ↑↓/Home/End: Select  •  Enter/Ctrl+S: Toggle sound  •  Del: Delete  •  Esc: Clear/Dismiss")
                .size(fs_sm * 0.9)
                .color(theme.muted)
                .align_x(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding { top: 8.0, right: header_inset, bottom: 0.0, left: header_inset });

        let card_content = container(
            column![header_row, input_area, list_container, footer].padding(Padding {
                top: pad_top,
                right: 0.0,
                bottom: pad_bottom,
                left: 0.0,
            }),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h))
        .style(|_t| iced::widget::container::Style::default());

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_t| iced::widget::container::Style::default())
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
