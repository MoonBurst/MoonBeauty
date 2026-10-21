use iced::widget::{button, column, container, horizontal_space, row, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::backend::weather::{WeatherAlert, WeatherData};
use crate::backend::WeatherBackend;
use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

const H: u32 = 220;
const W_START: f32 = 130.0;
const AUTO_MS: u128 = 10_000;

#[derive(Debug, Clone)]
pub enum Message { Tick, Loaded(Option<WeatherData>), Dismiss }

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error> {
        Err(self)
    }
}

pub struct App {
    opened: Instant,
    weather: Option<WeatherData>,
    fetched: bool,
    grow_right: bool,
    w: u32,
    w_start: f32,
    slant_l: SlantSide,
    slant_r: SlantSide,
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
        let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(340);
        let w_start: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let slant_r = if sr_str == "R" { SlantSide::Right } else { SlantSide::Left };

        let app = Self { opened: Instant::now(), weather: None, fetched: false, grow_right, w, w_start, slant_l, slant_r };
        let task = Task::perform(async { WeatherBackend::fetch().await.ok() }, Message::Loaded);
        (app, task)
    }

    fn namespace(&self) -> String { "mbs-wx-tip".into() }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => { if self.opened.elapsed().as_millis() > AUTO_MS { std::process::exit(0); } }
            Message::Loaded(w) => { self.weather = w; self.fetched = true; }
            Message::Dismiss => std::process::exit(0),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let anim = crate::overlays::tooltip_anim::compute(self.opened.elapsed().as_millis() as f32, self.w_start, self.w as f32, H as f32);
        let slant = (theme.slant_width * (anim.h / settings.bar_height as f32)).max(0.0);

        let border_color = match &self.weather {
            Some(w) => match &w.alert {
                WeatherAlert::Danger(_) => theme.danger,
                WeatherAlert::Upcoming(_) => theme.warning,
                WeatherAlert::None => theme.text,
            },
            None => theme.text,
        };

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
    fn build_content(&self, t: &ShellTheme) -> Element<'_, Message> {
        let (fg, muted, warn, danger) = (t.text, t.muted, t.warning, t.danger);
        let pad_x = (t.slant_width * 2.0 + 12.0).max(28.0);
        let pad = Padding { top: 20.0, right: pad_x, bottom: 20.0, left: pad_x };

        let close_btn: Element<'_, Message> = button(text("✕").size(11).color(muted))
            .on_press(Message::Dismiss)
            .padding([2, 6])
            .into();

        if let Some(wx) = &self.weather {
            let alert: Element<'_, Message> = match &wx.alert {
                WeatherAlert::Danger(s) => text(s.clone()).size(13).color(danger).into(),
                WeatherAlert::Upcoming(s) => text(s.clone()).size(13).color(warn).into(),
                WeatherAlert::None => text("").size(0).into(),
            };
            let mut hrs: iced::widget::Row<'_, Message> = iced::widget::Row::new().spacing(14).align_y(Alignment::Center);
            for hh in wx.hourly.iter().take(6) {
                hrs = hrs.push(column![
                    text(&hh.time_label).size(11).color(muted),
                    text(hh.icon).size(20),
                    text(format!("{}°", hh.temp_f)).size(12).color(fg),
                ].spacing(2).align_x(Alignment::Center));
            }
            column![
                row![
                    text(wx.icon).size(40),
                    column![text(format!("{}°F", wx.temp_f)).size(28).color(fg), text(format!("{}°C", wx.temp_c)).size(14).color(muted)].spacing(0),
                    text(wx.desc).size(16).color(fg).width(Length::Fill),
                    close_btn,
                ].spacing(16).align_y(Alignment::Center),
                alert,
                hrs,
            ].spacing(14).padding(pad)
             .width(Length::Fixed(self.w as f32))
             .height(Length::Fixed(H as f32))
             .into()
        } else if self.fetched {
            column![text("Weather unavailable").size(16).color(muted)].spacing(8).padding(pad).width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
        } else {
            column![text("Loading weather…").size(16).color(muted)].spacing(8).padding(pad).width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
        }
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    eprintln!("[tip-output] raw={:?} parsed={:?}", std::env::var("MBS_OUTPUT"), output);
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(340);
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
