use iced::widget::{column, container, row, text, Space, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::backend::weather::{WeatherAlert, WeatherData};
use crate::backend::WeatherBackend;
use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::SlantSide;
use crate::style::TextExt;

const W_START: f32 = 130.0;
const AUTO_MS: u128 = 20_000;

#[derive(Debug, Clone)]
pub enum Message { Tick, Loaded(Option<WeatherData>), Dismiss }

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error> { Err(self) }
}

pub struct App {
    opened: Instant,
    weather: Option<WeatherData>,
    fetched: bool,
    grow_right: bool,
    w: u32,
    h: u32,
    drop_ms: f32,
    expand_ms: f32,
    w_start: f32,
    slant_l: SlantSide,
    #[allow(dead_code)]
    slant_r: SlantSide,
    closing_at: Option<Instant>,
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_: ()) -> (Self, Task<Message>) {
        let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let sw: i32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|s| s.parse().ok()).unwrap_or(1920);
        let grow_right = std::env::var("MBS_GROW_RIGHT").map(|v| v == "1").unwrap_or_else(|_| cx < sw / 2);
        let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(420);
        let s = ShellSettings::load();
        let h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(520);
        
        let drop_ms = s.tooltip_drop_ms as f32;
        let expand_ms = s.tooltip_expand_ms as f32;
        let w_start: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let slant_r = if sr_str == "R" { SlantSide::Right } else { SlantSide::Left };

        let app = Self { opened: Instant::now(), weather: None, fetched: false, grow_right, w, w_start, slant_l, slant_r, h, drop_ms, expand_ms, closing_at: None };
        let task = Task::perform(async { WeatherBackend::fetch().await.ok() }, Message::Loaded);
        (app, task)
    }

    fn namespace(&self) -> String { "mbs-wx-tip".into() }
    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                if let Some(t) = self.closing_at {
                    if t.elapsed().as_millis() > (self.drop_ms * 1.9) as u128 { std::process::exit(0); }
                } else if self.opened.elapsed().as_millis() > AUTO_MS { std::process::exit(0); }
            }
            Message::Loaded(w) => { self.weather = w; self.fetched = true; }
            Message::Dismiss => { self.closing_at = Some(Instant::now()); }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let anim = if let Some(t) = self.closing_at {
            crate::overlays::tooltip_engine::compute_close(
                t.elapsed().as_millis() as f32, self.w_start, self.w as f32, self.h as f32,
                theme.slant_width, settings.bar_height as f32, self.drop_ms,
            )
        } else {
            crate::overlays::tooltip_engine::compute(
                self.opened.elapsed().as_millis() as f32, self.w_start, self.w as f32, self.h as f32,
                theme.slant_width, settings.bar_height as f32, self.drop_ms, self.expand_ms,
            )
        };
        
        let slant_h = theme.slant_width * (anim.h / settings.bar_height.max(1) as f32);
        let bg = crate::overlays::tooltip_engine::ParallelogramBg {
            w: anim.w, h: anim.h, slant: slant_h,
            slant_right: self.slant_l == SlantSide::Right,
            background: theme.background, border_color: theme.text, border_width: theme.control_border_width,
        };

        let content: Element<'_, Message> = self.build_content(&theme, &settings, anim.opacity);
        let clipped = crate::overlays::tooltip_engine::clip_content(content, anim.w, anim.h, self.grow_right);
        let stacked: Element<'_, Message> = Stack::new().push(bg.view()).push(clipped).into();
        crate::overlays::tooltip_engine::align_to_side(stacked, self.grow_right)
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick),
            iced::keyboard::on_key_press(|k, _| {
                if k == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) { Some(Message::Dismiss) } else { None }
            }),
        ])
    }
    fn theme(&self) -> Theme { Theme::Dark }
}

