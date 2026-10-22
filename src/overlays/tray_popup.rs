use iced::widget::{button, column, container, row, text, MouseArea, Space, Stack};
use iced::{Alignment, Color, Element, Length, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::path::PathBuf;
use std::time::Instant;

use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::SlantSide;

const W_START: f32 = 100.0;
const AUTO_MS: u128 = 25_000;

#[derive(Debug, Clone)]
pub struct TrayItem {
    pub bus: String,
    pub path: String,
    pub id: String,
    pub title: String,
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick, Dismiss, ItemsLoaded(Vec<TrayItem>), Activate(TrayItem), ContextMenu(TrayItem),
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
    w_start: f32,
    slant_l: SlantSide,
    #[allow(dead_code)]
    slant_r: SlantSide,
    items: Vec<TrayItem>,
    loaded: bool,
    closing_at: Option<Instant>,
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
        let _h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
        let s = ShellSettings::load();
        let h = s.tooltip_height_px;
        let drop_ms = s.tooltip_drop_ms as f32;
        let expand_ms = s.tooltip_expand_ms as f32;
        let w_start: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
        let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
        let sr_str = std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "L".into());
        let slant_l = if sl_str == "R" { SlantSide::Right } else { SlantSide::Left };
        let slant_r = if sr_str == "R" { SlantSide::Right } else { SlantSide::Left };

        let app = Self {
            opened: Instant::now(), grow_right, w, w_start, slant_l, slant_r,
            items: Vec::new(), loaded: false, h, drop_ms, expand_ms,
            closing_at: None,};
        let task = Task::perform(fetch_tray_items(), Message::ItemsLoaded);
        (app, task)
    }

    fn namespace(&self) -> String { "mbs-tray-tip".into() }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance { background_color: Color::TRANSPARENT, text_color: Color::WHITE }
    }

    fn update(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::Tick => { if let Some(t) = self.closing_at {
                    if t.elapsed().as_millis() > (self.drop_ms * 1.9) as u128 {
                        std::process::exit(0);
                    }
                } else if self.opened.elapsed().as_millis() > AUTO_MS {
                    std::process::exit(0);
                } }
            Message::ItemsLoaded(items) => { self.items = items; self.loaded = true; }
            Message::Activate(item) => {
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok();
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
                    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok();
                    if let Some(rt) = rt {
                        rt.block_on(async move {
                            let _ = call_sni_method(&item.bus, &item.path, "ContextMenu").await;
                        });
                    }
                });
                std::process::exit(0);
            }
            Message::Dismiss => { self.closing_at = Some(Instant::now()); }
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
        };

        let body: Element<'_, Message> = if !self.loaded {
            container(text("Loading tray items…").size(13).color(theme.muted))
                .padding(16).center_x(Length::Fill).into()
        } else if self.items.is_empty() {
            column![
                text("No tray items registered").size(13).color(theme.muted),
                text("Start a tray app (Steam, CoreCtrl, music player, …)").size(11).color(theme.muted),
            ].spacing(6).padding(16).into()
        } else {
            let text_color = theme.text;
            let icon_color = theme.success;
            let mut col: iced::widget::Column<'_, Message> = iced::widget::Column::new().spacing(2).width(Length::Fill);
            for item in &self.items {
                let label = if !item.title.is_empty() { item.title.clone() } else { item.id.clone() };
                let icon_elem: Element<'_, Message> = if let Some(path) = &item.icon_path {
                    iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                        .width(Length::Fixed(22.0)).height(Length::Fixed(22.0)).into()
                } else {
                    text("●").size(18).color(icon_color).into()
                };

                let row_inner = container(
                    row![icon_elem, text(label).size(13).color(text_color).width(Length::Fill)]
                        .spacing(10).align_y(Alignment::Center).padding([8, 10]),
                ).width(Length::Fill);

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
        let clipped = crate::overlays::tooltip_engine::clip_content(body_fixed, anim.w, anim.h, self.grow_right);
        let inner: Element<'_, Message> = Stack::new().push(bg.view()).push(clipped).into();
        let positioned: Element<'_, Message> = container(crate::overlays::tooltip_engine::align_to_side(inner, self.grow_right)).into();

        let catcher: Element<'_, Message> = button(container(Space::new(Length::Fill, Length::Fill)))
            .on_press(Message::Dismiss)
            .padding(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        Stack::new().push(catcher).push(positioned).into()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick),
            iced::keyboard::on_key_press(|k, _| if k == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) { Some(Message::Dismiss) } else { None }),
        ])
    }
    fn theme(&self) -> Theme { Theme::Dark }
}

fn parse_registered(s: &str) -> Option<(String, String)> {
    if let Some(idx) = s.find('/') { Some((s[..idx].to_string(), s[idx..].to_string())) }
    else if !s.is_empty() { Some((s.to_string(), "/StatusNotifierItem".to_string())) }
    else { None }
}

