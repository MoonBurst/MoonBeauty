use iced::widget::{column, container, row, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use std::time::Instant;

use crate::bar::weather::client::{WeatherAlert, WeatherData};
use crate::bar::weather::WeatherBackend;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::config::ShellSettings;
use crate::libraries::style::shape_box::SlantSide;
use crate::libraries::style::TextExt;

pub use crate::overlays::tooltip_common::slanted_row;

const W_START: f32 = 130.0;
const AUTO_MS: u128 = 20_000;

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Loaded(Option<WeatherData>),
    Dismiss,
}

impl TryInto<iced_layershell::actions::LayershellCustomActionWithId> for Message {
    type Error = Message;
    fn try_into(
        self,
    ) -> Result<iced_layershell::actions::LayershellCustomActionWithId, Self::Error> {
        Err(self)
    }
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
    closing_at: Option<crate::overlays::tooltip_engine::ClosingState>,
    anim_frame: u64,
    // Cached at boot so view() doesn't reload from disk at 60fps
    // during the drop/expand animation (~400ms of ~24 frames each).
    settings: ShellSettings,
    theme: ShellTheme,
}

#[allow(dead_code)]
impl App {
    fn new_app() -> (Self, Task<Message>) {
        crate::libraries::process::install_sigterm_handler();
        let cx: i32 = std::env::var("MBS_CAPSULE_CX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(200);
        let sw: i32 = std::env::var("MBS_SCREEN_W")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1920);
        let grow_right = std::env::var("MBS_GROW_RIGHT")
            .map(|v| v == "1")
            .unwrap_or_else(|_| cx < sw / 2);
        let w: u32 = std::env::var("MBS_TOOLTIP_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(420);
        let s = ShellSettings::load();
        let h: u32 = std::env::var("MBS_TOOLTIP_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(520);
        eprintln!("[weather_tooltip-h] MBS_TOOLTIP_H = {}", h);
        eprintln!("[child-h] weather_tooltip MBS_TOOLTIP_H = {}", h);

        let drop_ms = s.tooltip_drop_ms as f32;
        let expand_ms = s.tooltip_expand_ms as f32;
        let w_start: f32 = std::env::var("MBS_CAPSULE_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(W_START);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" {
            SlantSide::Right
        } else {
            SlantSide::Left
        };
        let slant_r = if sr_str == "R" {
            SlantSide::Right
        } else {
            SlantSide::Left
        };

        let app = Self {
            opened: Instant::now(),
            weather: None,
            fetched: false,
            grow_right,
            w,
            w_start,
            slant_l,
            slant_r,
            h,
            drop_ms,
            expand_ms,
            closing_at: None,
            anim_frame: 0,
            settings: s.clone(),
            theme: ShellTheme::from_settings(&s),
        };
        let task = Task::perform(
            async { WeatherBackend::fetch().await.ok() },
            Message::Loaded,
        );
        (
            app,
            Task::batch(vec![
                task,
                Task::perform(
                    async { tokio::time::sleep(std::time::Duration::from_millis(4)).await },
                    |_| Message::Tick,
                ),
            ]),
        )
    }

    fn snapshot_for_close(&self) -> crate::overlays::tooltip_engine::ClosingState {
        let cur = crate::overlays::tooltip_engine::compute(
            self.opened.elapsed().as_millis() as f32,
            self.w_start,
            self.w as f32,
            self.h as f32,
            self.theme.slant_width,
            self.settings.bar_height.max(1) as f32,
            self.drop_ms,
            self.expand_ms,
        );
        crate::overlays::tooltip_engine::ClosingState {
            started: Instant::now(),
            from_w: cur.w,
            from_h: cur.h,
            from_opacity: cur.opacity,
        }
    }

    fn namespace(&self) -> String {
        "mbs-wx-tip".into()
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                if crate::libraries::process::was_sigterm_signaled()
                    && self.closing_at.is_none()
                {
                    let snap = self.snapshot_for_close();
                    self.closing_at = Some(snap);
                }
                self.anim_frame = self.anim_frame.wrapping_add(1);
                let elapsed_ms = self.opened.elapsed().as_millis() as u64;
                if std::env::var_os("MBS_TICK_DIAG").is_some() {
                    eprintln!("[tick] frame={} elapsed={}ms", self.anim_frame, elapsed_ms);
                }

                if let Some(t) = &self.closing_at {
                    if t.started.elapsed().as_millis() > (self.drop_ms * 3.0) as u128 {
                        std::process::exit(0);
                    }
                } else if self.opened.elapsed().as_millis() > AUTO_MS {
                    std::process::exit(0);
                }

                let next_ms: u64 = if elapsed_ms < 2000 || self.closing_at.is_some() {
                    std::env::var("MBS_TICK_MS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .filter(|n| *n >= 1 && *n <= 100)
                        .unwrap_or(4)
                } else {
                    // 100ms (was 500) so SIGTERM-close is picked up within
                    // ~100ms during idle; without this, the tooltip sits
                    // visible for up to 500ms before the close animation
                    // can start.
                    100
                };

                return Task::perform(
                    async move { tokio::time::sleep(std::time::Duration::from_millis(next_ms)).await },
                    |_| Message::Tick,
                );
            }
            Message::Loaded(w) => {
                self.weather = w;
                self.fetched = true;
            }
            Message::Dismiss => {
                if self.closing_at.is_none() {
                    let snap = self.snapshot_for_close();
                    self.closing_at = Some(snap);
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = &self.settings;
        let theme = &self.theme;
        let anim = if let Some(t) = &self.closing_at {
            crate::overlays::tooltip_engine::compute_close_from(
                t.started.elapsed().as_millis() as f32,
                t.from_w,
                t.from_h,
                t.from_opacity,
                self.w_start,
                self.w as f32,
                theme.slant_width,
                settings.bar_height as f32,
                self.drop_ms,
            )
        } else {
            crate::overlays::tooltip_engine::compute(
                self.opened.elapsed().as_millis() as f32,
                self.w_start,
                self.w as f32,
                self.h as f32,
                theme.slant_width,
                settings.bar_height as f32,
                self.drop_ms,
                self.expand_ms,
            )
        };

        let slant_h = theme.slant_width * (anim.h / settings.bar_height.max(1) as f32);
        let bg = crate::overlays::tooltip_engine::ParallelogramBg {
            w: anim.w,
            h: anim.h,
            slant: slant_h,
            slant_right: self.slant_l == SlantSide::Right,
            background: theme.background,
            border_color: theme.text,
            border_width: theme.control_border_width,
            frame: self.opened.elapsed().as_millis() as u64,
        };

        let content: Element<'_, Message> = self.build_content(theme, settings, anim.opacity);
        let clipped = crate::overlays::tooltip_engine::clip_content(
            content,
            anim.w,
            anim.h,
            if self.grow_right {
                iced::alignment::Horizontal::Left
            } else {
                iced::alignment::Horizontal::Right
            },
        );
        let stacked: Element<'_, Message> = Stack::new().push(bg.view()).push(clipped).into();
        crate::overlays::tooltip_engine::align_to_side(
            stacked,
            if self.grow_right {
                iced::alignment::Horizontal::Left
            } else {
                iced::alignment::Horizontal::Right
            },
        )
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        // ESC + blur + diagnostic event logging. The eprintlns are temporary
        // while we diagnose why ESC isn't reaching the tooltip — removed
        // once confirmed.
        iced::event::listen_with(|event, _status, _window| {
            match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => {
                    Some(Message::Dismiss)
                }
                iced::Event::Window(iced::window::Event::Focused) => {
                    None
                }
                iced::Event::Window(iced::window::Event::Unfocused) => {
                    None
                }
                _ => None,
            }
        })
    }
    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

impl App {
    fn build_content<'a>(
        &'a self,
        t: &ShellTheme,
        settings: &ShellSettings,
        alpha: f32,
    ) -> Element<'a, Message> {
        let a = move |c: Color| Color {
            a: c.a * alpha,
            ..c
        };
        let text_col = a(t.text);
        let _muted = a(t.muted);
        let danger = a(t.danger);
        let warning = a(t.warning);

        let fs = settings.tooltip_font_size;
        let total_h = self.h as f32;
        let bar_h = settings.bar_height.max(1) as f32;
        let slant_h = t.slant_width * (total_h / bar_h);
        let slant_right = self.slant_l == SlantSide::Right;
        let _half_tt = settings.control_border_width * 0.5;
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
            text(title).size((fs.max(12)) as f32).bold().color(
                if let Some(WeatherAlert::Danger(_)) = self.weather.as_ref().map(|w| &w.alert) {
                    danger
                } else {
                    text_col
                }
            ),
            iced::widget::space::horizontal(),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();
        col = col.push(slanted_row(
            header_el,
            y + header_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            header_h,
        ));
        y += header_h + gap;

        if let Some(wx) = &self.weather {
            let cell_h = fs as f32 + 12.0;
            let mut list_col = column![].spacing(2).width(Length::Fill);

            for (i, line) in wx.tooltip_lines.iter().take(12).enumerate() {
                let color = if line.contains("Danger") || line.contains("Storm") {
                    danger
                } else if line.contains("Alert") || line.contains("Rain") || line.contains("Snow") {
                    warning
                } else {
                    text_col
                };

                let txt = if i == 0 {
                    text(line)
                        .size((fs) as f32)
                        .font(iced::Font::MONOSPACE)
                        .bold()
                        .color(color)
                        .wrapping(iced::widget::text::Wrapping::None)
                } else {
                    text(line)
                        .size((fs) as f32)
                        .font(iced::Font::MONOSPACE)
                        .color(color)
                        .wrapping(iced::widget::text::Wrapping::None)
                };
                let row_el = row![txt]
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();

                list_col = list_col.push(slanted_row(
                    row_el,
                    y + cell_h * 0.5,
                    total_h,
                    slant_h,
                    slant_right,
                    safe,
                    w_f,
                    cell_h,
                ));
                y += cell_h + 2.0;
            }

            col = col.push(list_col);
        } else if self.fetched {
            col = col.push(slanted_row(
                text("Weather unavailable").into(),
                y + 20.0,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                40.0,
            ));
        }

        container(col)
            .padding(Padding {
                top: vpad,
                right: 0.0,
                bottom: vpad,
                left: 20.0,
            })
            .width(Length::Fixed(self.w as f32))
            .height(Length::Fixed(self.h as f32))
            .clip(true)
            .into()
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(420);
    let h: u32 = std::env::var("MBS_TOOLTIP_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(520);
    eprintln!("[weather_tooltip-h] MBS_TOOLTIP_H = {}", h);
    eprintln!("[child-h] weather_tooltip MBS_TOOLTIP_H = {}", h);

    let cx: i32 = std::env::var("MBS_CAPSULE_CX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1920);
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100.0);
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);
    let half_cap = (cap_w / 2.0) as i32;

    // Finding 76: per-capsule bias added to global offsets.
    let (bias_l, bias_r) = crate::bar::tooltip_spawn::read_offset_extra();
    let adjust_left = s.effective_tooltip_offset_left() + bias_l;
    let adjust_right = s.effective_tooltip_offset_right() + bias_r;

    let (anchor, margin) = if grow_right {
        (
            Anchor::Top | Anchor::Left,
            (margin_top, 0, 0, (cx - half_cap + adjust_left).max(0)),
        )
    } else {
        (
            Anchor::Top | Anchor::Right,
            (margin_top, (sw - cx - half_cap + adjust_right).max(0), 0, 0),
        )
    };

    let layer = LayerShellSettings {
        size: Some((w, h)),
        exclusive_zone: -1,
        anchor,
        layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        start_mode: output
            .map(iced_layershell::settings::StartMode::TargetScreen)
            .unwrap_or(iced_layershell::settings::StartMode::Active),
        margin,
        ..Default::default()
    };
    iced_layershell::application(
        App::new_app,
        || "mbs-wx-tip".to_string(),
        App::update,
        App::view,
    )
    .subscription(App::subscription)
    .style(|_state, _theme| iced::theme::Style {
        background_color: iced::Color::TRANSPARENT,
        text_color: iced::Color::WHITE,
    })
    .settings(Settings {
        layer_settings: layer,
        ..Default::default()
    })
    .run()
}
