use crate::style::TextExt;
use iced::widget::{button, column, row, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::backend::system::{SystemCollector, ProcessItem};
use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

const W: u32 = 340;
const H: u32 = 240;
const W_START: f32 = 130.0;
const AUTO_MS: u128 = 10_000;

#[derive(Debug, Clone)]
pub enum Data {
    Cpu(crate::backend::system::CpuMetrics),
    Gpu(crate::backend::system::GpuMetrics),
    Ram(crate::backend::system::RamMetrics),
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Loaded(Data),
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
    target: String,
    data: Option<Data>,
    slant_mode: String,
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_: ()) -> (Self, Task<Message>) {
        let target = std::env::var("MBS_SYS_TARGET").unwrap_or_else(|_| "cpu".into());
        let slant_mode = std::env::var("MBS_WX_SLANT").unwrap_or_else(|_| "left".into());

        let app = Self {
            opened: Instant::now(),
            target: target.clone(),
            data: None,
            slant_mode,
        };

        let task = if target == "cpu" {
            Task::perform(SystemCollector::collect_cpu(), |d| Message::Loaded(Data::Cpu(d)))
        } else if target == "gpu" {
            Task::perform(SystemCollector::collect_gpu("card0", false, 0), |d| Message::Loaded(Data::Gpu(d)))
        } else {
            Task::perform(SystemCollector::collect_ram(), |d| Message::Loaded(Data::Ram(d)))
        };

        (app, task)
    }

    fn namespace(&self) -> String { format!("mbs-sys-tip-{}", self.target) }

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
            Message::Loaded(d) => { self.data = Some(d); }
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

        // Match the bar module's dynamic alert color
        let border_color = match &self.data {
            Some(Data::Cpu(c)) => {
                if c.temp_c >= 80 || c.usage_pct >= 90 {
                    theme.danger
                } else if c.temp_c >= 70 || c.usage_pct >= 75 {
                    theme.warning
                } else {
                    theme.text
                }
            }
            Some(Data::Gpu(g)) => {
                if g.temp_c >= theme.gpu0_temp_danger || g.vram_free_gib <= theme.gpu0_vram_danger {
                    theme.danger
                } else if g.temp_c >= theme.gpu0_temp_warn || g.vram_free_gib <= theme.gpu0_vram_warn {
                    theme.warning
                } else {
                    theme.text
                }
            }
            Some(Data::Ram(r)) => {
                let total = r.total_gib;
                let avail = r.effective_avail_gib;
                let usage_ratio = if total > 0.0 { ((total - avail).max(0.0)) / total } else { 0.0 };
                if usage_ratio >= 0.85 {
                    theme.danger
                } else if usage_ratio >= 0.50 {
                    theme.warning
                } else {
                    theme.text
                }
            }
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
        let (fg, muted, succ) = (t.text, t.muted, t.success);
        let pad_x = (t.slant_width * 2.0 + 12.0).max(28.0);
        let pad = Padding { top: 16.0, right: pad_x, bottom: 16.0, left: pad_x };

        let close_btn = button(text("✕").size(11).color(muted))
            .on_press(Message::Dismiss)
            .padding([2, 6])
            .style(|_t, _s| button::Style { background: None, ..Default::default() });

        let body: Element<Message> = if let Some(d) = &self.data {
            match d {
                Data::Cpu(c) => column![
                    row![text("CPU METRICS").size(14).bold().color(fg), row![].width(Length::Fill), close_btn].align_y(Alignment::Center),
                    text(format!("Usage: {}%", c.usage_pct)).size(15).color(succ),
                    text(format!("Temp: {}°C", c.temp_c)).size(13).color(muted),
                    Self::render_procs(&c.top_procs, t),
                ].spacing(6).into(),
                Data::Gpu(g) => column![
                    row![text(format!("GPU: {}", g.name)).size(14).bold().color(fg), row![].width(Length::Fill), close_btn].align_y(Alignment::Center),
                    text(format!("Usage: {}%", g.usage_pct)).size(15).color(succ),
                    text(format!("Temp: {}°C  •  {}W", g.temp_c, g.power_w)).size(13).color(muted),
                    text(format!("VRAM Free: {} GiB", g.vram_free_gib)).size(13).color(muted),
                    Self::render_procs(&g.top_procs, t),
                ].spacing(6).into(),
                Data::Ram(r) => column![
                    row![text("RAM USAGE").size(14).bold().color(fg), row![].width(Length::Fill), close_btn].align_y(Alignment::Center),
                    text(format!("Avail: {:.1} / {:.1} GiB", r.effective_avail_gib, r.total_gib)).size(15).color(succ),
                    text(format!("Zram Saved: {:.1} GiB", r.zram_saved_gib)).size(13).color(muted),
                    Self::render_procs(&r.top_procs, t),
                ].spacing(6).into(),
            }
        } else {
            column![
                row![text("LOADING METRICS...").size(14).color(muted), row![].width(Length::Fill), close_btn].align_y(Alignment::Center)
            ].into()
        };

        column![body].padding(pad)
            .width(Length::Fixed(W as f32))
            .height(Length::Fixed(H as f32))
            .into()
    }

    fn render_procs<'a>(procs: &'a [ProcessItem], t: &ShellTheme) -> Element<'a, Message> {
        let mut c = column![].spacing(4);
        for p in procs.iter().take(3) {
            c = c.push(
                row![
                    text(&p.name).size(12).color(t.text).width(Length::Fill),
                    text(&p.metric).size(12).color(t.muted),
                ]
            );
        }
        c.into()
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
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
