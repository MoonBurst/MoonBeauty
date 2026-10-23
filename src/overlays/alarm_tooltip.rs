use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{button, column, container, horizontal_space, row, scrollable, text, text_input, Space, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Point, Rectangle, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::time::Instant;

use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::overlays::reminders::{save_reminders, ReminderEntry, RemindersOverlay};
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::style::TextExt;

const H: u32 = 320;
const W_START: f32 = 130.0;
const AUTO_MS: u128 = 60_000;

static ACTIVE_FIELD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    AdjCountdownH(bool), // true = increment
    AdjCountdownM(bool),
    StartCountdown,
    AdjClockH(bool),
    AdjClockM(bool),
    ToggleAmPm,
    SetClock,
    DeleteTimer(u64),
    Dismiss,
    FocusNext,
    FocusPrev,
    InitialFocus,
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
    h: u32,
    drop_ms: f32,
    expand_ms: f32,
    slant_l: SlantSide,
    #[allow(dead_code)]
    slant_r: SlantSide,
    entries: Vec<ReminderEntry>,
    cd_h: u32,
    cd_m: u32,
    clk_h: u32,
    clk_m: u32,
    is_pm: bool,
    active_field: usize,
    closing_at: Option<Instant>,
    close_open_elapsed_ms: f32,
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
        let h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(H);
        let drop_ms: f32 = std::env::var("MBS_TOOLTIP_DROP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(220.0);
        let expand_ms: f32 = std::env::var("MBS_TOOLTIP_EXPAND_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(260.0);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let slant_r = if sr_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let entries = RemindersOverlay::load();

        let app = Self {
            opened: Instant::now(), grow_right, w, h, drop_ms, expand_ms,
            slant_l, slant_r,
            entries,
            cd_h: 0,
            cd_m: 5,
            clk_h: 12,
            clk_m: 0,
            is_pm: true,
            active_field: 0,
            closing_at: None,
            close_open_elapsed_ms: 0.0,
        };
        let focus_task = iced::Task::perform(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            },
            |_| Message::InitialFocus,
        );
        (app, focus_task)
    }

    fn namespace(&self) -> String { "mbs-alarm-tip".into() }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                if let Some(t) = self.closing_at {
                    if t.elapsed().as_millis() > (self.drop_ms * 1.9) as u128 {
                        std::process::exit(0);
                    }
                } else if self.opened.elapsed().as_millis() > AUTO_MS {
                    std::process::exit(0);
                }
                
                // Throttle disk loading to once per second, while keeping 60 FPS animation
                static TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let t = TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if t % 60 == 0 {
                    self.entries = RemindersOverlay::load();
                }
            }
            Message::AdjCountdownH(inc) => {
                if inc { self.cd_h = (self.cd_h + 1) % 24; }
                else { self.cd_h = (self.cd_h + 23) % 24; }
            }
            Message::AdjCountdownM(inc) => {
                if inc { self.cd_m = (self.cd_m + 1) % 60; }
                else { self.cd_m = (self.cd_m + 59) % 60; }
            }
            Message::StartCountdown => {
                let mins = (self.cd_h as i64 * 60) + self.cd_m as i64;
                if mins > 0 {
                    let now = chrono::Local::now();
                    let target = now + chrono::Duration::minutes(mins);
                    let max_id = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
                    self.entries.push(ReminderEntry {
                        id: max_id + 1,
                        target_epoch: target.timestamp(),
                        text: format!("Timer {}h {:02}m", self.cd_h, self.cd_m),
                        target_str: target.format("%I:%M %p").to_string(),
                        alerted: false,
                    });
                    save_reminders(&self.entries);
                    self.cd_h = 0; self.cd_m = 5; // reset
                }
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
            Message::SetClock => {
                let mut h = self.clk_h % 12;
                if self.is_pm { h += 12; }
                let now = chrono::Local::now();
                let mut target = now.date_naive()
                    .and_hms_opt(h, self.clk_m, 0).unwrap()
                    .and_local_timezone(chrono::Local).unwrap();
                if target < now { target = target + chrono::Duration::days(1); }
                let max_id = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
                self.entries.push(ReminderEntry {
                    id: max_id + 1,
                    target_epoch: target.timestamp(),
                    text: format!("Alarm {:02}:{:02} {}", self.clk_h, self.clk_m, if self.is_pm {"PM"} else {"AM"}),
                    target_str: target.format("%I:%M %p").to_string(),
                    alerted: false,
                });
                save_reminders(&self.entries);
            }
            Message::InitialFocus => {
                return iced::window::get_latest().and_then(iced::window::gain_focus);
            }
            Message::FocusNext => {
                self.active_field = (self.active_field + 1) % 7;
                ACTIVE_FIELD.store(self.active_field, std::sync::atomic::Ordering::Relaxed);
            }
            Message::FocusPrev => {
                self.active_field = (self.active_field + 6) % 7;
                ACTIVE_FIELD.store(self.active_field, std::sync::atomic::Ordering::Relaxed);
            }
            Message::DeleteTimer(id) => {
                self.entries.retain(|e| e.id != id);
                save_reminders(&self.entries);
            }
            Message::Dismiss => {
                if self.closing_at.is_none() {
                    let total_open_ms = self.drop_ms + self.expand_ms + (self.drop_ms * 0.6);
                    let current_elapsed = self.opened.elapsed().as_millis() as f32;
                    self.close_open_elapsed_ms = current_elapsed.min(total_open_ms).max(50.0);
                    self.closing_at = Some(Instant::now());
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);

        let time_ms = if let Some(t) = self.closing_at {
            // Reverse: pass elapsed time counting DOWN from where open left off
            let reverse_elapsed = self.close_open_elapsed_ms
                - (t.elapsed().as_millis() as f32);
            reverse_elapsed.max(0.0)
        } else {
            self.opened.elapsed().as_millis() as f32
        };

        let a = crate::overlays::tooltip_engine::compute(
            time_ms,
            self.w_start(),
            self.w as f32,
            self.h as f32,
            theme.slant_width,
            settings.bar_height as f32,
            self.drop_ms,
            self.expand_ms,
        );
        let (anim_w, anim_h, anim_opacity) = (a.w, a.h, a.opacity);

        let slant = (theme.slant_width * (anim_h / settings.bar_height as f32)).max(0.0);

        let bg = crate::overlays::tooltip_engine::ParallelogramBg {
            w: anim_w,
            h: anim_h,
            slant: slant,
            slant_right: self.slant_l == SlantSide::Right,
            background: theme.background,
            border_color: theme.text,
            border_width: theme.control_border_width,
        };

        let content = self.build_content(&theme, anim_opacity);
        let clipped = crate::overlays::tooltip_engine::clip_content(content, anim_w, anim_h, self.grow_right);
        let stacked: Element<'_, Message> = Stack::new().push(bg.view()).push(clipped).into();
        crate::overlays::tooltip_engine::align_to_side(stacked, self.grow_right)
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let tick = iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick);
        let keys = iced::keyboard::on_key_press(|k, _m| {
            let af = ACTIVE_FIELD.load(std::sync::atomic::Ordering::Relaxed);
            let esc = iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape);
            let up = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp);
            let down = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown);
            let left = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft);
            let right = iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight);
            let enter = iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter);
            let tab = iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab);

            if k == esc { return Some(Message::Dismiss); }
            
            // Focus navigation
            if k == left || k == tab { return Some(Message::FocusPrev); } // Tab backwards or left
            if k == right { return Some(Message::FocusNext); }          // Right moves forward

            // Value adjustment (Up/Down to match the buttons)
            if k == up {
                return match af {
                    0 => Some(Message::AdjCountdownH(true)),
                    1 => Some(Message::AdjCountdownM(true)),
                    3 => Some(Message::AdjClockH(true)),
                    4 => Some(Message::AdjClockM(true)),
                    5 => Some(Message::ToggleAmPm),
                    _ => None,
                };
            }
            if k == down {
                return match af {
                    0 => Some(Message::AdjCountdownH(false)),
                    1 => Some(Message::AdjCountdownM(false)),
                    3 => Some(Message::AdjClockH(false)),
                    4 => Some(Message::AdjClockM(false)),
                    5 => Some(Message::ToggleAmPm),
                    _ => None,
                };
            }

            if k == enter {
                return match af {
                    0 | 1 => Some(Message::StartCountdown),
                    3 | 4 | 5 => Some(Message::SetClock),
                    _ => None,
                };
            }
            None
        });
        iced::Subscription::batch([tick, keys])
    }
    fn theme(&self) -> Theme { Theme::Dark }
}

