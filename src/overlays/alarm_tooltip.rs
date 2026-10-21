use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::overlays::reminders::{save_reminders, ReminderEntry, RemindersOverlay};
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

const W: u32 = 380;
const H: u32 = 290;
const W_START: f32 = 130.0;
const AUTO_MS: u128 = 60_000;

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    TaskChanged(String),
    WhenChanged(String),
    QuickWhen(String),
    AddTimer,
    DeleteTimer(u64),
    Dismiss,
}

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error> {
        Err(self)
    }
}

pub struct App {
    opened: Instant,
    slant_mode: String,
    task_text: String,
    when_text: String,
    entries: Vec<ReminderEntry>,
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_: ()) -> (Self, Task<Message>) {
        let slant_mode = std::env::var("MBS_WX_SLANT").unwrap_or_else(|_| "left".into());
        let entries = RemindersOverlay::load();

        let app = Self {
            opened: Instant::now(),
            slant_mode,
            task_text: String::new(),
            when_text: String::new(),
            entries,
        };
        (app, Task::none())
    }

    fn namespace(&self) -> String { "mbs-alarm-tip".into() }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance {
            background_color: Color::TRANSPARENT,
            text_color: Color::WHITE,
        }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                if self.opened.elapsed().as_millis() > AUTO_MS { std::process::exit(0); }
                self.entries = RemindersOverlay::load();
            }
            Message::TaskChanged(t) => self.task_text = t,
            Message::WhenChanged(w) => self.when_text = w,
            Message::QuickWhen(w) => {
                self.when_text = w;
                return self.update(Message::AddTimer);
            }
            Message::AddTimer => {
                let w = self.when_text.trim();
                if !w.is_empty() {
                    let label = if self.task_text.trim().is_empty() { "Timer".to_string() } else { self.task_text.trim().to_string() };
                    let (epoch, str_repr) = RemindersOverlay::parse_when(w);
                    let max_id = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
                    self.entries.push(ReminderEntry {
                        id: max_id + 1,
                        target_epoch: epoch,
                        text: label,
                        target_str: str_repr,
                        alerted: false,
                    });
                    save_reminders(&self.entries);
                    self.task_text.clear();
                    self.when_text.clear();
                }
            }
            Message::DeleteTimer(id) => {
                self.entries.retain(|e| e.id != id);
                save_reminders(&self.entries);
            }
            Message::Dismiss => std::process::exit(0),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);

        let anim_w = W as f32;
        let anim_h = H as f32;

        let slant = (theme.slant_width * (anim_h / settings.bar_height as f32)).max(0.0);
        let (sl, sr) = match self.slant_mode.as_str() {
            "right" => (SlantSide::Right, SlantSide::Right),
            "center" => (SlantSide::Left, SlantSide::Right),
            _ => (SlantSide::Left, SlantSide::Left),
        };

        // Match active timer warning border
        let now = chrono::Local::now().timestamp();
        let has_active = self.entries.iter().any(|e| e.target_epoch > now);
        let border_color = if has_active { theme.warning } else { theme.text };

        let bg_shape = ShapeBox::new()
            .shape(ShapeType::Slant { left: sl, right: sr })
            .background(theme.background)
            .border(border_color, theme.control_border_width)
            .slant_width(slant)
            .size(Length::Fixed(anim_w), Length::Fixed(anim_h));

        let content = self.build_content(&theme);

        Stack::new()
            .push(bg_shape.view())
            .push(content)
            .into()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([
            iced::time::every(std::time::Duration::from_millis(500)).map(|_| Message::Tick),
            iced::keyboard::on_key_press(|k, _| {
                if k == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    Some(Message::Dismiss)
                } else { None }
            }),
        ])
    }

    fn theme(&self) -> Theme { Theme::Dark }
}

