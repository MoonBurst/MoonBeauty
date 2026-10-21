use iced::widget::{button, column, container, horizontal_space, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::overlays::reminders::{save_reminders, ReminderEntry, RemindersOverlay};
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

const H: u32 = 290;
const W_START: f32 = 130.0;
const AUTO_MS: u128 = 60_000;

#[derive(Debug, Clone)]
pub enum Message {
    Tick, TaskChanged(String), WhenChanged(String), QuickWhen(String),
    AddTimer, DeleteTimer(u64), Dismiss,
}

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error> {
        Err(self)
    }
}

pub struct App {
    opened: Instant,
    grow_right: bool,
    w: u32,
    w_start: f32,
    slant_l: SlantSide,
    slant_r: SlantSide,
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
        let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|s| s.parse().ok()).unwrap_or(1920);
        let grow_right = std::env::var("MBS_GROW_RIGHT").map(|v| v == "1").unwrap_or_else(|_| cx < sw / 2);
        let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
        let w_start: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let slant_r = if sr_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let entries = RemindersOverlay::load();

        let app = Self {
            opened: Instant::now(), grow_right, w, w_start, slant_l, slant_r,
            task_text: String::new(), when_text: String::new(), entries,
        };
        (app, Task::none())
    }

    fn namespace(&self) -> String { "mbs-alarm-tip".into() }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE }
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
                        id: max_id + 1, target_epoch: epoch, text: label, target_str: str_repr, alerted: false,
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
        let anim = crate::overlays::tooltip_anim::compute(self.opened.elapsed().as_millis() as f32, self.w_start, self.w as f32, H as f32);
        let slant = (theme.slant_width * (anim.h / settings.bar_height as f32)).max(0.0);
        let now = chrono::Local::now().timestamp();
        let has_active = self.entries.iter().any(|e| e.target_epoch > now);
        let border_color = if has_active { theme.warning } else { theme.text };

        let bg_shape = ShapeBox::new()
            .shape(ShapeType::Slant { left: self.slant_l, right: self.slant_r })
            .background(theme.background)
            .border(border_color, theme.control_border_width)
            .slant_width(slant)
            .size(Length::Fixed(self.w as f32), Length::Fixed(anim.h));

        let content: Element<'_, Message> = self.build_content(&theme);
        let clipped = crate::overlays::tooltip_anim::clip_content(content, anim.w, anim.h, self.grow_right);
        let stacked: Element<'_, Message> = Stack::new().push(bg_shape.view()).push(clipped).into();
        crate::overlays::tooltip_anim::align_to_side(stacked, self.grow_right)
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick),
            iced::keyboard::on_key_press(|k, _| if k == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) { Some(Message::Dismiss) } else { None }),
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
        let pad_x = (t.slant_width * 2.0 + 12.0).max(28.0);
        let pad = Padding { top: 16.0, right: pad_x, bottom: 16.0, left: pad_x };

        let close_btn: Element<'a, Message> = button(text("✕").size(11).color(muted))
            .on_press(Message::Dismiss)
            .into();

        let header: Element<'a, Message> = row![
            text("⏰ TIMERS & ALARMS").size(13).color(success),
            horizontal_space(),
            close_btn,
        ].align_y(Alignment::Center).into();

        let quick_btn = |lbl: &'static str, val: &'static str| -> Element<'a, Message> {
            button(text(lbl).size(11).color(fg))
                .on_press(Message::QuickWhen(val.into()))
                .padding([2, 6])
                .into()
        };

        let quick_row: Element<'a, Message> = row![
            text("Quick:").size(11).color(muted),
            quick_btn("+1m", "1m"), quick_btn("+5m", "5m"),
            quick_btn("+15m", "15m"), quick_btn("+30m", "30m"), quick_btn("+1h", "1h"),
        ].spacing(6).align_y(Alignment::Center).into();

        let task_input: Element<'a, Message> = text_input("Note (e.g. Tea)", &self.task_text)
            .on_input(Message::TaskChanged)
            .padding([4, 8])
            .size(12)
            .width(Length::FillPortion(1))
            .into();
        let when_input: Element<'a, Message> = text_input("When (5m, 5pm)", &self.when_text)
            .on_input(Message::WhenChanged)
            .on_submit(Message::AddTimer)
            .padding([4, 8])
            .size(12)
            .width(Length::FillPortion(1))
            .into();
        let set_btn: Element<'a, Message> = button(text("Set").size(12).color(Color::BLACK))
            .on_press(Message::AddTimer)
            .padding([4, 10])
            .into();
        let input_row: Element<'a, Message> = row![task_input, when_input, set_btn]
            .spacing(6).align_y(Alignment::Center).into();

        let now = chrono::Local::now().timestamp();
        let mut timers_col: iced::widget::Column<'a, Message> = iced::widget::Column::new().spacing(4).width(Length::Fill);

        for e in self.entries.iter().filter(|e| e.target_epoch > now) {
            let id = e.id;
            let diff = (e.target_epoch - now).max(0);
            let rem_label = if diff >= 3600 { format!("{}h {}m left", diff / 3600, (diff % 3600) / 60) }
                            else if diff >= 60 { format!("{}m {}s left", diff / 60, diff % 60) }
                            else { format!("{}s left", diff) };
            let del_btn: Element<'a, Message> = button(text("✕").size(11).color(danger))
                .on_press(Message::DeleteTimer(id))
                .padding([1, 4])
                .into();
            timers_col = timers_col.push(row![
                text(&e.text).size(12).color(fg).width(Length::Fill),
                text(rem_label).size(11).color(warning),
                del_btn,
            ].spacing(8).align_y(Alignment::Center));
        }

        let scroll_list: Element<'a, Message> = container(scrollable(timers_col).height(Length::Fill))
            .height(Length::Fixed(120.0))
            .width(Length::Fill)
            .into();

        column![header, quick_row, input_row, scroll_list]
            .spacing(10)
            .padding(pad)
            .width(Length::Fixed(self.w as f32))
            .height(Length::Fixed(H as f32))
            .into()
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    eprintln!("[tip-output] raw={:?} parsed={:?}", std::env::var("MBS_OUTPUT"), output);
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let grow_right = cx < sw / 2;
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
    let half_start = (cap_w / 2.0) as i32;
    let bar_h = s.bar_height as i32;
    let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
    let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
    let sw_cap = s.slant_width;
    let sw_tip = s.slant_width * (H as f32 / s.bar_height as f32);
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);

    let (anchor, margin) = if grow_right {
        let sl_left = sl_str == "L";
        let shift = if sl_left { sw_cap } else { 0.0 };
        (Anchor::Top | Anchor::Left, (margin_top, 0, 0, ((cx - half_start) as f32 + shift).max(0.0).min((sw - w as i32).max(0) as f32) as i32))
    } else {
        let sr_right = sr_str == "R";
        let shift = if sr_right { sw_cap } else { 0.0 };
        (Anchor::Top | Anchor::Right, (margin_top, ((sw - cx - half_start) as f32 + shift).max(0.0).min((sw - w as i32).max(0) as f32) as i32, 0, 0))
    };
    let layer = LayerShellSettings { size: Some((w, H)), exclusive_zone: -1, anchor, layer: Layer::Overlay, keyboard_interactivity: KeyboardInteractivity::OnDemand, binded_output_name: output, margin, ..Default::default() };
    App::run(Settings { layer_settings: layer, ..Default::default() })
}