impl App {
    fn w_start(&self) -> f32 {
        std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START)
    }

    fn build_content<'a>(&'a self, t: &ShellTheme, alpha: f32) -> Element<'a, Message> {
        let apply_alpha = |c: Color| Color { a: c.a * alpha, ..c };
        let fg = apply_alpha(t.text);
        let muted = apply_alpha(t.muted);
        let danger = apply_alpha(t.danger);
        let warning = apply_alpha(t.danger);
        let focus = apply_alpha(t.focus);
        let caution = apply_alpha(t.danger);
        let surface_alt = apply_alpha(t.surface_alt);

        let slant_end = t.slant_width * (self.h as f32 / 42.0);
        let pad_x = ((slant_end + 16.0).min(self.w as f32 * 0.28)).max(20.0);
        let pad_top = (self.h as f32 * 0.08).max(16.0);
        let pad_bottom = (self.h as f32 * 0.08).max(16.0);
        let pad = Padding { top: pad_top, right: pad_x, bottom: pad_bottom, left: pad_x };

        let content_w = (self.w as f32 - 2.0 * pad_x).max(120.0);
        let title_size: u16 = ((content_w / 7.0) as u16).clamp(14, 26);
        let label_size: u16 = ((content_w / 10.0) as u16).clamp(10, 16);

        // ── Hex spinner with border highlight for active field ──
        macro_rules! hex_spinner {
        ($val:expr, $up_msg:expr, $down_msg:expr, $w:expr, $border:expr) => {{
            let btn_w = $w;
                let btn_h = 42.0;
                let cut = (btn_w * 0.08) as u16;
                let bg = ShapeBox::new()
                    .shape(ShapeType::Hexagon { cut })
                    .background(surface_alt)
                    .border($border, 1.5)
                    .size(Length::Fixed(btn_w), Length::Fixed(btn_h));

                let inner = row![
                    button(text("▼").size(14).color(fg)).on_press($down_msg).padding(4).style(|_t, _s| button::Style { background: None, ..Default::default() }),
                    text($val).size(18).bold().color(fg).width(Length::Fill).wrapping(iced::widget::text::Wrapping::None).align_x(Alignment::Center),
                    button(text("▲").size(14).color(fg)).on_press($up_msg).padding(4).style(|_t, _s| button::Style { background: None, ..Default::default() }),
                ].align_y(Alignment::Center).width(Length::Fixed(btn_w)).height(Length::Fixed(btn_h));

                let stacked: Element<'a, Message> = Stack::new().push(bg.view()).push(inner).into();
                stacked
            }};
        }

        // Spinner width calc — sized to fit all buttons on one row
        let cd_spinner_w = ((content_w - 60.0 - 16.0) / 2.0).max(70.0);
        let clk_spinner_w = ((content_w - 90.0 - 24.0) / 2.0).max(60.0);
        let cd_h_border = if self.active_field == 0 { caution } else { focus };
        let cd_m_border = if self.active_field == 1 { caution } else { focus };
        let clk_h_border = if self.active_field == 3 { caution } else { focus };
        let clk_m_border = if self.active_field == 4 { caution } else { focus };
        let ampm_border = if self.active_field == 5 { caution } else { focus };

        // ── Countdown row ──
        let cd_row = row![
            hex_spinner!(format!("{:02}h", self.cd_h), Message::AdjCountdownH(true), Message::AdjCountdownH(false), cd_spinner_w, cd_h_border),
            text(":").size(14).color(fg),
            hex_spinner!(format!("{:02}m", self.cd_m), Message::AdjCountdownM(true), Message::AdjCountdownM(false), cd_spinner_w, cd_m_border),

        ].spacing(6).align_y(Alignment::Center);

        // ── Clock row (narrower spinners + AM/PM + Set) ──
        let ampm_lbl = if self.is_pm { "PM" } else { "AM" };
        let ampm_btn = button(text(ampm_lbl).size(12).bold().color(fg))
            .on_press(Message::ToggleAmPm)
            .padding([6, 6])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if self.active_field == 5 { caution } else { surface_alt })),
                border: iced::Border { color: ampm_border, width: 1.5, radius: 4.0.into() },
                ..Default::default()
            });

        let clk_row = row![
            hex_spinner!(format!("{:02}", self.clk_h), Message::AdjClockH(true), Message::AdjClockH(false), clk_spinner_w, clk_h_border),
            text(":").size(14).color(fg),
            hex_spinner!(format!("{:02}", self.clk_m), Message::AdjClockM(true), Message::AdjClockM(false), clk_spinner_w, clk_m_border),
            ampm_btn,

        ].spacing(6).align_y(Alignment::Center);

        let now = chrono::Local::now().timestamp();
        let mut timers_col: iced::widget::Column<'a, Message> = iced::widget::Column::new().spacing(4).width(Length::Fill);
        for e in self.entries.iter().filter(|e| e.target_epoch > now) {
            let id = e.id;
            let diff = (e.target_epoch - now).max(0);
            let rem_label = if diff >= 3600 {
                format!("{}h {}m {}s", diff / 3600, (diff % 3600) / 60, diff % 60)
            } else if diff >= 60 {
                format!("{}m {}s", diff / 60, diff % 60)
            } else {
                format!("{}s", diff)
            };

            let del_btn: Element<'a, Message> = button(text("✕").size(10).color(danger))
                .on_press(Message::DeleteTimer(id))
                .padding([2, 6])
                .into();

            timers_col = timers_col.push(row![
                text(&e.text).size(12).color(fg).width(Length::Fill),
                text(rem_label).size(12).color(warning),
                del_btn,
            ].spacing(8).align_y(Alignment::Center));
        }

        let header = row![
            row![].width(Length::Fill),
            text("Set Alarm").size(title_size).bold().color(caution),
            row![].width(Length::Fill),
        ].width(Length::Fill).align_y(Alignment::Center);

        let mut body = column![
            header,
            column![
                text("Countdown").size(label_size).bold().color(caution).align_x(Alignment::Center).width(Length::Fill),
                cd_row,
            ].spacing(6).width(Length::Fill).align_x(Alignment::Center),
            column![
                text("Clock Time").size(label_size).bold().color(caution).align_x(Alignment::Center).width(Length::Fill),
                clk_row,
            ].spacing(6).width(Length::Fill).align_x(Alignment::Center).padding(Padding { top: 12.0, right: 0.0, bottom: 0.0, left: 0.0 }),
        ]
        .spacing(16)
        .padding(pad)
        .align_x(Alignment::Center)
        .width(Length::Fixed(self.w as f32))
        .height(Length::Fixed(self.h as f32));

        body = body.push(
            container(
                text("Enter to start/confirm  •  Esc to close")
                    .size(10)
                    .color(muted)
                    .align_x(Alignment::Center)
                    .width(Length::Fill)
            )
            .padding(Padding { top: 12.0, right: 0.0, bottom: 0.0, left: 0.0 })
        );

        if !self.entries.is_empty() {
            body = body.push(
                container(
                    column![
                        text("Active Timers").size(12).bold().color(muted),
                        scrollable(timers_col).height(Length::Fixed(60.0))
                    ].spacing(6)
                )
                .padding(Padding { top: 16.0, right: pad_x.max(20.0), bottom: 0.0, left: pad_x.max(20.0) })
            );
        }

        body.into()
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
    let mut w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
    let mut h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(s.tooltip_height_px);
    if std::env::var("MBS_SYS_TARGET").unwrap_or_default() == "ram" {
        w = w.max(540);
        h = h.max(480);
    }

    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(100.0);
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);
    
    let half_cap = (cap_w / 2.0) as i32;
    let slant_px = s.slant_width as i32;

    let (anchor, margin) = if grow_right {
        // Left side: left edge of tooltip binds to left edge of capsule
        let left_edge = (cx - half_cap).max(0);
        (Anchor::Top | Anchor::Left, (margin_top, 0, 0, left_edge))
    } else {
        // Right side: right edge of tooltip binds to right edge of capsule.
        // We subtract `slant_px` to account for the Parallelogram's inner slant.
        let right_edge = sw - cx - half_cap - slant_px;
        (Anchor::Top | Anchor::Right, (margin_top, right_edge.max(0), 0, 0))
    };

    let layer = LayerShellSettings {
        size: Some((w, h)), exclusive_zone: -1, anchor, layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        binded_output_name: output, margin, ..Default::default()
    };
    App::run(Settings { layer_settings: layer, ..Default::default() })
}