impl App {
    fn build_content<'a>(&'a self, t: &ShellTheme) -> Element<'a, Message> {
        let fg = t.text;
        let muted = t.muted;
        let success = t.success;
        let warning = t.warning;
        let danger = t.danger;
        let surface_alt = t.surface_alt;
        let border = t.border;
        let bg = t.background;

        let pad_x = (t.slant_width * 2.0 + 12.0).max(28.0);
        let pad = Padding { top: 16.0, right: pad_x, bottom: 16.0, left: pad_x };

        let header = row![
            text("⏰ TIMERS & ALARMS").size(13).color(success),
            row![].width(Length::Fill),
            button(text("✕").size(11).color(muted)).on_press(Message::Dismiss).style(|_t, _s| button::Style { background: None, ..Default::default() })
        ].align_y(Alignment::Center);

        let quick_btn = move |lbl: &'static str, val: &'static str| {
            button(text(lbl).size(11).color(fg))
                .on_press(Message::QuickWhen(val.into()))
                .padding([2, 6])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(surface_alt)),
                    border: iced::Border { color: border, width: 1.0, radius: 3.0.into() },
                    ..Default::default()
                })
        };

        let quick_row = row![
            text("Quick:").size(11).color(muted),
            quick_btn("+1m", "1m"),
            quick_btn("+5m", "5m"),
            quick_btn("+15m", "15m"),
            quick_btn("+30m", "30m"),
            quick_btn("+1h", "1h"),
        ].spacing(6).align_y(Alignment::Center);

        let input_row = row![
            text_input("Note (e.g. Tea)", &self.task_text).on_input(Message::TaskChanged).padding([4, 8]).size(12).width(Length::FillPortion(1)),
            text_input("When (5m, 5pm)", &self.when_text).on_input(Message::WhenChanged).on_submit(Message::AddTimer).padding([4, 8]).size(12).width(Length::FillPortion(1)),
            button(text("Set").size(12).color(bg)).on_press(Message::AddTimer).padding([4, 10]).style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(fg)),
                border: iced::Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            }),
        ].spacing(6).align_y(Alignment::Center);

        let now = chrono::Local::now().timestamp();
        let mut timers_col = column![].spacing(4).width(Length::Fill);

        for e in self.entries.iter().filter(|e| e.target_epoch > now) {
            let id = e.id;
            let diff = (e.target_epoch - now).max(0);
            let rem_label = if diff >= 3600 {
                format!("{}h {}m left", diff / 3600, (diff % 3600) / 60)
            } else if diff >= 60 {
                format!("{}m {}s left", diff / 60, diff % 60)
            } else {
                format!("{}s left", diff)
            };

            let item_row = row![
                text(&e.text).size(12).color(fg).width(Length::Fill),
                text(rem_label).size(11).color(warning),
                button(text("✕").size(11).color(danger)).on_press(Message::DeleteTimer(id)).padding([1, 4]).style(|_t, _s| button::Style { background: None, ..Default::default() })
            ].spacing(8).align_y(Alignment::Center);

            timers_col = timers_col.push(item_row);
        }

        let scroll_list = container(scrollable(timers_col).height(Length::Fill)).height(Length::Fixed(120.0)).width(Length::Fill);

        column![
            header,
            quick_row,
            input_row,
            scroll_list,
        ].spacing(10).padding(pad).width(Length::Fixed(W as f32)).height(Length::Fixed(H as f32)).into()
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    eprintln!("[alarm-tooltip-run] Starting alarm tooltip process...");
    let s = ShellSettings::load();
    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let half_start = (W_START / 2.0) as i32;

    let (anchor, margin) = if grow_right {
        (Anchor::Top | Anchor::Left, (bar_h, 0, 0, (cx - half_start).max(0)))
    } else {
        (Anchor::Top | Anchor::Right, (bar_h, (sw - cx - half_start).max(0), 0, 0))
    };

    let layer = LayerShellSettings {
        size: Some((W, H)),
        exclusive_zone: -1,
        anchor,
        layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        margin,
        ..Default::default()
    };
    App::run(Settings { layer_settings: layer, ..Default::default() })
}
