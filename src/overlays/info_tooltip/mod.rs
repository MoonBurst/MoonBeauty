mod buttons;
mod calendar;
mod music;
mod unified;
mod net;
mod clock;
mod custom;
mod net_proc;
use buttons::*;

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

use crate::overlays::tooltip_common::{slanted_row, KillButtonShape};

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
    closing_at: Option<crate::overlays::tooltip_engine::ClosingState>,
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
        crate::libraries::process::install_sigterm_handler();
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

    fn snapshot_for_close(&self) -> crate::overlays::tooltip_engine::ClosingState {
        let cur = crate::overlays::tooltip_engine::compute(
            self.opened.elapsed().as_millis() as f32,
            self.w_start,
            self.w as f32,
            self.h as f32,
            if self.mode == "clock" { 0.0 } else { self.theme.slant_width },
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
        format!("mbs-info-tip-{}", self.mode)
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
                
                let mut tasks = Vec::new();

                // Keep the tooltip live if it's the music player (~1 sec)
                // ONLY do this if we aren't currently closing!
                if self.mode == "music" && self.anim_frame % 250 == 0 && self.closing_at.is_none() {
                    tasks.push(Task::perform(fetch_data("music".to_string()), Message::Loaded));
                }

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
                    // ~100ms during idle.
                    100
                };

                tasks.push(Task::perform(
                    async move { tokio::time::sleep(std::time::Duration::from_millis(next_ms)).await },
                    |_| Message::Tick,
                ));

                return Task::batch(tasks);
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
                if self.closing_at.is_none() {
                    let snap = self.snapshot_for_close();
                    self.closing_at = Some(snap);
                }
            }
            Message::MusicPrev => {
                return Task::perform(
                    async {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = tokio::process::Command::new(exe)
                                .arg("--music-previous")
                                .status()
                                .await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        fetch_data("music".to_string()).await
                    },
                    Message::Loaded,
                );
            }
            Message::MusicNext => {
                return Task::perform(
                    async {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = tokio::process::Command::new(exe)
                                .arg("--music-next")
                                .status()
                                .await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        fetch_data("music".to_string()).await
                    },
                    Message::Loaded,
                );
            }
            Message::MusicPlayPause => {
                return Task::perform(
                    async {
                        if let Ok(exe) = std::env::current_exe() {
                            let _ = tokio::process::Command::new(exe)
                                .arg("--music-play-pause")
                                .status()
                                .await;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        fetch_data("music".to_string()).await
                    },
                    Message::Loaded,
                );
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

        let anim = if let Some(t) = &self.closing_at {
            crate::overlays::tooltip_engine::compute_close_from(
                t.started.elapsed().as_millis() as f32,
                t.from_w,
                t.from_h,
                t.from_opacity,
                self.w_start,
                self.w as f32,
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
        let muted = a(t.muted);
        let half_tt = settings.control_border_width * 0.5;

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
                    col = self.render_music(track_opt, col, &mut y, t, settings, total_h, slant_h, slant_right, safe, w_f, alpha);
                }
                InfoData::Unified(podman, borg, custom_items) => {
                    col = self.render_unified(podman, borg, custom_items, col, &mut y, t, settings, total_h, slant_h, slant_right, safe, w_f, alpha);
                }
                InfoData::Net(net) => {
                    col = self.render_net(net, col, &mut y, t, settings, total_h, slant_h, slant_right, safe, w_f, alpha, tooltip_slope);
                }
                InfoData::Clock(cols) => {
                    col = self.render_clock(cols, col, &mut y, t, settings, total_h, safe, w_f, alpha);
                }
                InfoData::Custom => {
                    col = self.render_custom(col, &mut y, t, settings, total_h, slant_h, slant_right, safe, w_f, alpha);
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
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
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
        "music" => InfoData::Music(crate::libraries::backend::poll_track().await.0),
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