async fn fetch_tray_items() -> Vec<TrayItem> {
    let conn = match zbus::Connection::session().await { Ok(c) => c, Err(_) => return Vec::new() };
    let watcher = match zbus::Proxy::new(&conn, "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher").await { Ok(p) => p, Err(_) => return Vec::new() };
    let raw_items: Vec<String> = match watcher.get_property("RegisteredStatusNotifierItems").await { Ok(v) => v, Err(_) => return Vec::new() };

    let mut out = Vec::new();
    for raw in raw_items {
        let Some((bus, path)) = parse_registered(&raw) else { continue };
        let proxy = match zbus::Proxy::new(&conn, bus.as_str(), path.as_str(), "org.kde.StatusNotifierItem").await { Ok(p) => p, Err(_) => continue };

        let id: String = proxy.get_property("Id").await.unwrap_or_default();
        let title: String = proxy.get_property("Title").await.unwrap_or_default();
        let status: String = proxy.get_property("Status").await.unwrap_or_default();
        if status.eq_ignore_ascii_case("Passive") { continue; }

        let icon_name: String = proxy.get_property("IconName").await.unwrap_or_default();
        let mut icon_path = if !icon_name.is_empty() { resolve_icon(&icon_name) } else { None };
        if icon_path.is_none() {
            if let Ok(pixmaps) = proxy.get_property::<Vec<(i32, i32, Vec<u8>)>>("IconPixmap").await {
                if let Some((w, h, data)) = pixmaps.into_iter().max_by_key(|(w, h, _)| (*w, *h)) {
                    icon_path = save_pixmap(&id, w.max(0) as u32, h.max(0) as u32, &data);
                }
            }
        }
        out.push(TrayItem { bus, path, id, title, icon_path });
    }
    out
}

async fn call_sni_method(bus: &str, path: &str, method: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, bus, path, "org.kde.StatusNotifierItem").await?;
    proxy.call_method(method, &(0i32, 0i32)).await?;
    Ok(())
}

fn resolve_icon(icon_name: &str) -> Option<String> {
    use std::path::Path;
    if icon_name.is_empty() { return None; }
    if icon_name.starts_with('/') { return if Path::new(icon_name).exists() { Some(icon_name.to_string()) } else { None }; }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let roots = [
        PathBuf::from("/run/current-system/sw/share/icons/hicolor"), PathBuf::from("/usr/share/icons/hicolor"),
        home.join(".local/share/icons/hicolor"), home.join(".nix-profile/share/icons/hicolor"),
    ];
    let sizes = ["48x48", "64x64", "128x128", "256x256", "512x512", "scalable", "symbolic"];
    let exts = ["png", "svg", "xpm"];
    for root in &roots {
        for size in &sizes {
            for ext in &exts {
                let p = root.join(size).join("apps").join(format!("{}.{}", icon_name, ext));
                if p.exists() { return Some(p.to_string_lossy().into_owned()); }
            }
        }
    }
    None
}

fn save_pixmap(id: &str, w: u32, h: u32, data: &[u8]) -> Option<String> {
    if w == 0 || h == 0 || data.len() != (w as usize * h as usize * 4) { return None; }
    let mut rgba = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(4) {
        rgba.push(chunk[1]); rgba.push(chunk[2]); rgba.push(chunk[3]); rgba.push(chunk[0]);
    }
    let img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba)?;
    let safe: String = id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let path = std::env::temp_dir().join(format!("mbs-tray-{}.png", safe));
    img.save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}

pub fn run() -> Result<(), iced_layershell::Error> {
    let output: Option<String> = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());
    eprintln!("[tip-output] raw={:?} parsed={:?}", std::env::var("MBS_OUTPUT"), output);
    let s = ShellSettings::load();
    let h: u32 = std::env::var("MBS_TOOLTIP_H").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let w: u32 = std::env::var("MBS_TOOLTIP_W").ok().and_then(|v| v.parse().ok()).unwrap_or(380);
    let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let grow_right = cx < sw / 2;
    let cap_w: f32 = std::env::var("MBS_CAPSULE_W").ok().and_then(|v| v.parse().ok()).unwrap_or(W_START);
    let half_start = (cap_w / 2.0) as i32;
    let bar_h = s.bar_height as i32;
    let sl_str = std::env::var("MBS_SLANT_L").unwrap_or_else(|_| "L".into());
    let sr_str = if grow_right { sl_str.clone() } else {
        std::env::var("MBS_SLANT_R").unwrap_or_else(|_| "R".into())
    };
    let sw_cap = s.slant_width;
    let _sw_tip = s.slant_width * (h as f32 / s.bar_height as f32);
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
        let layer = LayerShellSettings { size: Some((w, h)), exclusive_zone: -1, anchor, layer: Layer::Overlay, keyboard_interactivity: KeyboardInteractivity::OnDemand, binded_output_name: output, margin, ..Default::default() };
    App::run(Settings { layer_settings: layer, ..Default::default() })
}
