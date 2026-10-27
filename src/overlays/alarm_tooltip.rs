use iced::widget::{button, column, container, row, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use std::time::Instant;

use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::SlantSide;
use crate::overlays::tooltip_engine::{compute, clip_content, align_to_side, ParallelogramBg};

const AUTO_MS: u128 = 60_000;

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    AdjCountdownH(bool),
    AdjCountdownM(bool),
    AdjClockH(bool),
    AdjClockM(bool),
    ToggleAmPm,
    StartCountdown,
    StartClock,
    Dismiss,
}

pub struct App {
    opened: Instant,
    grow_right: bool,
    w: u32,
    h: u32,
    drop_ms: f32,
    expand_ms: f32,
    slant_l: SlantSide,
    closing_at: Option<Instant>,
    settings: ShellSettings,
    theme: ShellTheme,

    cd_h: u32,
    cd_m: u32,
    clk_h: u32,
    clk_m: u32,
    is_pm: bool,
}

impl App {
    fn new_app() -> (Self, Task<Message>) {
        let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|s| s.parse().ok()).unwrap_or(1920);
        let grow_right = std::env::var("MBS_GROW_RIGHT").map(|v| v == "1").unwrap_or_else(|_| cx < sw / 2);
        let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(420);
        let h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(480);
    eprintln!("[alarm_tooltip-h] MBS_TOOLTIP_H = {}", h);
    eprintln!("[child-h] alarm_tooltip MBS_TOOLTIP_H = {}", h);
        let drop_ms = std::env::var("MBS_TOOLTIP_DROP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(220.0);
        let expand_ms = std::env::var("MBS_TOOLTIP_EXPAND_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(260.0);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" { SlantSide::Right } else { SlantSide::Left };

        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);

        let app = Self {
            opened: Instant::now(),
            grow_right,
            w,
            h,
            drop_ms,
            expand_ms,
            slant_l,
            closing_at: None,
            settings,
            theme,
            cd_h: 0,
            cd_m: 5,
            clk_h: 12,
            clk_m: 0,
            is_pm: true,
        };
        
        (app, Task::none())
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                let elapsed_ms = self.opened.elapsed().as_millis();
                if let Some(t) = self.closing_at {
                    if t.elapsed().as_millis() > (self.drop_ms * 1.9) as u128 {
                        std::process::exit(0);
                    }
                } else if elapsed_ms > AUTO_MS {
                    std::process::exit(0);
                }

                let next_ms: u64 = if elapsed_ms < 2000 || self.closing_at.is_some() { 16 } else { 500 };
                return Task::perform(
                    async move { tokio::time::sleep(std::time::Duration::from_millis(next_ms)).await },
                    |_| Message::Tick,
                );
            }
            Message::AdjCountdownH(inc) => {
                if inc { self.cd_h = (self.cd_h + 1) % 24; }
                else { self.cd_h = (self.cd_h + 23) % 24; }
            }
            Message::AdjCountdownM(inc) => {
                if inc { self.cd_m = (self.cd_m + 1) % 60; }
                else { self.cd_m = (self.cd_m + 59) % 60; }
            }
            Message::AdjClockH(inc) => {
                if inc { self.clk_h = if self.clk_h >= 12 { 1 } else { self.clk_h + 1 }; }
                else { self.clk_h = if self.clk_h <= 1 { 12 } else { self.clk_h - 1 }; }
            }
            Message::AdjClockM(inc) => {
                if inc { self.clk_m = (self.clk_m + 1) % 60; }
                else { self.clk_m = (self.clk_m + 59) % 60; }
            }
            Message::ToggleAmPm => { self.is_pm = !self.is_pm; }
            Message::StartCountdown => {
                let mins = (self.cd_h as i64 * 60) + self.cd_m as i64;
                if mins > 0 {
                    let target = chrono::Local::now() + chrono::Duration::minutes(mins);
                    let mut entries = crate::overlays::reminders::RemindersOverlay::load();
                    let max_id = entries.iter().map(|e| e.id).max().unwrap_or(0);
                    entries.push(crate::overlays::reminders::ReminderEntry {
                        id: max_id + 1,
                        target_epoch: target.timestamp(),
                        text: format!("Timer {}h {:02}m", self.cd_h, self.cd_m),
                        target_str: target.format("%I:%M %p").to_string(),
                        alerted: false,
                    });
                    crate::overlays::reminders::save_reminders(&entries);
                }
                self.closing_at = Some(Instant::now());
            }
            Message::StartClock => {
                let mut h = self.clk_h % 12;
                if self.is_pm { h += 12; }
                let now = chrono::Local::now();
                let mut target = now.date_naive().and_hms_opt(h, self.clk_m, 0).unwrap().and_local_timezone(chrono::Local).unwrap();
                if target < now { target = target + chrono::Duration::days(1); }
                
                let mut entries = crate::overlays::reminders::RemindersOverlay::load();
                let max_id = entries.iter().map(|e| e.id).max().unwrap_or(0);
                entries.push(crate::overlays::reminders::ReminderEntry {
                    id: max_id + 1,
                    target_epoch: target.timestamp(),
                    text: format!("Alarm {:02}:{:02} {}", self.clk_h, self.clk_m, if self.is_pm {"PM"} else {"AM"}),
                    target_str: target.format("%I:%M %p").to_string(),
                    alerted: false,
                });
                crate::overlays::reminders::save_reminders(&entries);
                self.closing_at = Some(Instant::now());
            }
            Message::Dismiss => {
                if self.closing_at.is_none() {
                    self.closing_at = Some(Instant::now());
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let time_ms = if let Some(t) = self.closing_at {
            (self.opened.elapsed().as_millis() as f32 - t.elapsed().as_millis() as f32).max(0.0)
        } else {
            self.opened.elapsed().as_millis() as f32
        };

        let dynamic_h = 340.0 + self.settings.bar_height as f32;
        let content_h: f32 = 340.0 + self.settings.bar_height as f32;
        let effective_h = self.h as f32;
        let a = compute(time_ms, 150.0, self.w as f32, effective_h - self.settings.bar_height as f32, self.theme.slant_width, self.settings.bar_height as f32, self.drop_ms, self.expand_ms);
        let slant = (self.theme.slant_width * (a.h / self.settings.bar_height.max(1) as f32)).max(0.0);

        let bg = ParallelogramBg {
            w: a.w,
            h: a.h,
            slant,
            slant_right: self.slant_l == SlantSide::Right,
            background: self.theme.background,
            border_color: self.theme.text,
            border_width: self.theme.control_border_width,
            frame: self.opened.elapsed().as_millis() as u64,
        };


        let spinner = |val: u32, msg_up: Message, msg_down: Message| {
            column![
                button(text("▲").size(12).color(self.theme.text)).on_press(msg_up).padding(2).style(|_t, _s| button::Style { background: Some(iced::Background::Color(Color::TRANSPARENT)), border: iced::Border::default(), ..Default::default() }),
                text(format!("{:02}", val)).size(24).font(iced::Font::MONOSPACE).color(self.theme.text),
                button(text("▼").size(12).color(self.theme.text)).on_press(msg_down).padding(2).style(|_t, _s| button::Style { background: Some(iced::Background::Color(Color::TRANSPARENT)), border: iced::Border::default(), ..Default::default() }),
            ].spacing(2).align_x(Alignment::Center)
        };

        let cd_row_el = row![
            spinner(self.cd_h, Message::AdjCountdownH(true), Message::AdjCountdownH(false)),
            text("h").size(16).color(self.theme.muted),
            spinner(self.cd_m, Message::AdjCountdownM(true), Message::AdjCountdownM(false)),
            text("m").size(16).color(self.theme.muted),
            text("").width(8),
            button(text("Start").size(16).color(self.theme.background)).on_press(Message::StartCountdown).padding([6, 12]).style(|_t, _s| button::Style { background: Some(iced::Background::Color(self.theme.success)), border: iced::Border::default(), ..Default::default() })
        ].spacing(6).align_y(Alignment::Center);

        let clk_row_el = row![
            spinner(self.clk_h, Message::AdjClockH(true), Message::AdjClockH(false)),
            text(":").size(20).color(self.theme.text),
            spinner(self.clk_m, Message::AdjClockM(true), Message::AdjClockM(false)),
            text("").width(8),
            button(text(if self.is_pm { "PM" } else { "AM" }).size(14).color(self.theme.text))
                .on_press(Message::ToggleAmPm).padding([4, 8]).style(|_t, _s| button::Style { background: Some(iced::Background::Color(self.theme.surface_alt)), border: iced::Border::default(), ..Default::default() }),
            text("").width(8),
            button(text("Set").size(16).color(self.theme.background)).on_press(Message::StartClock).padding([6, 12]).style(|_t, _s| button::Style { background: Some(iced::Background::Color(self.theme.success)), border: iced::Border::default(), ..Default::default() })
        ].spacing(6).align_y(Alignment::Center);

        // Compute FINAL dimensions once.
        let final_h = effective_h - self.settings.bar_height as f32;
        // Extra horizontal padding so spinner numbers don't clip against the left diagonal.
        let safe: f32 = 40.0;
        let final_slant = (self.theme.slant_width * (final_h / self.settings.bar_height.max(1) as f32)).max(0.0);
        let final_w = self.w as f32;
        let slant_r = self.slant_l == SlantSide::Right;

        // Dynamically track the Y coordinate so it perfectly matches the layout.
        let mut y = 20.0;
        let spacer = 6.0;

        let title_row = crate::overlays::weather_tooltip::slanted_row(
            text("SET ALARM").size(20).color(self.theme.warning).font(crate::style::shell_font(iced::font::Weight::Bold)).into(),
            y + 20.0, final_h, final_slant, slant_r, safe, final_w, 40.0,
        );
        y += 40.0 + spacer;

        let cd_label = crate::overlays::weather_tooltip::slanted_row(
            text("Countdown").size(14).color(self.theme.muted).into(),
            y + 15.0, final_h, final_slant, slant_r, safe, final_w, 30.0,
        );
        y += 30.0 + spacer;

        let cd_wrapped = crate::overlays::weather_tooltip::slanted_row(
            cd_row_el.into(),
            y + 40.0, final_h, final_slant, slant_r, safe, final_w, 80.0,
        );
        y += 80.0 + spacer;

        let clk_label = crate::overlays::weather_tooltip::slanted_row(
            text("Clock Time").size(14).color(self.theme.muted).into(),
            y + 15.0, final_h, final_slant, slant_r, safe, final_w, 30.0,
        );
        y += 30.0 + spacer;

        let clk_wrapped = crate::overlays::weather_tooltip::slanted_row(
            clk_row_el.into(),
            y + 40.0, final_h, final_slant, slant_r, safe, final_w, 80.0,
        );

        let content = iced::widget::container(
            column![
                title_row,
                cd_label,
                cd_wrapped,
                clk_label,
                clk_wrapped,
            ].spacing(6)
        ).width(Length::Fill).height(Length::Fill).center_y(Length::Fill);

        let stacked = iced::widget::container(Stack::new().push(bg.view()).push(clip_content(content.into(), a.w, a.h, self.grow_right)))
            .width(Length::Fixed(a.w))
            .height(Length::Fixed(a.h))
            .clip(true);
        align_to_side(stacked.into(), self.grow_right)
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen_with(|event, _s, _w| {
            match event {
                iced::Event::Window(iced::window::Event::Opened { .. }) => Some(Message::Tick),
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape), .. }) => Some(Message::Dismiss),
                _ => None,
            }
        })
    }
    fn theme(&self) -> Theme { Theme::Dark }
}

