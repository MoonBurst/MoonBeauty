use crate::libraries::style::TextExt;
use iced::widget::{button, column, container, row, text, Stack};
use iced::{Alignment, Color, Element, Length, Padding, Task, Theme};
use iced_layershell::reexport::{KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use std::time::Instant;

use crate::libraries::backend::system::{GpuInfo, ProcessItem, SystemCollector};
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::config::ShellSettings;
use crate::libraries::style::shape_box::SlantSide;

use crate::overlays::tooltip_common::{
    layer_anchor_and_margin, proc_kill_row, slanted_row, slanted_search_bar, truncate_proc_name,
    TooltipEnv,
};

const W_START: f32 = 130.0;
const AUTO_MS: u128 = 10_000;

#[derive(Debug, Clone)]
pub enum Data {
    Cpu(crate::libraries::backend::system::CpuMetrics),
    Gpu(crate::libraries::backend::system::GpuMetrics),
    Ram(crate::libraries::backend::system::RamMetrics),
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Loaded(Data),
    Dismiss,
    KillProcess(u32),
    Noop,
    SearchChanged(String),
    CycleGpu,
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
    target: String,
    data: Option<Data>,
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
    gpu_list: Vec<GpuInfo>,
    selected_gpu: usize,
    anim_frame: u64,
    settings: ShellSettings,
    theme: ShellTheme,
}

#[allow(dead_code)]
impl App {
    fn new_app() -> (Self, Task<Message>) {
        crate::libraries::process::install_sigterm_handler();
        eprintln!("[sys-tooltip] V16 LOADED");
        let target = std::env::var("MBS_SYS_TARGET").unwrap_or_else(|_| "cpu".into());
        let env = TooltipEnv::from_env(W_START);
        let s = ShellSettings::load();
        let mut w: u32 = std::env::var("MBS_TOOLTIP_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(380);
        let mut h: u32 = std::env::var("MBS_TOOLTIP_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(s.tooltip_height_px);
        if target == "ram" {
            w = w.max(540);
            h = h.max(480);
        }

        let drop_ms: f32 = std::env::var("MBS_TOOLTIP_DROP_MS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(s.tooltip_drop_ms) as f32;
        let expand_ms: f32 = std::env::var("MBS_TOOLTIP_EXPAND_MS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(s.tooltip_expand_ms) as f32;
        

        let gpu_list = if target == "gpu" {
            SystemCollector::list_gpus_sync()
        } else {
            Vec::new()
        };
        for (i, g) in gpu_list.iter().enumerate() {
            eprintln!(
                "[gpu-init] {} card_id={} pci={} name={} nv={}",
                i, g.card_id, g.pci_addr, g.name, g.is_nvidia
            );
        }

        // Start on whichever GPU the user selected in Settings.
        let selected_gpu = if target == "gpu" {
            gpu_list
                .iter()
                .position(|g| g.card_id == s.gpu_selected_id)
                .unwrap_or(0)
        } else {
            0
        };
        eprintln!(
            "[gpu-init] selected_gpu={} (from settings.gpu_selected_id={:?})",
            selected_gpu, s.gpu_selected_id
        );

        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let app = Self {
            opened: Instant::now(),
            target: target.clone(),
            data: None,
            grow_right: env.grow_right,
            w,
            h,
            drop_ms,
            expand_ms,
            w_start: env.w_start,
            slant_l: env.slant_l,
            slant_r: env.slant_r,
            closing_at: None,
            search_query: String::new(),
            gpu_list,
            selected_gpu,
            anim_frame: 0,
            settings,
            theme,
        };
        let task = if target == "cpu" {
            Task::perform(async { SystemCollector::collect_all().await.0 }, |d| {
                Message::Loaded(Data::Cpu(d))
            })
        } else if target == "gpu" {
            let info = app.gpu_list.first().cloned().unwrap_or_default();
            Task::perform(
                async move {
                    // Run the blocking file I/O on a background thread so it
                    // doesn't freeze the iced event loop during animation.
                    tokio::task::spawn_blocking(move || {
                        tokio::runtime::Handle::current()
                            .block_on(SystemCollector::collect_gpu_by_info(&info))
                    })
                    .await
                    .unwrap_or_default()
                },
                |d| Message::Loaded(Data::Gpu(d)),
            )
        } else if target == "ram" {
            Task::perform(async { SystemCollector::collect_all().await.1 }, |d| {
                Message::Loaded(Data::Ram(d))
            })
        } else {
            eprintln!(
                "[sys-tooltip] FATAL: unknown MBS_SYS_TARGET={:?}; expected cpu|gpu|ram",
                target
            );
            std::process::exit(2);
        };
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
        format!("mbs-sys-tip-{}", self.target)
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
                    100
                };

                let mut tasks = Vec::new();

                if self.anim_frame % 250 == 0 && self.closing_at.is_none() {
                    let target = self.target.clone();
                    let gpu_info = self.gpu_list.get(self.selected_gpu).cloned().unwrap_or_default();
                    tasks.push(Task::perform(
                        async move {
                            match target.as_str() {
                                "cpu" => Data::Cpu(async { SystemCollector::collect_all().await.0 }.await),
                                "gpu" => Data::Gpu(SystemCollector::collect_gpu_by_info(&gpu_info).await),
                                _ => Data::Ram(async { SystemCollector::collect_all().await.1 }.await),
                            }
                        },
                        Message::Loaded,
                    ));
                }

                tasks.push(Task::perform(
                    async move { tokio::time::sleep(std::time::Duration::from_millis(next_ms)).await },
                    |_| Message::Tick,
                ));

                return Task::batch(tasks);
            }
            Message::SearchChanged(q) => {
                self.search_query = q;
            }
            Message::Loaded(d) => {
                self.data = Some(d);
            }
            Message::Dismiss => {
                if self.closing_at.is_none() {
                    let snap = self.snapshot_for_close();
                    self.closing_at = Some(snap);
                }
            }
            Message::KillProcess(pid) => {
                let target = self.target.clone();
                let gpu_info = self
                    .gpu_list
                    .get(self.selected_gpu)
                    .cloned()
                    .unwrap_or_default();
                return Task::batch([
                    Task::perform(SystemCollector::kill_process(pid), |_| Message::Noop),
                    Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            match target.as_str() {
                                "cpu" => Data::Cpu(
                                    async { SystemCollector::collect_all().await.0 }.await,
                                ),
                                "gpu" => {
                                    Data::Gpu(SystemCollector::collect_gpu_by_info(&gpu_info).await)
                                }
                                _ => Data::Ram(
                                    async { SystemCollector::collect_all().await.1 }.await,
                                ),
                            }
                        },
                        Message::Loaded,
                    ),
                ]);
            }
            Message::CycleGpu => {
                if !self.gpu_list.is_empty() {
                    self.selected_gpu = (self.selected_gpu + 1) % self.gpu_list.len();
                    let info = self.gpu_list[self.selected_gpu].clone();
                    eprintln!("[gpu-cycle] now {}", info.name);
                    return Task::perform(
                        async move { SystemCollector::collect_gpu_by_info(&info).await },
                        |d| Message::Loaded(Data::Gpu(d)),
                    );
                }
            }
            Message::Noop => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = &self.settings;
        let theme = &self.theme;
        let effective_h = self.h as f32;

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
                effective_h,
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
        let stacked = iced::widget::container(Stack::new().push(bg.view()).push(clipped))
            .width(Length::Fixed(anim.w))
            .height(Length::Fixed(anim.h))
            .clip(true);
        crate::overlays::tooltip_engine::align_to_side(
            stacked.into(),
            if self.grow_right {
                iced::alignment::Horizontal::Left
            } else {
                iced::alignment::Horizontal::Right
            },
        )
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        // Animation is driven by Command::perform recursion in update(),
        // not by a subscription timer. Subscriptions here only handle ESC.
        iced::event::listen_with(|event, _status, _window| {
            let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) = event
            else {
                return None;
            };
            Some(Message::Dismiss)
        })
    }
    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

fn render_proc_row(
    p: &ProcessItem,
    t: &ShellTheme,
    total_ram_gib: Option<f32>,
    is_gpu: bool,
    font_size: u16,
    alpha: f32,
    row_w: f32,
    row_h: f32,
    slant_right: bool,
    tooltip_slope: f32,
) -> Element<'static, Message> {
    let metric_str = if is_gpu {
        p.metric.clone()
    } else if let Some(total) = total_ram_gib {
        let rss_mib = leading_number(&p.metric);
        let total_mib = (total.max(0.01) as f64) * 1024.0;
        let pct = (rss_mib / total_mib * 100.0).clamp(0.0, 100.0);
        if rss_mib >= 1024.0 {
            format!("{:.1}% · {:.1}G", pct, rss_mib / 1024.0)
        } else {
            format!("{:.1}% · {:.0}M", pct, rss_mib)
        }
    } else {
        p.metric.clone()
    };
    proc_kill_row(
        truncate_proc_name(&p.name), metric_str, 140.0, Message::KillProcess(p.pid),
        t, font_size, alpha, row_w, row_h, slant_right, tooltip_slope,
    )
}