impl App {
    fn build_content<'a>(&'a self, t: &ShellTheme, settings: &ShellSettings, alpha: f32) -> Element<'a, Message> {
        let a = move |c: Color| Color { a: c.a * alpha, ..c };
        let text_col = a(t.text);
        let muted = a(t.muted);
        let danger = a(t.danger);
        let warning = a(t.warning);

        let fs = settings.tooltip_font_size;
        let total_h = self.h as f32;
        let bar_h = settings.bar_height.max(1) as f32;
        let slant_h = t.slant_width * (total_h / bar_h);
        let slant_right = self.slant_l == SlantSide::Right;
        let half_tt = settings.control_border_width * 0.5;
        let safe = 16.0;
        let vpad = 14.0;
        let gap = 8.0;
        let w_f = self.w as f32;

        let mut y = vpad;
        let mut col = column![].spacing(gap).width(Length::Fill);

        // Header
        let header_h = fs as f32 + 16.0;
        let title = match &self.weather.as_ref().map(|w| &w.alert) {
            Some(WeatherAlert::Danger(_)) => "DANGER",
            Some(WeatherAlert::Upcoming(_)) => "WARNING",
            _ => "LOCAL WEATHER",
        };
        let header_el = row![
            text(title).size(fs.max(12)).bold().color(if let Some(WeatherAlert::Danger(_)) = self.weather.as_ref().map(|w| &w.alert) { danger } else { text_col }),
            iced::widget::horizontal_space(),
        ].align_y(Alignment::Center).width(Length::Fill).into();
        col = col.push(slanted_row(header_el, y + header_h * 0.5, total_h, slant_h, slant_right, safe, w_f, header_h));
        y += header_h + gap;

        if let Some(wx) = &self.weather {
            let cell_h = fs as f32 + 12.0;
            let mut list_col = column![].spacing(2).width(Length::Fill);

            for (i, line) in wx.tooltip_lines.iter().take(12).enumerate() {
                let color = if line.contains("Danger") || line.contains("Storm") { danger }
                            else if line.contains("Alert") || line.contains("Rain") || line.contains("Snow") { warning }
                            else { text_col };
                
                let txt = if i == 0 {
                    text(line).size(fs).font(iced::Font::MONOSPACE).bold().color(color).wrapping(iced::widget::text::Wrapping::None)
                } else {
                    text(line).size(fs).font(iced::Font::MONOSPACE).color(color).wrapping(iced::widget::text::Wrapping::None)
                };
                let row_el = row![txt].align_y(Alignment::Center).width(Length::Fill).into();
                
                list_col = list_col.push(slanted_row(row_el, y + cell_h * 0.5, total_h, slant_h, slant_right, safe, w_f, cell_h));
                y += cell_h + 2.0;
            }

            col = col.push(list_col);
        } else if self.fetched {
            col = col.push(slanted_row(text("Weather unavailable").into(), y + 20.0, total_h, slant_h, slant_right, safe, w_f, 40.0));
        } else {
            col = col.push(slanted_row(text("Loading weather...").into(), y + 20.0, total_h, slant_h, slant_right, safe, w_f, 40.0));
        }

        container(col)
            .padding(Padding { top: vpad, right: 0.0, bottom: vpad, left: 0.0 })
            .width(Length::Fixed(self.w as f32))
            .height(Length::Fixed(self.h as f32))
            .clip(true)
            .into()
    }
}

fn slanted_row<'a, M: 'a>(
    inner: Element<'a, M>,
    y_mid: f32,
    total_h: f32,
    slant: f32,
    slant_right: bool,
    safe: f32,
    total_w: f32,
    row_h: f32,
) -> Element<'a, M> {
    let frac = (y_mid / total_h.max(1.0)).clamp(0.0, 1.0);
    let (lp, rp) = if slant_right {
        (slant * (1.0 - frac) + safe, slant * frac + safe)
    } else {
        (slant * frac + safe, slant * (1.0 - frac) + safe)
    };
    let inner_w = (total_w - lp - rp).max(10.0);

    let inner_clipped = container(inner)
        .width(Length::Fixed(inner_w))
        .height(Length::Fixed(row_h))
        .clip(true);

    row![
        Space::new(Length::Fixed(lp), Length::Shrink),
        inner_clipped,
        Space::new(Length::Fixed(rp), Length::Shrink),
    ].height(Length::Fixed(row_h)).width(Length::Fixed(total_w)).into()
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(420);
    let h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(520);

    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(100.0);
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);
    let half_cap = (cap_w / 2.0) as i32;
    let slant_px = s.slant_width as i32;

    let (anchor, margin) = if grow_right {
        (Anchor::Top | Anchor::Left, (margin_top, 0, 0, (cx - half_cap).max(0)))
    } else {
        (Anchor::Top | Anchor::Right, (margin_top, (sw - cx - half_cap - slant_px).max(0), 0, 0))
    };

    let layer = LayerShellSettings {
        size: Some((w, h)), exclusive_zone: -1, anchor, layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        binded_output_name: output, margin, ..Default::default()
    };
    App::run(Settings { layer_settings: layer, ..Default::default() })
}
