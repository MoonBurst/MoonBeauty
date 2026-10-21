use iced::widget::{column, row, text, Stack};
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

const W: u32 = 340;
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
    slant_mode: String,
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_: ()) -> (Self, Task<Message>) {

        let slant_mode = std::env::var("MBS_WX_SLANT").unwrap_or_else(|_| "left".into());

        let app = Self {
            opened: Instant::now(),
            weather: None,
            fetched: false,
            slant_mode,
        };
        let task = Task::perform(
            async { WeatherBackend::fetch().await.ok() },
            Message::Loaded,
        );
        (app, task)
    }

    fn namespace(&self) -> String { "mbs-wx-tip".into() }

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
            }
            Message::Loaded(w) => { self.weather = w; self.fetched = true; }
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

        // Match weather alert level
        let border_color = match &self.weather {
            Some(w) => match &w.alert {
                WeatherAlert::Danger(_) => theme.danger,
                WeatherAlert::Upcoming(_) => theme.warning,
                WeatherAlert::None => theme.text,
            },
            None => theme.text,
        };

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
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick),
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
    fn build_content(&self, t: &ShellTheme) -> Element<'_, Message> {
        let (fg, muted, warn, danger) = (t.text, t.muted, t.warning, t.danger);
        let pad_x = (t.slant_width * 2.0 + 12.0).max(28.0);
        let pad = Padding { top: 20.0, right: pad_x, bottom: 20.0, left: pad_x };

        if let Some(wx) = &self.weather {
            let alert: Element<Message> = match &wx.alert {
                WeatherAlert::Danger(s) => text(s.clone()).size(13).color(danger).into(),
                WeatherAlert::Upcoming(s) => text(s.clone()).size(13).color(warn).into(),
                WeatherAlert::None => text("").size(0).into(),
            };
            let mut hrs = row![].spacing(14).align_y(Alignment::Center);
            for hh in wx.hourly.iter().take(6) {
                hrs = hrs.push(
                    column![
                        text(&hh.time_label).size(11).color(muted),
                        text(hh.icon).size(20),
                        text(format!("{}°", hh.temp_f)).size(12).color(fg),
                    ].spacing(2).align_x(Alignment::Center),
                );
            }
            column![
                row![
                    text(wx.icon).size(40),
                    column![
                        text(format!("{}°F", wx.temp_f)).size(28).color(fg),
                        text(format!("{}°C", wx.temp_c)).size(14).color(muted),
                    ].spacing(0),
                    text(wx.desc).size(16).color(fg).width(Length::Fill),
                ].spacing(16).align_y(Alignment::Center),
                alert,
                hrs,
            ].spacing(14).padding(pad)
             .width(Length::Fixed(W as f32))
             .height(Length::Fixed(H as f32))
             .into()
        } else if self.fetched {
            column![text("Weather unavailable").size(16).color(muted)]
                .spacing(8).padding(pad)
                .width(Length::Fixed(W as f32))
                .height(Length::Fixed(H as f32))
                .into()
        } else {
            column![text("Loading weather…").size(16).color(muted)]
                .spacing(8).padding(pad)
                .width(Length::Fixed(W as f32))
                .height(Length::Fixed(H as f32))
                .into()
        }
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let s = ShellSettings::load();
    let cx: i32 = std::env::var("MBS_CAPSULE_CX")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let half_start = (W_START / 2.0) as i32;

    let (anchor, margin) = if grow_right {
        (
            Anchor::Top | Anchor::Left,
            (bar_h, 0, 0, (cx - half_start).max(0)),
        )
    } else {
        (
            Anchor::Top | Anchor::Right,
            (bar_h, (sw - cx - half_start).max(0), 0, 0),
        )
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
