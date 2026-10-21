use iced::widget::{button, column, container, horizontal_space, row, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::backend::{BorgBackend, BorgStatus, ClockBackend, MprisBackend, MprisTrack,
                     PodmanBackend, PodmanStatus, SystemCollector};
use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::style::TextExt;

const H: u32 = 240;
const W_START: f32 = 130.0;
const AUTO_MS: u128 = 15_000;

#[derive(Debug, Clone)]
pub enum InfoData {
    Calendar(crate::backend::CalendarViewData),
    Music(Option<MprisTrack>),
    Unified(PodmanStatus, BorgStatus),
    Net(crate::backend::NetMetrics),
    Custom,
}

#[derive(Debug, Clone)]
pub enum Message { Tick, Loaded(InfoData), Dismiss }

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(self) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error> { Err(self) }
}

pub struct App {
    opened: Instant,
    mode: String,
    data: Option<InfoData>,
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
        let mode = std::env::var("MBS_INFO_MODE").unwrap_or_else(|_| "custom".into());
        let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|s| s.parse().ok()).unwrap_or(1920);
        let grow_right = std::env::var("MBS_GROW_RIGHT").map(|v| v == "1").unwrap_or_else(|_| cx < sw / 2);
        let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
        let w_start: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
        let sl = std::env::var("MBS_SLANT_L").ok().map(|s| if s == "R" { SlantSide::Right } else { SlantSide::Left }).unwrap_or(SlantSide::Left);
        let sr = std::env::var("MBS_SLANT_R").ok().map(|s| if s == "R" { SlantSide::Right } else { SlantSide::Left }).unwrap_or(SlantSide::Left);

        let app = Self { opened: Instant::now(), mode: mode.clone(), data: None, grow_right, w, w_start, slant_l: sl, slant_r: sr };
        let task = Task::perform(fetch_data(mode), Message::Loaded);
        (app, task)
    }

    fn namespace(&self) -> String { format!("mbs-info-tip-{}", self.mode) }
    fn style(&self, _t: &Self::Theme) -> iced_layershell::Appearance { iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE } }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => { if self.opened.elapsed().as_millis() > AUTO_MS { std::process::exit(0); } }
            Message::Loaded(d) => { self.data = Some(d); }
            Message::Dismiss => std::process::exit(0),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let anim = crate::overlays::tooltip_anim::compute(self.opened.elapsed().as_millis() as f32, self.w_start, self.w as f32, H as f32);
        let slant = (theme.slant_width * (anim.h / settings.bar_height as f32)).max(0.0);

        let bg = ShapeBox::new()
            .shape(ShapeType::Slant { left: self.slant_l, right: self.slant_r })
            .background(theme.background)
            .border(theme.text, theme.control_border_width)
            .slant_width(slant)
            .size(Length::Fixed(self.w as f32), Length::Fixed(anim.h));

        let content: Element<'_, Message> = self.build_content(&theme);
        let clipped = crate::overlays::tooltip_anim::clip_content(content, anim.w, anim.h, self.grow_right);
        let stacked: Element<'_, Message> = Stack::new().push(bg.view()).push(clipped).into();
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
        let pad = Padding { top: 16.0, right: 24.0, bottom: 16.0, left: 24.0 };
        let close: Element<'_, Message> = button(text("✕").size(11).color(t.muted)).on_press(Message::Dismiss).into();
        let header = |title: &'static str| -> Element<'_, Message> {
            row![text(title).size(13).bold().color(t.success), horizontal_space(), close].align_y(Alignment::Center).into()
        };

        let body: Element<'_, Message> = match self.data.as_ref() {
            None => column![text("Loading…").size(13).color(t.muted)].into(),
            Some(InfoData::Calendar(c)) => {
                let day_of_week = chrono::Local::now().format("%A").to_string();
                let month = chrono::Local::now().format("%B %Y").to_string();
                // Week containing today
                let today = c.today_day;
                let sun0 = today.saturating_sub(c.first_day_offset as u32 + 6).max(1);
                let _ = sun0;
                let week_start = today.saturating_sub((c.first_day_offset as u32 + 7 - (c.first_day_offset as u32 + 7)) / 1).max(1);
                let _ = week_start;
                // Simple 7-day strip around today
                let start = today.saturating_sub(3).max(1);
                let mut strip: iced::widget::Row<'_, Message> = iced::widget::Row::new().spacing(6);
                for i in 0..7 {
                    let d = start + i;
                    if d > c.days_in_month { break; }
                    let is_today = d == today;
                    let color = if is_today { t.danger } else { t.text };
                    let weight = if is_today { iced::font::Weight::Bold } else { iced::font::Weight::Normal };
                    strip = strip.push(
                        text(format!("{:>2}", d)).size(15)
                            .color(color)
                            .font(iced::Font { weight, ..Default::default() })
                    );
                }
                column![
                    header("📅 CALENDAR"),
                    text(day_of_week).size(16).color(t.text),
                    text(month).size(12).color(t.muted),
                    container(text("")).height(Length::Fixed(1.0)),
                    strip,
                ].spacing(8).padding(pad)
                 .width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
            }
            Some(InfoData::Music(track_opt)) => {
                let (title, artist, album, playing, pos, len) = match track_opt {
                    Some(tr) => (tr.title.clone(), tr.artist.clone(), tr.album.clone(), tr.is_playing, tr.position_sec, tr.length_sec),
                    None => ("No track".into(), "".into(), "".into(), false, 0.0, 0.0),
                };
                let pos_str = format!("{}:{:02}", (pos as u32) / 60, (pos as u32) % 60);
                let len_str = format!("{}:{:02}", (len as u32) / 60, (len as u32) % 60);
                let pct = if len > 0.0 { (pos / len).clamp(0.0, 1.0) } else { 0.0 };
                let bar_w = (self.w as f32 - 64.0).max(100.0);
                let played = bar_w * pct as f32;
                let icon = if playing { "⏸" } else { "▶" };
                column![
                    header("🎵 NOW PLAYING"),
                    text(title).size(15).bold().color(t.text),
                    text(format!("{} — {}", artist, album)).size(12).color(t.muted),
                    row![
                        container(text("")).width(Length::Fixed(played)).height(Length::Fixed(3.0)),
                        container(text("")).width(Length::Fixed((bar_w - played).max(0.0))).height(Length::Fixed(3.0)),
                    ],
                    row![text(pos_str).size(11).color(t.muted), horizontal_space(), text(len_str).size(11).color(t.muted)],
                    text(format!("{}   ⏮   ⏭", icon)).size(22).color(t.success).align_x(Alignment::Center),
                ].spacing(8).padding(pad)
                 .width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
            }
            Some(InfoData::Unified(podman, borg)) => {
                let mut col: iced::widget::Column<'_, Message> = iced::widget::Column::new().spacing(4);
                for c in podman.containers.iter().take(3) {
                    let dot = if c.is_running { "●" } else { "○" };
                    let color = if c.is_running { t.success } else { t.muted };
                    col = col.push(row![
                        text(dot).size(12).color(color),
                        text(&c.name).size(12).color(t.text).width(Length::Fill),
                        text(&c.status).size(10).color(t.muted),
                    ].spacing(6).align_y(Alignment::Center));
                }
                if podman.containers.is_empty() {
                    col = col.push(text("No containers").size(12).color(t.muted));
                }
                column![
                    header("⚙ SYSTEM STATUS"),
                    col,
                    container(text("")).height(Length::Fixed(1.0)),
                    row![text("BORG:").size(11).bold().color(t.muted), text(&borg.state_label).size(11).color(t.text)],
                ].spacing(8).padding(pad)
                 .width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
            }
            Some(InfoData::Net(net)) => {
                column![
                    header("🌐 NETWORK"),
                    text(format!("{} ({})", if net.iface.is_empty() { "—" } else { &net.iface }, if net.hw_type.is_empty() { "?" } else { &net.hw_type })).size(13).color(t.text),
                    text(format!("IP: {}", if net.ip.is_empty() { "—" } else { &net.ip })).size(11).color(t.muted),
                    container(text("")).height(Length::Fixed(1.0)),
                    row![text("▼ ").size(13).color(t.success), text(&net.down_str).size(15).bold().color(t.text)],
                    row![text("▲ ").size(13).color(t.success), text(&net.up_str).size(15).bold().color(t.text)],
                ].spacing(8).padding(pad)
                 .width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
            }
            Some(InfoData::Custom) => {
                column![
                    header("⚙ CUSTOM MODULE"),
                    text("Configure in Settings → Bar Layout").size(12).color(t.muted),
                ].spacing(8).padding(pad)
                 .width(Length::Fixed(self.w as f32)).height(Length::Fixed(H as f32)).into()
            }
        };

        body
    }
}

async fn fetch_data(mode: String) -> InfoData {
    match mode.as_str() {
        "calendar" => InfoData::Calendar(ClockBackend::compute_calendar(0, 0)),
        "music" => InfoData::Music(MprisBackend::poll().await),
        "unified" => {
            let p = PodmanBackend::poll().await;
            let b = BorgBackend::poll().await;
            InfoData::Unified(p, b)
        }
        "net" => {
            let mut sc = SystemCollector::new();
            let _ = sc.update_net();
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            InfoData::Net(sc.update_net())
        }
        _ => InfoData::Custom,
    }
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let grow_right = std::env::var("MBS_GROW_RIGHT").map(|v| v == "1").unwrap_or_else(|_| cx < sw / 2);
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
    let half_start = (cap_w / 2.0) as i32;
    let bar_h = s.bar_height as i32;
    let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
    let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
    let sw_cap = s.slant_width;
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
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

    let layer = LayerShellSettings {
        size: Some((w, H)),
        exclusive_zone: -1,
        anchor,
        layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        binded_output_name: output,
        margin,
        ..Default::default()
    };
    App::run(Settings { layer_settings: layer, ..Default::default() })
}