fn leading_number(s: &str) -> f64 {
    let first = s.split_whitespace().next().unwrap_or("");
    first.trim_end_matches('%').parse().unwrap_or(0.0)
}

impl App {
    fn render_search_bar<'a>(
        &'a self,
        t: &ShellTheme,
        settings: &ShellSettings,
        alpha: f32,
        y: f32,
        total_h: f32,
        slant_h: f32,
        slant_right: bool,
        safe: f32,
        w_f: f32,
        fs: u16,
        tooltip_slope: f32,
        gap: f32,
    ) -> (Element<'a, Message>, f32) {
        slanted_search_bar(
            &self.search_query, Message::SearchChanged, t, settings.control_border_width,
            alpha, y, total_h, slant_h, slant_right, safe, w_f, fs, tooltip_slope, gap,
            self.opened.elapsed().as_millis() as u64,
        )
    }
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
        let muted = a(t.muted);

        let fs = settings.tooltip_font_size;
        let total_h = self.h as f32;
        let bar_h = settings.bar_height.max(1) as f32;
        let slant_h = t.slant_width * (total_h / bar_h);
        let half_tt = settings.control_border_width * 0.5;
        // The tooltip's actual edge slope: slant measured over the path height (h - 2 * half_border)
        let tooltip_slope = slant_h / (total_h - 2.0 * half_tt);
        // One-time diagnostic
        {
            use std::sync::atomic::{AtomicBool, Ordering};
            static SHOWN: AtomicBool = AtomicBool::new(false);
            if !SHOWN.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "[slope-diag] tooltip h={:.1} slant={:.2} slope={:.4}",
                    total_h, slant_h, tooltip_slope
                );
            }
        }
        let slant_right = self.slant_l == SlantSide::Right;
        let safe = 16.0;
        let vpad = 14.0;
        let gap = 8.0;
        let w_f = self.w as f32;

        let mut y = vpad;
        let mut col = column![].spacing(gap).width(Length::Fill);

        if let Some(d) = &self.data {
            match d {
                Data::Cpu(c) => {
                    let header_h = fs as f32 + 16.0;
                    let header: Element<'_, Message> = row![
                        text("CPU METRICS")
                            .size((fs.max(12)) as f32)
                            .bold()
                            .color(text_col)
                            .wrapping(iced::widget::text::Wrapping::None),
                        iced::widget::space::horizontal(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();
                    col = col.push(slanted_row(
                        header,
                        y + header_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        header_h,
                    ));
                    y += header_h + gap;

                    let (search_el, new_y) = self.render_search_bar(
                        t,
                        settings,
                        alpha,
                        y,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        fs,
                        tooltip_slope,
                        gap,
                    );
                    col = col.push(search_el);
                    y = new_y;

                    let proc_h = fs as f32 + 10.0;
                    let proc_gap = 2.0;
                    let mut procs_col = column![].spacing(proc_gap).width(Length::Fill);
                    let q = self.search_query.to_lowercase();
                    let mut items: Vec<&ProcessItem> = c
                        .top_procs
                        .iter()
                        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                        .collect();
                    items.sort_by(|a, b| {
                        leading_number(&b.metric)
                            .partial_cmp(&leading_number(&a.metric))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for &p in items.iter().take(10) {
                        let inner_w = w_f
                            - (slant_h * (1.0 - (y + proc_h * 0.5) / total_h.max(1.0)) + safe)
                            - (slant_h * ((y + proc_h * 0.5) / total_h.max(1.0)) + safe);
                        let proc_row = render_proc_row(
                            p,
                            t,
                            None,
                            false,
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
                    }
                }
                Data::Gpu(g) => {
                    let header_h = fs as f32 + 16.0;
                    let multi = self.gpu_list.len() > 1;
                    let header_label = if multi {
                        format!("GPU: {} ({}%)  ▼", g.name, g.usage_pct)
                    } else {
                        format!("GPU: {} ({}%)", g.name, g.usage_pct)
                    };

                    let header_el: Element<'_, Message> = if multi {
                        button(
                            row![
                                text(header_label)
                                    .size((fs.max(12)) as f32)
                                    .bold()
                                    .color(text_col)
                                    .wrapping(iced::widget::text::Wrapping::None),
                                iced::widget::space::horizontal(),
                            ]
                            .align_y(Alignment::Center)
                            .width(Length::Fill),
                        )
                        .on_press(Message::CycleGpu)
                        .padding(0)
                        .width(Length::Fill)
                        .style(|_t, _s| button::Style {
                            background: None,
                            ..Default::default()
                        })
                        .into()
                    } else {
                        row![
                            text(header_label)
                                .size((fs.max(12)) as f32)
                                .bold()
                                .color(text_col)
                                .wrapping(iced::widget::text::Wrapping::None),
                            iced::widget::space::horizontal(),
                        ]
                        .align_y(Alignment::Center)
                        .width(Length::Fill)
                        .into()
                    };
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

                    let (search_el, new_y) = self.render_search_bar(
                        t,
                        settings,
                        alpha,
                        y,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        fs,
                        tooltip_slope,
                        gap,
                    );
                    col = col.push(search_el);
                    y = new_y;

                    let proc_h = fs as f32 + 10.0;
                    let proc_gap = 2.0;
                    let mut procs_col = column![].spacing(proc_gap).width(Length::Fill);
                    let q = self.search_query.to_lowercase();
                    let mut items: Vec<&ProcessItem> = g
                        .top_procs
                        .iter()
                        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                        .collect();
                    items.sort_by(|a, b| {
                        leading_number(&b.metric)
                            .partial_cmp(&leading_number(&a.metric))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for &p in items.iter().take(10) {
                        let inner_w = w_f
                            - (slant_h * (1.0 - (y + proc_h * 0.5) / total_h.max(1.0)) + safe)
                            - (slant_h * ((y + proc_h * 0.5) / total_h.max(1.0)) + safe);
                        let proc_row = render_proc_row(
                            p,
                            t,
                            Some(g.vram_total_gib as f32),
                            true,
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
                        let empty_el: Element<'_, Message> = container(
                            text(if g.top_procs.is_empty() {
                                "(no GPU processes tracked)"
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
                Data::Ram(r) => {
                    let total = r.total_gib.max(0.01);

                    let header_h = fs as f32 + 16.0;
                    let header: Element<'_, Message> = row![
                        text("RAM USAGE")
                            .size((fs.max(12)) as f32)
                            .bold()
                            .color(text_col)
                            .wrapping(iced::widget::text::Wrapping::None),
                        iced::widget::space::horizontal(),
                    ]
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .into();
                    col = col.push(slanted_row(
                        header,
                        y + header_h * 0.5,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        header_h,
                    ));
                    y += header_h + gap;

                    let (search_el, new_y) = self.render_search_bar(
                        t,
                        settings,
                        alpha,
                        y,
                        total_h,
                        slant_h,
                        slant_right,
                        safe,
                        w_f,
                        fs,
                        tooltip_slope,
                        gap,
                    );
                    col = col.push(search_el);
                    y = new_y;

                    let proc_h = fs as f32 + 10.0;
                    let proc_gap = 2.0;
                    let mut procs_col = column![].spacing(proc_gap).width(Length::Fill);
                    let q = self.search_query.to_lowercase();
                    let mut items: Vec<&ProcessItem> = r
                        .top_procs
                        .iter()
                        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                        .collect();
                    items.sort_by(|a, b| {
                        leading_number(&b.metric)
                            .partial_cmp(&leading_number(&a.metric))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for &p in items.iter().take(10) {
                        let inner_w = w_f
                            - (slant_h * (1.0 - (y + proc_h * 0.5) / total_h.max(1.0)) + safe)
                            - (slant_h * ((y + proc_h * 0.5) / total_h.max(1.0)) + safe);
                        let proc_row = render_proc_row(
                            p,
                            t,
                            Some(total),
                            false,
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
                    }
                }
            }
        } else {
            let header_h = fs as f32 + 16.0;
            let header: Element<'_, Message> = row![
                text("LOADING METRICS...")
                    .size((fs.max(12)) as f32)
                    .color(muted)
                    .wrapping(iced::widget::text::Wrapping::None),
                iced::widget::space::horizontal(),
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .into();
            col = col.push(slanted_row(
                header,
                y + header_h * 0.5,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                header_h,
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

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    let s = ShellSettings::load();
    let mut w: u32 = std::env::var("MBS_TOOLTIP_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(380);
    let mut h: u32 = std::env::var("MBS_TOOLTIP_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(s.tooltip_height_px);
    if std::env::var("MBS_SYS_TARGET").unwrap_or_default() == "ram" {
        w = w.max(540);
        h = h.max(480);
    }

    let (anchor, margin) = layer_anchor_and_margin(&s, w, false);

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
    let ns_tgt_val = std::env::var("MBS_SYS_TARGET").unwrap_or_else(|_| "cpu".into());
    let ns_target = format!("mbs-sys-tip-{}", ns_tgt_val);

    iced_layershell::application(
        App::new_app,
        move || ns_target.clone(),
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
