mod buttons;
mod calendar;
mod net_proc;
use buttons::*;
use net_proc::*;

use iced::widget::canvas::Canvas;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use std::time::Instant;

use crate::bar::unified::{BorgBackend, BorgStatus, PodmanBackend, PodmanStatus};
use crate::libraries::backend::system::ProcessItem;
use crate::libraries::backend::{ClockBackend, MprisBackend, MprisTrack, SystemCollector};
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::config::ShellSettings;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::libraries::style::TextExt;

use crate::overlays::tooltip_common::{octagon_row, slanted_row, KillButtonShape};

const W_START: f32 = 130.0;
const AUTO_MS: u128 = 15_000;

#[derive(Debug, Clone)]
pub enum InfoData {
    Calendar(crate::libraries::backend::CalendarViewData),
    Music(Option<MprisTrack>),
    Unified(
        PodmanStatus,
        BorgStatus,
        Vec<crate::bar::unified::UnifiedItemStatus>,
    ),
    Net(crate::libraries::backend::NetMetrics),
    Clock(Vec<crate::libraries::backend::WorldZoneColumn>),
    Custom,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Loaded(InfoData),
    Dismiss,
    MusicPrev,
    MusicNext,
    MusicPlayPause,
    CyclePlayer,
    MusicSeek(f64),
    MusicSetVolume(u8),
    MusicOpenFolder(Option<String>),
    MusicTrashConfirm,
    MusicTrashExecute(Option<String>),
    MusicSearchChanged(String),
    MusicSearchSubmit,
    MusicSearchResults(Vec<(String, String)>),
    MusicPlaySong(String),
    MusicSelectNext,
    MusicSelectPrev,
    MusicPlaySelected,
    MusicToggleSource(usize),
    PodmanRestart(String),
    UnifiedToggle(usize),
    SearchChanged(String),
    KillProcess(u32),
    Noop,
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
    mode: String,
    data: Option<InfoData>,
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
    search_query: String,
    trash_confirm: bool,
    anim_frame: u64,
    settings: ShellSettings,
    theme: ShellTheme,
    music_search_query: String,
    music_search_results: Vec<(String, String)>,
    music_selected_idx: usize,
    music_sources: crate::libraries::backend::music::MusicSources,
}

