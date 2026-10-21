use iced::widget::{button, column, container, row, text, MouseArea, Stack};
use iced::{Alignment, Color, Element, Length, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use std::path::PathBuf;
use std::time::Instant;

use crate::config::theme::ShellTheme;
use crate::config::ShellSettings;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

const W: u32 = 300;
const H: u32 = 380;
const W_START: f32 = 100.0;
const H_START: f32 = 48.0;
const ANIM_MS: f32 = 320.0;
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
    Tick,
    Dismiss,
    ItemsLoaded(Vec<TrayItem>),
    Activate(TrayItem),
    ContextMenu(TrayItem),
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
    slant_mode: String,
    items: Vec<TrayItem>,
    loaded: bool,
}

impl Application for App {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_: ()) -> (Self, Task<Message>) {
        let cx: i32 = std::env::var("MBS_CAPSULE_CX").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
        let sw: i32 = std::env::var("MBS_SCREEN_W").ok().and_then(|s| s.parse().ok()).unwrap_or(1920);
        let grow_right = cx < sw / 2;
        let slant_mode = std::env::var("MBS_WX_SLANT").unwrap_or_else(|_| "left".into());
        let app = Self {
            opened: Instant::now(),
            grow_right,
            slant_mode,
            items: Vec::new(),
            loaded: false,
        };
        let task = Task::perform(fetch_tray_items(), Message::ItemsLoaded);
        (app, task)
    }

    fn namespace(&self) -> String { "mbs-tray-tip".into() }

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
            Message::ItemsLoaded(items) => {
                eprintln!("[tray-popup] loaded {} item(s)", items.len());
                self.items = items;
                self.loaded = true;
            }
            Message::Activate(item) => {
                eprintln!("[tray-popup] activate {}/{}", item.bus, item.path);
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all().build().ok();
                    if let Some(rt) = rt {
                        rt.block_on(async move {
                            if let Err(e) = call_sni_method(&item.bus, &item.path, "Activate").await {
                                eprintln!("[tray-popup] Activate failed: {e}");
                            }
                        });
                    }
                });
                std::process::exit(0);
            }
            Message::ContextMenu(item) => {
                eprintln!("[tray-popup] context menu {}/{}", item.bus, item.path);
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all().build().ok();
                    if let Some(rt) = rt {
                        rt.block_on(async move {
                            if let Err(e) = call_sni_method(&item.bus, &item.path, "ContextMenu").await {
                                eprintln!("[tray-popup] ContextMenu failed: {e}");
                            }
                        });
                    }
                });
                std::process::exit(0);
            }
            Message::Dismiss => std::process::exit(0),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let t = (self.opened.elapsed().as_millis() as f32 / ANIM_MS).min(1.0);

        let w_phase = (t / 0.35).min(1.0);
        let w_ease = 1.0 - (1.0 - w_phase).powi(3);
        let anim_w = W_START + (W as f32 - W_START) * w_ease;

        let h_phase = ((t - 0.25) / 0.75).clamp(0.0, 1.0);
        let h_ease = 1.0 - (1.0 - h_phase).powi(3);
        let anim_h = H_START + (H as f32 - H_START) * h_ease;

        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);

        let slant = (theme.slant_width * (H_START / settings.bar_height as f32)).max(0.0);
        let (sl, sr) = match self.slant_mode.as_str() {
            "right"  => (SlantSide::Right, SlantSide::Right),
            "center" => (SlantSide::Left,  SlantSide::Right),
            _        => (SlantSide::Left,  SlantSide::Left),
        };

        let bg_shape = ShapeBox::new()
            .shape(ShapeType::Slant { left: sl, right: sr })
            .background(theme.background)
            .border(theme.text, theme.control_border_width)
            .slant_width(slant)
            .size(Length::Fixed(anim_w), Length::Fixed(anim_h));

        let body: Element<Message> = if !self.loaded {
            container(text("Loading tray items…").size(13).color(theme.muted))
                .padding(16).center_x(Length::Fill).into()
        } else if self.items.is_empty() {
            column![
                text("No tray items registered").size(13).color(theme.muted),
                text("Start a tray app (Steam, CoreCtrl, music player, …)").size(11).color(theme.muted),
            ]
            .spacing(6)
            .padding(16)
            .into()
        } else {
            let _hover_bg = theme.surface_alt;
            let text_color = theme.text;
            let icon_color = theme.success;

            let mut col = column![].spacing(2).width(Length::Fill);
            for item in &self.items {
                let label = if !item.title.is_empty() { item.title.clone() } else { item.id.clone() };
                let icon_elem: Element<Message> = if let Some(path) = &item.icon_path {
                    iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                        .width(Length::Fixed(22.0))
                        .height(Length::Fixed(22.0))
                        .into()
                } else {
                    text("●").size(18).color(icon_color).into()
                };

                let row_inner = container(
                    row![
                        icon_elem,
                        text(label).size(13).color(text_color).width(Length::Fill),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .padding([8, 10]),
                )
                .width(Length::Fill);

                let interactive = MouseArea::new(row_inner)
                    .on_press(Message::Activate(item.clone()))
                    .on_right_press(Message::ContextMenu(item.clone()));

                col = col.push(interactive);
            }
            container(col).padding(6).into()
        };

        let clip_align = if self.grow_right {
            iced::alignment::Horizontal::Left
        } else {
            iced::alignment::Horizontal::Right
        };

        let clipped = container(body)
            .width(Length::Fixed(anim_w))
            .height(Length::Fixed(anim_h))
            .clip(true)
            .align_x(clip_align)
            .align_y(iced::alignment::Vertical::Top);

        let inner = Stack::new().push(bg_shape.view()).push(clipped);

        let positioned = container(inner)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(clip_align)
            .align_y(iced::alignment::Vertical::Top)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            });

        let catcher = button(container(row![]).width(Length::Fill).height(Length::Fill))
            .on_press(Message::Dismiss)
            .padding(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            });

        Stack::new().push(catcher).push(positioned).into()
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