impl TryInto<iced_layershell::actions::LayershellCustomActionWithId> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionWithId, Self::Error> {
        Err(self)
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(420);
    let h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(480);
    eprintln!("[alarm_tooltip-h] MBS_TOOLTIP_H = {}", h);
    eprintln!("[child-h] alarm_tooltip MBS_TOOLTIP_H = {}", h);

    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(100.0);
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);
    let half_cap = (cap_w / 2.0) as i32;

    let adjust_left = s.tooltip_offset_left;
    let adjust_right = s.tooltip_offset_right;

    let (anchor, margin) = if grow_right {
        (Anchor::Top | Anchor::Left, (margin_top, 0, 0, (cx - half_cap + adjust_left).max(0)))
    } else {
        (Anchor::Top | Anchor::Right, (margin_top, (sw - cx - half_cap + adjust_right).max(0), 0, 0))
    };

    let layer = LayerShellSettings {
        size: Some((w, h)), exclusive_zone: -1, anchor, layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        start_mode: output.map(iced_layershell::settings::StartMode::TargetScreen).unwrap_or(iced_layershell::settings::StartMode::Active), margin, ..Default::default()
    };
    
    iced_layershell::application(
        App::new_app,
        || "mbs-alarm-tip".to_string(),
        App::update,
        App::view,
    )
    .subscription(App::subscription)
    .style(|_state, _theme| iced::theme::Style {
        background_color: iced::Color::TRANSPARENT,
        text_color: iced::Color::WHITE,
    })
    .settings(Settings { layer_settings: layer, ..Default::default() })
    .run()
}