#[allow(dead_code)]
impl App {
    fn new_app() -> (Self, Task<Message>) {
        let mode = std::env::var("MBS_INFO_MODE").unwrap_or_else(|_| "custom".into());
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
            .unwrap_or(380);
        let s = ShellSettings::load();
        let h: u32 = std::env::var("MBS_TOOLTIP_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.tooltip_height_px);
        eprintln!("[info_tooltip-h] MBS_TOOLTIP_H = {}", h);
        let drop_ms = s.tooltip_drop_ms as f32;
        let expand_ms = s.tooltip_expand_ms as f32;
        let w_start: f32 = std::env::var("MBS_CAPSULE_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(W_START);
        let sl = std::env::var("MBS_SLANT_L")
            .ok()
            .map(|s| {
                if s == "R" {
                    SlantSide::Right
                } else {
                    SlantSide::Left
                }
            })
            .unwrap_or(SlantSide::Left);
        let sr = std::env::var("MBS_SLANT_R")
            .ok()
            .map(|s| {
                if s == "R" {
                    SlantSide::Right
                } else {
                    SlantSide::Left
                }
            })
            .unwrap_or(SlantSide::Left);

        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let app = Self {
            opened: Instant::now(),
            mode: mode.clone(),
            data: None,
            grow_right,
            w,
            w_start,
            slant_l: sl,
            slant_r: sr,
            h,
            drop_ms,
            expand_ms,
            closing_at: None,
            search_query: String::new(),
            trash_confirm: false,
            anim_frame: 0,
            settings,
            theme,
            music_search_query: String::new(),
            music_search_results: Vec::new(),
            music_selected_idx: 0,
            music_sources: crate::libraries::backend::music::MusicSources::load(),
        };
        let task = Task::perform(fetch_data(mode), Message::Loaded);
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

    fn namespace(&self) -> String {
        format!("mbs-info-tip-{}", self.mode)
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                self.anim_frame = self.anim_frame.wrapping_add(1);
                let elapsed_ms = self.opened.elapsed().as_millis() as u64;
                if std::env::var_os("MBS_TICK_DIAG").is_some() {
                    eprintln!("[tick] frame={} elapsed={}ms", self.anim_frame, elapsed_ms);
                }

                if let Some(t) = self.closing_at {
                    if t.elapsed().as_millis() > (self.drop_ms * 1.9) as u128 {
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
                    500
                };

                return Task::perform(
                    async move { tokio::time::sleep(std::time::Duration::from_millis(next_ms)).await },
                    |_| Message::Tick,
                );
            }
            Message::SearchChanged(q) => {
                self.search_query = q;
            }
            Message::KillProcess(pid) => {
                let target = self.mode.clone();
                return Task::batch([
                    Task::perform(SystemCollector::kill_process(pid), |_| Message::Noop),
                    Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            match target.as_str() {
                                "net" => {
                                    let mut sc = SystemCollector::new();
                                    let _ = sc.update_net();
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                    InfoData::Net(sc.update_net())
                                }
                                _ => InfoData::Custom,
                            }
                        },
                        Message::Loaded,
                    ),
                ]);
            }
            Message::Noop => {}
            Message::Loaded(d) => {
                self.data = Some(d);
            }
            Message::Dismiss => {
                self.closing_at = Some(Instant::now());
            }
            Message::MusicPrev => {
                // Finding 74: route through the bar's backend selector
                // (MPD if playing, else MPRIS) instead of playerctl,
                // which would only control the active MPRIS player.
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe)
                        .arg("--music-previous")
                        .status();
                }
            }
            Message::MusicNext => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--music-next").status();
                }
            }
            Message::MusicPlayPause => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe)
                        .arg("--music-play-pause")
                        .status();
                }
            }
            Message::CyclePlayer => {
                // Cycle to next player (or pause current), then re-fetch
                // so the tooltip shows the new state. 500ms gives the
                // bar time to complete the async cycle.
                return Task::perform(
                    async {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = tokio::process::Command::new(exe)
                                .arg("--music-cycle")
                                .status()
                                .await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        fetch_data("music".to_string()).await
                    },
                    Message::Loaded,
                );
            }
            Message::MusicSeek(secs) => {
                return Task::perform(
                    async move {
                        let _ = crate::libraries::backend::MprisBackend::set_position(secs).await;
                    },
                    |_| Message::Noop,
                );
            }
            Message::MusicSetVolume(vol) => {
                return Task::perform(
                    async move {
                        let _ = crate::libraries::backend::MprisBackend::set_volume(vol).await;
                    },
                    |_| Message::Noop,
                );
            }
            Message::MusicOpenFolder(url) => {
                return Task::perform(
                    async move {
                        crate::libraries::backend::MprisBackend::open_track_folder(url.as_deref())
                            .await;
                    },
                    |_| Message::Noop,
                );
            }
            Message::PodmanRestart(cname) => {
                return Task::perform(
                    async move {
                        crate::bar::unified::PodmanBackend::restart(&cname).await;
                    },
                    |_| Message::Dismiss,
                );
            }
            Message::UnifiedToggle(idx) => {
                let s = ShellSettings::load();
                if let Some(item) = s.unified_items.get(idx).cloned() {
                    let mode = self.mode.clone();
                    return Task::perform(
                        async move {
                            if let Err(e) = crate::bar::unified::checks::toggle_item(&item).await {
                                eprintln!("[unified] toggle '{}' failed: {}", item.name, e);
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            fetch_data(mode).await
                        },
                        Message::Loaded,
                    );
                }
            }
            Message::MusicToggleSource(idx) => {
                match idx {
                    0 => self.music_sources.local = !self.music_sources.local,
                    1 => self.music_sources.spotify = !self.music_sources.spotify,
                    2 => self.music_sources.browser = !self.music_sources.browser,
                    _ => {}
                }
                self.music_sources.save();
            }
            Message::MusicSearchChanged(q) => {
                self.music_search_query = q.clone();
                self.music_selected_idx = 0;
                if q.is_empty() {
                    self.music_search_results.clear();
                    return Task::none();
                }
                return Task::perform(
                    async move { crate::libraries::backend::MpdBackend::search_songs(&q).await },
                    Message::MusicSearchResults,
                );
            }
            Message::MusicSearchSubmit => {
                return Task::done(Message::MusicPlaySelected);
            }
            Message::MusicSearchResults(res) => {
                self.music_search_results = res;
                self.music_selected_idx = 0;
            }
            Message::MusicSelectNext => {
                if !self.music_search_results.is_empty() {
                    self.music_selected_idx =
                        (self.music_selected_idx + 1).min(self.music_search_results.len() - 1);
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("music_scroll"),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: (self.music_selected_idx as f32 * 32.0 - 60.0).max(0.0),
                        },
                    )
                    .map(|_: ()| Message::Tick);
                }
            }
            Message::MusicSelectPrev => {
                if !self.music_search_results.is_empty() {
                    self.music_selected_idx = self.music_selected_idx.saturating_sub(1);
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("music_scroll"),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: (self.music_selected_idx as f32 * 24.0 - 40.0).max(0.0),
                        },
                    )
                    .map(|_: ()| Message::Tick);
                }
            }
            Message::MusicPlaySelected => {
                if self
                    .music_search_results
                    .get(self.music_selected_idx)
                    .is_some()
                {
                    let urls: Vec<String> = self
                        .music_search_results
                        .iter()
                        .map(|(_, u)| u.clone())
                        .collect();
                    let idx = self.music_selected_idx;
                    return Task::perform(
                        async move {
                            crate::libraries::backend::MpdBackend::play_queue(&urls, idx).await;
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            fetch_data("music".to_string()).await
                        },
                        |info| Message::Loaded(info),
                    );
                }
            }
            Message::MusicPlaySong(url) => {
                if let Some(idx) = self
                    .music_search_results
                    .iter()
                    .position(|(_, u)| u == &url)
                {
                    let urls: Vec<String> = self
                        .music_search_results
                        .iter()
                        .map(|(_, u)| u.clone())
                        .collect();
                    return Task::perform(
                        async move {
                            crate::libraries::backend::MpdBackend::play_queue(&urls, idx).await;
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            fetch_data("music".to_string()).await
                        },
                        |info| Message::Loaded(info),
                    );
                }
            }
            Message::MusicTrashConfirm => {
                self.trash_confirm = true;
            }
            Message::MusicTrashExecute(url) => {
                self.trash_confirm = false;
                return Task::perform(
                    async move {
                        crate::libraries::backend::MprisBackend::trash_current_track(
                            url.as_deref(),
                        )
                        .await;
                    },
                    |_| Message::Noop,
                );
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = &self.settings;
        let theme = &self.theme;
        let _content_h = match &self.data {
            Some(InfoData::Calendar(c)) => {
                let total_cells = c.first_day_offset as u32 + c.days_in_month;
                let weeks = total_cells.div_ceil(7);
                40.0 + 36.0 + 24.0 + (weeks as f32) * 50.0 + 20.0 + settings.bar_height as f32
            }
            Some(InfoData::Music(_)) => 520.0 + settings.bar_height as f32,
            Some(InfoData::Unified(..)) => 400.0 + settings.bar_height as f32,
            Some(InfoData::Net(_)) => 450.0 + settings.bar_height as f32,
            Some(InfoData::Clock(_)) => 320.0 + settings.bar_height as f32,
            Some(InfoData::Custom) => 300.0 + settings.bar_height as f32,
            None => self.h as f32,
        };
        let effective_h = self.h as f32;

        let is_clock = self.mode == "clock";
        // For the keystone (clock) we pass slant_base=0. The octagon draw path
        // doesn't use `slant` at all — it uses `cut`. Passing 0 forces
        // compute() to lock width at the capsule's width during the drop phase,
        // then expand symmetrically during the expand phase (instead of growing
        // width by slant-coupling during drop like the parallelogram does).
        let slant_base = if is_clock { 0.0 } else { theme.slant_width };

        let anim = if let Some(t) = self.closing_at {
            crate::overlays::tooltip_engine::compute_close(
                t.elapsed().as_millis() as f32,
                self.w_start,
                self.w as f32,
                effective_h,
                slant_base,
                settings.bar_height as f32,
                self.drop_ms,
            )
        } else {
            crate::overlays::tooltip_engine::compute(
                self.opened.elapsed().as_millis() as f32,
                self.w_start,
                self.w as f32,
                effective_h,
                slant_base,
                settings.bar_height as f32,
                self.drop_ms,
                self.expand_ms,
            )
        };

        // Clock gets an octagon; every other info tooltip gets the parallelogram.
        let bg_view: Element<'_, Message> = if is_clock {
            let oct = crate::overlays::tooltip_engine::OctagonBg {
                w: anim.w,
                h: anim.h,
                cut: anim.cut,
                background: theme.background,
                border_color: theme.text,
                border_width: theme.control_border_width,
                frame: self.opened.elapsed().as_millis() as u64,
            };
            oct.view()
        } else {
            let slant_h = theme.slant_width * (anim.h / settings.bar_height.max(1) as f32);
            let para = crate::overlays::tooltip_engine::ParallelogramBg {
                w: anim.w,
                h: anim.h,
                slant: slant_h,
                slant_right: self.slant_l == SlantSide::Right,
                background: theme.background,
                border_color: theme.text,
                border_width: theme.control_border_width,
                frame: self.opened.elapsed().as_millis() as u64,
            };
            para.view()
        };

        // Clock: content and outer box both centered. Other modes preserve
        // the existing left/right grow behavior.
        let align = if is_clock {
            iced::alignment::Horizontal::Center
        } else if self.grow_right {
            iced::alignment::Horizontal::Left
        } else {
            iced::alignment::Horizontal::Right
        };
        let content: Element<'_, Message> =
            self.build_content(theme, settings, anim.opacity, effective_h);
        let clipped = crate::overlays::tooltip_engine::clip_content(content, anim.w, anim.h, align);
        let stacked = iced::widget::container(Stack::new().push(bg_view).push(clipped))
            .width(Length::Fixed(anim.w))
            .height(Length::Fixed(anim.h))
            .clip(true);
        crate::overlays::tooltip_engine::align_to_side(stacked.into(), align)
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let music_search_nav = if self.mode == "music" && !self.music_search_results.is_empty() {
            iced::event::listen_with(|event, _status, _window| {
                if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event
                {
                    match key {
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                            Some(Message::MusicSelectNext)
                        }
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                            Some(Message::MusicSelectPrev)
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            })
        } else {
            iced::Subscription::none()
        };

        let esc_sub = iced::event::listen_with(|event, _status, _window| {
            let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) = event
            else {
                return None;
            };
            Some(Message::Dismiss)
        });

        iced::Subscription::batch(vec![music_search_nav, esc_sub])
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
        total_h: f32,
    ) -> Element<'a, Message> {
        let a = move |c: Color| Color {
            a: c.a * alpha,
            ..c
        };
        let text_col = a(t.text);
        let muted = a(t.muted);
        let danger = a(t.danger);
        let warning = a(t.warning);
        let half_tt = settings.control_border_width * 0.5;
        let success = a(t.success);

        let fs = settings.tooltip_font_size;
        let bar_h = settings.bar_height.max(1) as f32;
        let slant_h = t.slant_width * (total_h / bar_h);
        let slant_right = self.slant_l == SlantSide::Right;
        let tooltip_slope = slant_h / (total_h - 2.0 * half_tt);
        let safe = 16.0;
        let vpad = 14.0;
        let gap = 8.0;
        let w_f = self.w as f32;

        let mut y = vpad;
        let mut col = column![].spacing(gap).width(Length::Fill);

        let mk_close = || -> Element<'a, Message> {
            // Removed per user request — press Escape to close.
            iced::widget::container(iced::widget::Space::new())
                .width(Length::Fixed(0.0))
                .height(Length::Fixed(0.0))
                .into()
        };

        if let Some(d) = &self.data {
            match d {
                InfoData::Calendar(c) => {
                    col = self.render_calendar(c, col, &mut y, t, settings, total_h, slant_h, slant_right, safe, w_f, alpha);
                }
                InfoData::Music(track_opt) => {
                    // Extra horizontal padding so widgets don't clip against the diagonal edges
                    let safe = safe + 20.0;

                    let (title, artist, _album, playing, pos, len, url, vol) = match track_opt {
                        Some(tr) => (
                            tr.title.clone(),
                            tr.artist.clone(),
                            tr.album.clone(),
                            tr.is_playing,
                            tr.position_sec,
                            tr.length_sec,
                            tr.url.clone(),
                            tr.volume_pct,
                        ),
                        None => (
                            "No Track".into(),
                            "No Artist".into(),
                            "".into(),
                            false,
                            0.0,
                            0.0,
                            None,
                            100,
                        ),
                    };

                    let pos_str = format!("{}:{:02}", (pos as u32) / 60, (pos as u32) % 60);
                    let len_str = format!("{}:{:02}", (len as u32) / 60, (len as u32) % 60);

                    let header_h = fs as f32 + 16.0;
                    let header = row![
                        text("MUSIC")
                            .size((fs.max(14)) as f32)
                            .bold()
                            .color(text_col),
                        iced::widget::space::horizontal(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        header.into(),
                        y + header_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        header_h,
                    ));
                    y += header_h + gap;

                    // Track card
                    let card_h = 80.0;
                    let card_bg_col = a(t.background);
                    let card_border_col = a(t.text);
                    let card_text_bold = a(t.text);
                    let card_text_muted = Color {
                        a: a(t.text).a * 0.8,
                        ..a(t.text)
                    };

                    let trunc_title = if title.chars().count() > 24 {
                        let s: String = title.chars().take(21).collect();
                        format!("{}...", s)
                    } else {
                        title.clone()
                    };

                    let card_el = column![
                        text(trunc_title)
                            .size((fs + 2) as f32)
                            .bold()
                            .color(card_text_bold)
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                            .wrapping(iced::widget::text::Wrapping::None),
                        text(artist)
                            .size((fs) as f32)
                            .color(card_text_muted)
                            .width(Length::Fill)
                            .align_x(Alignment::Center)
                            .wrapping(iced::widget::text::Wrapping::None),
                    ]
                    .spacing(4);
                    let card_shape = ShapeBox::new()
                        .shape(ShapeType::Hexagon { cut: 16 })
                        .background(card_bg_col)
                        .border(card_border_col, 1.5)
                        .size(Length::Fill, Length::Fill);
                    let card_layer = Stack::new().push(card_shape.view()).push(
                        container(card_el)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .center_x(Length::Fill)
                            .center_y(Length::Fill),
                    );
                    col = col.push(slanted_row(
                        card_layer.into(),
                        y + card_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        card_h,
                    ));
                    y += card_h + gap * 2.0;

                    // Seek bar
                    let seek_h = 20.0;
                    let slider_fg = a(t.text);
                    let slider_bg = a(t.muted);
                    let seek_bar = row![
                        text(pos_str).size((fs - 2) as f32).color(text_col),
                        container(
                            iced::widget::slider(0.0..=len.max(1.0), pos, Message::MusicSeek)
                                .style(move |_t, _s| iced::widget::slider::Style {
                                    rail: iced::widget::slider::Rail {
                                        backgrounds: (
                                            iced::Background::Color(slider_fg),
                                            iced::Background::Color(slider_bg)
                                        ),
                                        width: 2.0,
                                        border: iced::Border::default(),
                                    },
                                    handle: iced::widget::slider::Handle {
                                        shape: iced::widget::slider::HandleShape::Rectangle {
                                            width: 10,
                                            border_radius: 0.0.into()
                                        },
                                        background: iced::Background::Color(slider_fg),
                                        border_color: slider_fg,
                                        border_width: 0.0,
                                    },
                                })
                                .width(Length::Fill)
                        )
                        .width(Length::Fill)
                        .padding([0, 8]),
                        text(len_str).size((fs - 2) as f32).color(text_col),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        seek_bar.into(),
                        y + seek_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        seek_h,
                    ));
                    y += seek_h + gap;

                    // Volume bar
                    let vol_h = 20.0;
                    let vol_bar = row![
                        text("VOL").size((fs - 2) as f32).bold().color(text_col),
                        container(
                            iced::widget::slider(0..=100, vol, Message::MusicSetVolume)
                                .style(move |_t, _s| iced::widget::slider::Style {
                                    rail: iced::widget::slider::Rail {
                                        backgrounds: (
                                            iced::Background::Color(slider_fg),
                                            iced::Background::Color(slider_bg)
                                        ),
                                        width: 2.0,
                                        border: iced::Border::default(),
                                    },
                                    handle: iced::widget::slider::Handle {
                                        shape: iced::widget::slider::HandleShape::Rectangle {
                                            width: 10,
                                            border_radius: 0.0.into()
                                        },
                                        background: iced::Background::Color(slider_fg),
                                        border_color: slider_fg,
                                        border_width: 0.0,
                                    },
                                })
                                .width(Length::Fill)
                        )
                        .width(Length::Fill)
                        .padding([0, 8]),
                        text(format!("{}%", vol))
                            .size((fs - 2) as f32)
                            .color(text_col),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        vol_bar.into(),
                        y + vol_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        vol_h,
                    ));
                    y += vol_h + gap * 2.0;

                    // Playback controls
                    let ctrl_h = 32.0;
                    let btn_fs = fs - 2;
                    let text_col_l = a(t.text);
                    let bg_col_l = a(t.background);

                    let prev_btn = make_slanted_button(
                        "< Prev".into(),
                        Message::MusicPrev,
                        bg_col_l,
                        text_col_l,
                        text_col_l,
                        6.0,
                        btn_fs,
                    );
                    let play_btn = if playing {
                        make_slanted_button(
                            "Pause".into(),
                            Message::MusicPlayPause,
                            text_col_l,
                            bg_col_l,
                            text_col_l,
                            6.0,
                            btn_fs,
                        )
                    } else {
                        make_slanted_button(
                            "Play".into(),
                            Message::MusicPlayPause,
                            bg_col_l,
                            text_col_l,
                            text_col_l,
                            6.0,
                            btn_fs,
                        )
                    };
                    let next_btn = make_slanted_button(
                        "Next >".into(),
                        Message::MusicNext,
                        bg_col_l,
                        text_col_l,
                        text_col_l,
                        6.0,
                        btn_fs,
                    );
                    let cycle_btn = make_slanted_button(
                        "Cycle".into(),
                        Message::CyclePlayer,
                        bg_col_l,
                        text_col_l,
                        text_col_l,
                        6.0,
                        btn_fs,
                    );
                    let ctrl_row: Element<'_, Message> =
                        row![prev_btn, play_btn, next_btn, cycle_btn]
                            .spacing(6)
                            .width(Length::Fill)
                            .height(Length::Fixed(ctrl_h))
                            .into();
                    col = col.push(slanted_row(
                        ctrl_row,
                        y + ctrl_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        ctrl_h,
                    ));
                    y += ctrl_h + gap * 2.0;

                    // Actions (folder / trash)
                    let act_h = 28.0;
                    let folder_btn = make_slanted_button(
                        "Folder".into(),
                        Message::MusicOpenFolder(url.clone()),
                        bg_col_l,
                        text_col_l,
                        text_col_l,
                        4.0,
                        btn_fs,
                    );
                    let trash_btn = if self.trash_confirm {
                        make_slanted_button(
                            "Confirm?".into(),
                            Message::MusicTrashExecute(url.clone()),
                            bg_col_l,
                            danger,
                            danger,
                            4.0,
                            btn_fs,
                        )
                    } else {
                        make_slanted_button(
                            "Trash".into(),
                            Message::MusicTrashConfirm,
                            bg_col_l,
                            text_col_l,
                            text_col_l,
                            4.0,
                            btn_fs,
                        )
                    };
                    let act_row: Element<'_, Message> = row![folder_btn, trash_btn]
                        .spacing(8)
                        .width(Length::Fill)
                        .height(Length::Fixed(act_h))
                        .into();
                    col = col.push(slanted_row(
                        act_row,
                        y + act_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        act_h,
                    ));
                    y += act_h + gap * 2.0;
                    // ── Search UI ──
                    let search_h = 32.0;
                    let search_input = text_input("Search MPD...", &self.music_search_query)
                        .on_input(Message::MusicSearchChanged)
                        .on_submit(Message::MusicSearchSubmit)
                        .padding(6)
                        .size((fs - 2) as f32)
                        .style(move |_t, _s| iced::widget::text_input::Style {
                            background: iced::Background::Color(bg_col_l),
                            border: iced::Border {
                                color: text_col_l,
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            icon: text_col_l,
                            placeholder: text_col_l,
                            value: text_col_l,
                            selection: text_col_l,
                        });

                    col = col.push(slanted_row(
                        search_input.into(),
                        y + search_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        search_h,
                    ));
                    y += search_h + gap;

                    if !self.music_search_results.is_empty() {
                        let mut results_col = column![].spacing(2);
                        for (i, (title, url)) in self.music_search_results.iter().enumerate() {
                            let url_clone = url.clone();
                            let is_sel = i == self.music_selected_idx;
                            let bg = if is_sel { text_col_l } else { bg_col_l };
                            let fg = if is_sel { bg_col_l } else { text_col_l };

                            let btn = button(text(title).size((fs - 3) as f32).color(fg))
                                .on_press(Message::MusicPlaySong(url_clone))
                                .padding(4)
                                .width(Length::Fill)
                                .style(move |_t, _s| button::Style {
                                    background: Some(iced::Background::Color(bg)),
                                    border: iced::Border {
                                        color: text_col_l,
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                });
                            results_col = results_col.push(btn);
                        }
                        let scroll_results = scrollable(results_col)
                            .id(iced::widget::Id::new("music_scroll"))
                            .height(Length::Fixed(100.0));

                        col = col.push(slanted_row(
                            scroll_results.into(),
                            y + 50.0,
                            total_h,
                            slant_h,
                            slant_right,
                            safe,
                            w_f,
                            100.0,
                        ));
                        y += 100.0 + gap;
                    }

                    // Sources row
                    let src_h = 36.0;
                    let src_btn = |label: &'static str, idx: usize| -> Element<'_, Message> {
                        let active = match idx {
                            0 => self.music_sources.local,
                            1 => self.music_sources.spotify,
                            2 => self.music_sources.browser,
                            _ => false,
                        };
                        let bg = if active { a(t.text) } else { a(t.background) };
                        let fg = if active { a(t.background) } else { a(t.text) };
                        let border = a(t.text);
                        button(
                            container(text(label).size((fs - 1) as f32).bold().color(fg))
                                .center_y(Length::Fill),
                        )
                        .on_press(Message::MusicToggleSource(idx))
                        .padding([0, 16])
                        .height(Length::Fill)
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(bg)),
                            border: iced::Border {
                                color: border,
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            text_color: fg,
                            ..Default::default()
                        })
                        .into()
                    };

                    let src_row = row![
                        text("Sources:")
                            .size((fs - 1) as f32)
                            .bold()
                            .color(text_col),
                        iced::widget::space::horizontal(),
                        src_btn("Local", 0),
                        src_btn("Spotify", 1),
                        src_btn("Browser", 2),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        src_row.into(),
                        y + src_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        src_h,
                    ));
                }
                InfoData::Unified(podman, borg, custom_items) => {
                    let header_h = fs as f32 + 16.0;
                    let header = row![
                        text("SYSTEM")
                            .size((fs.max(14)) as f32)
                            .bold()
                            .color(text_col),
                        iced::widget::space::horizontal(),
                        mk_close(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        header.into(),
                        y + header_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        header_h,
                    ));
                    y += header_h + gap;

                    // Containers
                    if !podman.containers.is_empty() {
                        let title_h = fs as f32 + 4.0;
                        let title_el: Element<'_, Message> = text("Containers")
                            .size((fs - 2) as f32)
                            .bold()
                            .color(muted)
                            .into();
                        col = col.push(slanted_row(
                            title_el,
                            y + title_h * 0.5,
                            total_h,
                            slant_h,
                            slant_right,
                            safe,
                            w_f,
                            title_h,
                        ));
                        y += title_h + gap;

                        for c in podman.containers.iter().take(6) {
                            let row_h = fs as f32 + 12.0;
                            let dot = if c.is_running { "●" } else { "○" };
                            let dot_color = if c.is_running { success } else { muted };
                            let cname = c.name.clone();

                            let restart_btn = make_slanted_button(
                                "↻".into(),
                                Message::PodmanRestart(cname.clone()),
                                a(t.background),
                                a(t.text),
                                a(t.text),
                                4.0,
                                fs - 3,
                            );

                            let row_el = row![
                                text(dot).size((fs) as f32).color(dot_color),
                                text(&c.name)
                                    .size((fs - 1) as f32)
                                    .color(text_col)
                                    .width(Length::Fill),
                                text(&c.status).size((fs - 3) as f32).color(muted),
                                container(restart_btn)
                                    .width(Length::Fixed(28.0))
                                    .height(Length::Fixed(row_h - 2.0)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .width(Length::Fill);

                            col = col.push(slanted_row(
                                row_el.into(),
                                y + row_h * 0.5,
                                total_h,
                                slant_h,
                                slant_right,
                                safe,
                                w_f,
                                row_h,
                            ));
                            y += row_h + gap;
                        }
                    }

                    // Custom items
                    if !custom_items.is_empty() {
                        let title_h = fs as f32 + 4.0;
                        let title_el: Element<'_, Message> = text("Custom Items")
                            .size((fs - 2) as f32)
                            .bold()
                            .color(muted)
                            .into();
                        col = col.push(slanted_row(
                            title_el,
                            y + title_h * 0.5,
                            total_h,
                            slant_h,
                            slant_right,
                            safe,
                            w_f,
                            title_h,
                        ));
                        y += title_h + gap;

                        for (idx, item) in custom_items.iter().enumerate() {
                            let row_h = fs as f32 + 12.0;
                            let dot = if item.is_ok { "●" } else { "○" };
                            let dot_color = if item.is_ok { success } else { danger };

                            let action_el: Element<'_, Message> = if item.has_toggle {
                                make_slanted_button(
                                    "↻".into(),
                                    Message::UnifiedToggle(idx),
                                    a(t.background),
                                    a(t.text),
                                    a(t.text),
                                    4.0,
                                    fs - 3,
                                )
                            } else {
                                container(text("")).width(Length::Fixed(28.0)).into()
                            };

                            let msg_str = if item.message.is_empty() {
                                String::new()
                            } else {
                                let truncated: String = item.message.chars().take(18).collect();
                                format!(" ({})", truncated)
                            };

                            let row_el = row![
                                text(dot).size((fs) as f32).color(dot_color),
                                text(&item.name)
                                    .size((fs - 1) as f32)
                                    .color(text_col)
                                    .width(Length::Fill),
                                text(msg_str).size((fs - 3) as f32).color(muted),
                                container(action_el)
                                    .width(Length::Fixed(28.0))
                                    .height(Length::Fixed(row_h - 2.0)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .width(Length::Fill);

                            col = col.push(slanted_row(
                                row_el.into(),
                                y + row_h * 0.5,
                                total_h,
                                slant_h,
                                slant_right,
                                safe,
                                w_f,
                                row_h,
                            ));
                            y += row_h + gap;
                        }
                    }

                    if podman.containers.is_empty() && custom_items.is_empty() {
                        let msg_h = fs as f32 + 8.0;
                        let msg: Element<'_, Message> =
                            text("(no containers or custom items — add via Settings)")
                                .size((fs - 2) as f32)
                                .color(muted)
                                .into();
                        col = col.push(slanted_row(
                            msg,
                            y + msg_h * 0.5,
                            total_h,
                            slant_h,
                            slant_right,
                            safe,
                            w_f,
                            msg_h,
                        ));
                        y += msg_h + gap;
                    }

                    // Borg — three states: Failed (red) > Running (yellow) > Normal (green)
                    let title_h = fs as f32 + 4.0;
                    let title_el: Element<'_, Message> = text("Borg")
                        .size((fs - 2) as f32)
                        .bold()
                        .color(muted)
                        .into();
                    col = col.push(slanted_row(
                        title_el,
                        y + title_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        title_h,
                    ));
                    y += title_h + gap;

                    let borg_h = fs as f32 + 12.0;
                    let (borg_dot_color, borg_label) = if borg.is_failed {
                        let msg = borg
                            .details
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "Failed".to_string());
                        (danger, msg)
                    } else if borg.is_active {
                        let status = if borg.percent > 0 {
                            format!("Running ({}%)", borg.percent)
                        } else {
                            "Running".to_string()
                        };
                        (warning, status)
                    } else if borg.is_mounted {
                        (success, "Mounted".to_string())
                    } else {
                        (success, "Idle (Synced)".to_string())
                    };

                    let borg_el = row![
                        text("●").size((fs) as f32).color(borg_dot_color),
                        text(borg_label)
                            .size((fs - 1) as f32)
                            .color(text_col)
                            .width(Length::Fill),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();
                    col = col.push(slanted_row(
                        borg_el,
                        y + borg_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        borg_h,
                    ));
                }
                InfoData::Net(net) => {
                    let header = row![
                        text("NETWORK").size((fs) as f32).bold().color(text_col),
                        iced::widget::space::horizontal(),
                        mk_close(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        header.into(),
                        y + 16.0,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        32.0,
                    ));
                    y += 32.0 + gap;

                    let stats_h = fs as f32 + 8.0;
                    let stats_el = row![
                        text(if net.iface.is_empty() {
                            "—"
                        } else {
                            &net.iface
                        })
                        .size((fs) as f32)
                        .color(text_col),
                        text(" ▼ ")
                            .size((fs.saturating_sub(2)) as f32)
                            .color(success),
                        text(&net.down_str).size((fs) as f32).bold().color(text_col),
                        text(" ▲ ")
                            .size((fs.saturating_sub(2)) as f32)
                            .color(success),
                        text(&net.up_str).size((fs) as f32).bold().color(text_col),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();
                    col = col.push(slanted_row(
                        stats_el,
                        y + stats_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        stats_h,
                    ));
                    y += stats_h + gap;

                    // Search Bar
                    let search_h = 40.0;
                    let y_mid_search = y + search_h * 0.5;
                    let frac_s = (y_mid_search / total_h.max(1.0)).clamp(0.0, 1.0);
                    let (lp_s, rp_s) = if slant_right {
                        (slant_h * (1.0 - frac_s) + safe, slant_h * frac_s + safe)
                    } else {
                        (slant_h * frac_s + safe, slant_h * (1.0 - frac_s) + safe)
                    };
                    let search_w = (w_f - lp_s - rp_s).max(100.0);
                    let search_border = settings.control_border_width;
                    let half_s = search_border * 0.5;
                    let search_slant = tooltip_slope * (search_h - 2.0 * half_s);

                    let search_bg = crate::overlays::tooltip_engine::ParallelogramBg {
                        w: search_w,
                        h: search_h,
                        slant: search_slant,
                        slant_right,
                        background: a(t.surface_alt),
                        border_color: a(t.text),
                        border_width: search_border,
                        frame: self.opened.elapsed().as_millis() as u64,
                    };

                    let input_muted = Color { a: 1.0, ..t.muted };
                    let input_text = Color { a: 1.0, ..t.text };
                    let input_focus = Color { a: 1.0, ..t.focus };
                    let search_input = text_input("Search programs...", &self.search_query)
                        .on_input(Message::SearchChanged)
                        .padding(Padding {
                            top: 8.0,
                            bottom: 8.0,
                            left: search_slant + 8.0,
                            right: search_slant + 8.0,
                        })
                        .size((fs) as f32)
                        .width(Length::Fill)
                        .style(move |_t, _s| iced::widget::text_input::Style {
                            background: iced::Background::Color(Color::TRANSPARENT),
                            border: iced::Border::default(),
                            icon: input_muted,
                            placeholder: input_muted,
                            value: input_text,
                            selection: input_focus,
                        });
                    let search_layer: Element<'a, Message> = Stack::new()
                        .push(search_bg.view())
                        .push(
                            container(search_input)
                                .width(Length::Fixed(search_w))
                                .height(Length::Fixed(search_h))
                                .center_x(Length::Fill)
                                .center_y(Length::Fill),
                        )
                        .width(Length::Fixed(search_w))
                        .height(Length::Fixed(search_h))
                        .into();

                    col = col.push(slanted_row(
                        search_layer,
                        y_mid_search,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        search_h,
                    ));
                    y += search_h + gap;

                    // Process List
                    let proc_h = fs as f32 + 10.0;
                    let proc_gap = 2.0;
                    let mut procs_col = column![].spacing(proc_gap).width(Length::Fill);
                    let q = self.search_query.to_lowercase();
                    let items: Vec<&ProcessItem> = net
                        .top_procs
                        .iter()
                        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                        .collect();
                    for &p in items.iter().take(10) {
                        let inner_w = w_f
                            - (slant_h * (1.0 - (y + proc_h * 0.5) / total_h.max(1.0)) + safe)
                            - (slant_h * ((y + proc_h * 0.5) / total_h.max(1.0)) + safe);
                        let proc_row = render_net_proc_row(
                            p,
                            t,
                            fs,
                            alpha,
                            inner_w,
                            proc_h,
                            slant_right,
                            tooltip_slope,
                        );
                        procs_col = procs_col.push(slanted_row(
                            proc_row,
                            y + proc_h * 0.5,
                            total_h,
                            slant_h,
                            slant_right,
                            safe,
                            w_f,
                            proc_h,
                        ));
                        y += proc_h + proc_gap;
                    }
                    if !items.is_empty() {
                        col = col.push(procs_col);
                    } else {
                        let empty_el: Element<'a, Message> = container(
                            text(if net.top_procs.is_empty() {
                                "(no active network procs)"
                            } else {
                                "(no match)"
                            })
                            .size((fs.saturating_sub(4).max(9)) as f32)
                            .color(muted)
                            .wrapping(iced::widget::text::Wrapping::None),
                        )
                        .into();
                        col = col.push(slanted_row(
                            empty_el,
                            y + 10.0,
                            total_h,
                            slant_h,
                            slant_right,
                            safe,
                            w_f,
                            20.0,
                        ));
                    }
                }
                InfoData::Clock(cols) => {
                    let header_h = fs as f32 + 16.0;
                    let header = row![
                        text("WORLD CLOCK")
                            .size((fs.max(14)) as f32)
                            .bold()
                            .color(text_col),
                        iced::widget::space::horizontal(),
                        mk_close(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    let cut_f = (w_f.min(total_h) * 0.15).max(0.0);
                    col = col.push(octagon_row(
                        header.into(),
                        y + header_h * 0.5,
                        total_h,
                        cut_f,
                        safe,
                        w_f,
                        header_h,
                    ));
                    y += header_h + gap;

                    let mut grid_row = row![].spacing(12).width(Length::Fill);
                    for c in cols {
                        let mut col_widget = column![].spacing(6).width(Length::FillPortion(1));
                        col_widget = col_widget.push(
                            text(c.title)
                                .size((fs - 1) as f32)
                                .bold()
                                .color(muted)
                                .width(Length::Fill)
                                .align_x(Alignment::Center),
                        );
                        for (name, code, time, is_me) in &c.zones {
                            let t_color = if *is_me { success } else { text_col };
                            let is_me_val = *is_me;
                            let bg = if is_me_val {
                                Some(iced::Background::Color(a(t.surface_alt)))
                            } else {
                                None
                            };
                            let border_c = if is_me_val {
                                success
                            } else {
                                Color::TRANSPARENT
                            };

                            let item = container(
                                row![
                                    column![
                                        text(*name)
                                            .size((fs - 2) as f32)
                                            .bold()
                                            .color(t_color)
                                            .wrapping(iced::widget::text::Wrapping::None),
                                        text(*code)
                                            .size((fs - 3) as f32)
                                            .color(muted)
                                            .wrapping(iced::widget::text::Wrapping::None),
                                    ]
                                    .spacing(2)
                                    .width(Length::Fill),
                                    text(time)
                                        .size((fs + 1) as f32)
                                        .bold()
                                        .color(t_color)
                                        .wrapping(iced::widget::text::Wrapping::None),
                                ]
                                .align_y(Alignment::Center),
                            )
                            .padding([4, 8])
                            .width(Length::Fill)
                            .style(move |_t| container::Style {
                                background: bg,
                                border: iced::Border {
                                    color: border_c,
                                    width: if is_me_val { 1.0 } else { 0.0 },
                                    radius: 4.0.into(),
                                },
                                ..Default::default()
                            });
                            col_widget = col_widget.push(item);
                        }
                        grid_row = grid_row.push(col_widget);
                    }

                    let grid_h = 5.0 * 48.0;
                    col = col.push(octagon_row(
                        grid_row.into(),
                        y + grid_h * 0.5,
                        total_h,
                        cut_f,
                        safe,
                        w_f,
                        grid_h,
                    ));
                }
                InfoData::Custom => {
                    let header = row![
                        text("CUSTOM MODULE")
                            .size((fs) as f32)
                            .bold()
                            .color(text_col),
                        iced::widget::space::horizontal(),
                        mk_close(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill);
                    col = col.push(slanted_row(
                        header.into(),
                        y + 16.0,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        32.0,
                    ));
                    y += 32.0 + gap;
                    let msg = text("Configure in Settings → Bar Layout")
                        .size((fs) as f32)
                        .color(muted)
                        .into();
                    col = col.push(slanted_row(
                        msg,
                        y + 10.0,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        20.0,
                    ));
                }
            }
        } else {
            let header = row![
                text("LOADING...").size((fs) as f32).color(muted),
                iced::widget::space::horizontal(),
                mk_close(),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill);
            col = col.push(slanted_row(
                header.into(),
                y + 16.0,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                32.0,
            ));
        }

        container(col)
            .padding(Padding {
                top: vpad,
                right: 0.0,
                bottom: vpad,
                left: 0.0,
            })
            .width(Length::Fixed(self.w as f32))
            .height(Length::Fixed(self.h as f32))
            .clip(true)
            .into()
    }
}

/// Octagon-aware row insets: content hugs the middle columns of a widened
/// octagon, but the top and bottom rows are pulled inward proportionally
/// to how far they sit inside the diagonal corner cut. Mirrors slanted_row,
/// but for a 4-cut octagon instead of a 2-cut parallelogram.

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    let s = ShellSettings::load();
    let w: u32 = std::env::var("MBS_TOOLTIP_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(380);
    let h: u32 = std::env::var("MBS_TOOLTIP_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(s.tooltip_height_px);
    eprintln!("[info_tooltip-h] MBS_TOOLTIP_H = {}", h);

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

    // Finding 76: per-capsule bias (batch-76 missed this file — anchor
    // used moonbeauty:: but needed crate:: prefix).
    // Finding 77: global offsets auto-scale from slant_width.
    let (bias_l, bias_r) = crate::bar::tooltip_spawn::read_offset_extra();
    let adjust_left = s.effective_tooltip_offset_left() + bias_l;
    let adjust_right = s.effective_tooltip_offset_right() + bias_r;

    // Clock is the keystone — center the window on the capsule, so the
    // octagon grows symmetrically outward from the clock's position.
    let is_clock = std::env::var("MBS_INFO_MODE")
        .map(|m| m == "clock")
        .unwrap_or(false);

    let (anchor, margin) = if is_clock {
        // Left-anchor with margin = cx - w/2, so window center = cx.
        let margin_left = (cx - (w as i32) / 2).max(0);
        (Anchor::Top | Anchor::Left, (margin_top, 0, 0, margin_left))
    } else if grow_right {
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
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        start_mode: output
            .map(iced_layershell::settings::StartMode::TargetScreen)
            .unwrap_or(iced_layershell::settings::StartMode::Active),
        margin,
        ..Default::default()
    };
    let ns_mode_val = std::env::var("MBS_INFO_MODE").unwrap_or_else(|_| "custom".into());
    let ns_mode = format!("mbs-info-tip-{}", ns_mode_val);

    iced_layershell::application(
        App::new_app,
        move || ns_mode.clone(),
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

async fn fetch_data(mode: String) -> InfoData {
    match mode.as_str() {
        "calendar" => InfoData::Calendar(ClockBackend::compute_calendar(0, 0)),
        "music" => InfoData::Music(MprisBackend::poll().await),
        "unified" => {
            let p = PodmanBackend::poll().await;
            let b = BorgBackend::poll().await;
            let s = ShellSettings::load();
            let items = crate::bar::unified::checks::check_unified_items(&s.unified_items).await;
            InfoData::Unified(p, b, items)
        }
        "net" => {
            let mut sc = SystemCollector::new();
            let _ = sc.update_net(); // Prime global rate
            let _ = sc.get_top_net_procs_bandwidth(); // Prime process bandwidth
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            let mut net = sc.update_net();
            net.top_procs = sc.get_top_net_procs_bandwidth(); // Calculate delta
            InfoData::Net(net)
        }
        "clock" => InfoData::Clock(ClockBackend::get_world_matrix()),
        _ => InfoData::Custom,
    }
}
