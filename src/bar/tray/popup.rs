use iced::widget::{button, column, container, row, text, MouseArea, Stack};
use iced::{Alignment, Element, Length, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use std::time::Instant;

use super::watcher::{fetch_tray_items, TrayItem};
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::config::ShellSettings;
use crate::libraries::style::shape_box::SlantSide;

const W_START: f32 = 100.0;
const AUTO_MS: u128 = 25_000;

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Dismiss,
    ItemsLoaded(Vec<TrayItem>),
    Activate(TrayItem),
    ContextMenu(TrayItem),
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
    grow_right: bool,
    w: u32,
    h: u32,
    drop_ms: f32,
    expand_ms: f32,
    w_start: f32,
    slant_l: SlantSide,
    #[allow(dead_code)]
    slant_r: SlantSide,
    items: Vec<TrayItem>,
    loaded: bool,
    closing_at: Option<Instant>,
    anim_frame: u64,
}

#[allow(dead_code)]
impl App {
    fn new_app() -> (Self, Task<Message>) {
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
        let _h: u32 = std::env::var("MBS_TOOLTIP_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let w: u32 = std::env::var("MBS_TOOLTIP_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(380);
        let s = ShellSettings::load();
        let h = s.tooltip_height_px;
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
            grow_right,
            w,
            w_start,
            slant_l,
            slant_r,
            items: Vec::new(),
            loaded: false,
            h,
            drop_ms,
            expand_ms,
            closing_at: None,
            anim_frame: 0,
        };
        let task = Task::perform(fetch_tray_items(), Message::ItemsLoaded);
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
        "mbs-tray-tip".into()
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => {
                self.anim_frame = self.anim_frame.wrapping_add(1);
                let elapsed_ms = self.opened.elapsed().as_millis() as u64;
                eprintln!("[tick] frame={} elapsed={}ms", self.anim_frame, elapsed_ms);

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
            Message::ItemsLoaded(items) => {
                self.items = items;
                self.loaded = true;
            }
            Message::Activate(item) => {
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok();
                    if let Some(rt) = rt {
                        rt.block_on(async move {
                            let _ = call_sni_method(&item.bus, &item.path, "Activate").await;
                        });
                    }
                });
                std::process::exit(0);
            }
            Message::ContextMenu(item) => {
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .ok();
                    if let Some(rt) = rt {
                        rt.block_on(async move {
                            let _ = call_sni_method(&item.bus, &item.path, "ContextMenu").await;
                        });
                    }
                });
                std::process::exit(0);
            }
            Message::Dismiss => {
                self.closing_at = Some(Instant::now());
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        let anim = if let Some(t) = self.closing_at {
            crate::overlays::tooltip_engine::compute_close(
                t.elapsed().as_millis() as f32,
                self.w_start,
                self.w as f32,
                self.h as f32,
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
        let _slant = (theme.slant_width * (anim.h / settings.bar_height as f32)).max(0.0);

        let slant_h = theme.slant_width * (anim.h / settings.bar_height as f32);
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

        let body: Element<'_, Message> = if !self.loaded {
            container(text("Loading tray items…").size(13).color(theme.muted))
                .padding(16)
                .center_x(Length::Fill)
                .into()
        } else if self.items.is_empty() {
            column![
                text("No tray items registered").size(13).color(theme.muted),
                text("Start a tray app (Steam, CoreCtrl, music player, …)")
                    .size(11)
                    .color(theme.muted),
            ]
            .spacing(6)
            .padding(16)
            .into()
        } else {
            let text_color = theme.text;
            let icon_color = theme.success;
            let mut col: iced::widget::Column<'_, Message> =
                iced::widget::Column::new().spacing(2).width(Length::Fill);
            for item in &self.items {
                let label = if !item.title.is_empty() {
                    item.title.clone()
                } else {
                    item.id.clone()
                };
                let icon_elem: Element<'_, Message> = if let Some(path) = &item.icon_path {
                    if path.ends_with(".svg") {
                        iced::widget::svg(iced::widget::svg::Handle::from_path(path.clone()))
                            .width(Length::Fixed(22.0))
                            .height(Length::Fixed(22.0))
                            .into()
                    } else {
                        iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                            .width(Length::Fixed(22.0))
                            .height(Length::Fixed(22.0))
                            .content_fit(iced::ContentFit::Contain)
                            .into()
                    }
                } else {
                    text("●").size(18).color(icon_color).into()
                };

                let row_inner = container(
                    row![
                        icon_elem,
                        text(label).size(13).color(text_color).width(Length::Fill)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .padding([8, 10]),
                )
                .width(Length::Fill);

                let interactive: Element<'_, Message> = MouseArea::new(row_inner)
                    .on_press(Message::Activate(item.clone()))
                    .on_right_press(Message::ContextMenu(item.clone()))
                    .into();
                col = col.push(interactive);
            }
            container(col).padding(6).into()
        };

        let body_fixed: Element<'_, Message> = container(body)
            .width(Length::Fixed(self.w as f32))
            .height(Length::Fixed(self.h as f32))
            .into();
        let clipped = crate::overlays::tooltip_engine::clip_content(
            body_fixed,
            anim.w,
            anim.h,
            if self.grow_right {
                iced::alignment::Horizontal::Left
            } else {
                iced::alignment::Horizontal::Right
            },
        );
        let inner: Element<'_, Message> = Stack::new().push(bg.view()).push(clipped).into();
        let positioned: Element<'_, Message> =
            container(crate::overlays::tooltip_engine::align_to_side(
                inner,
                if self.grow_right {
                    iced::alignment::Horizontal::Left
                } else {
                    iced::alignment::Horizontal::Right
                },
            ))
            .into();

        let catcher: Element<'_, Message> = button(container(
            iced::widget::container(iced::widget::Space::new()).width(Length::Fill),
        ))
        .on_press(Message::Dismiss)
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        Stack::new().push(catcher).push(positioned).into()
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