// ── D-Bus helpers ────────────────────────────────────────────────────

fn parse_registered(s: &str) -> Option<(String, String)> {
    if let Some(idx) = s.find('/') {
        Some((s[..idx].to_string(), s[idx..].to_string()))
    } else if !s.is_empty() {
        Some((s.to_string(), "/StatusNotifierItem".to_string()))
    } else {
        None
    }
}

async fn fetch_tray_items() -> Vec<TrayItem> {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => { eprintln!("[tray-popup] session bus: {e}"); return Vec::new(); }
    };

    let watcher = match zbus::Proxy::new(
        &conn,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    ).await {
        Ok(p) => p,
        Err(e) => { eprintln!("[tray-popup] watcher proxy: {e}"); return Vec::new(); }
    };

    let raw_items: Vec<String> = match watcher.get_property("RegisteredStatusNotifierItems").await {
        Ok(v) => v,
        Err(e) => { eprintln!("[tray-popup] RegisteredStatusNotifierItems: {e}"); return Vec::new(); }
    };
    eprintln!("[tray-popup] registered: {:?}", raw_items);

    let mut out = Vec::new();
    for raw in raw_items {
        let Some((bus, path)) = parse_registered(&raw) else { continue };
        let proxy = match zbus::Proxy::new(&conn, bus.as_str(), path.as_str(), "org.kde.StatusNotifierItem").await {
            Ok(p) => p,
            Err(e) => { eprintln!("[tray-popup] {} proxy: {e}", raw); continue; }
        };

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

        eprintln!("[tray-popup]   {} -> id={:?} title={:?} icon={:?}", raw, id, title, icon_path);
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
    if icon_name.starts_with('/') {
        return if Path::new(icon_name).exists() { Some(icon_name.to_string()) } else { None };
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let roots = [
        PathBuf::from("/run/current-system/sw/share/icons/hicolor"),
        PathBuf::from("/usr/share/icons/hicolor"),
        PathBuf::from("/usr/local/share/icons/hicolor"),
        home.join(".local/share/icons/hicolor"),
        home.join(".local/share/flatpak/exports/share/icons/hicolor"),
        home.join(".nix-profile/share/icons/hicolor"),
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
    for pix in ["/run/current-system/sw/share/pixmaps", "/usr/share/pixmaps"] {
        for ext in &exts {
            let p = PathBuf::from(pix).join(format!("{}.{}", icon_name, ext));
            if p.exists() { return Some(p.to_string_lossy().into_owned()); }
        }
    }
    None
}

fn save_pixmap(id: &str, w: u32, h: u32, data: &[u8]) -> Option<String> {
    if w == 0 || h == 0 { return None; }
    if data.len() != (w as usize * h as usize * 4) { return None; }
    let mut rgba = Vec::with_capacity(data.len());
    // SNI pixmap format is ARGB32 in network byte order: [A, R, G, B]
    for chunk in data.as_chunks::<4>().0 {
        rgba.push(chunk[1]); // R
        rgba.push(chunk[2]); // G
        rgba.push(chunk[3]); // B
        rgba.push(chunk[0]); // A
    }
    let img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba)?;
    let safe: String = id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let path = std::env::temp_dir().join(format!("mbs-tray-{}.png", safe));
    img.save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
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