async fn call_sni_method(
    bus: &str,
    path: &str,
    method: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, bus, path, "org.kde.StatusNotifierItem").await?;
    proxy.call_method(method, &(0i32, 0i32)).await?;
    Ok(())
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    eprintln!(
        "[tip-output] raw={:?} parsed={:?}",
        std::env::var("MBS_OUTPUT"),
        output
    );
    let s = ShellSettings::load();
    let h: u32 = std::env::var("MBS_TOOLTIP_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let w: u32 = std::env::var("MBS_TOOLTIP_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(380);
    let cx: i32 = std::env::var("MBS_CAPSULE_CX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1920);
    let grow_right = cx < sw / 2;
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(W_START);
    let half_start = (cap_w / 2.0) as i32;
    let bar_h = s.bar_height as i32;
    let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
    let sr_str = if grow_right {
        sl_str.clone()
    } else {
        std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "R".into())
    };
    let sw_cap = s.slant_width;
    let _sw_tip = s.slant_width * (h as f32 / s.bar_height as f32);
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);

    let (anchor, margin) = if grow_right {
        let sl_left = sl_str == "L";
        let shift = if sl_left { sw_cap } else { 0.0 };
        (
            Anchor::Top | Anchor::Left,
            (
                margin_top,
                0,
                0,
                ((cx - half_start) as f32 + shift)
                    .max(0.0)
                    .min((sw - w as i32).max(0) as f32) as i32,
            ),
        )
    } else {
        let sr_right = sr_str == "R";
        let shift = if sr_right { sw_cap } else { 0.0 };
        (
            Anchor::Top | Anchor::Right,
            (
                margin_top,
                ((sw - cx - half_start) as f32 + shift)
                    .max(0.0)
                    .min((sw - w as i32).max(0) as f32) as i32,
                0,
                0,
            ),
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
    iced_layershell::application(
        App::new_app,
        || "mbs-tray-tip".to_string(),
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
