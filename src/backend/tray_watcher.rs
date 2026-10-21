//! StatusNotifierWatcher host.
//!
//! Tray applications (Steam, CoreCtrl, music players, Discord, …) speak the
//! `org.kde.StatusNotifierItem` protocol. For their registration call to go
//! anywhere, something has to own `org.kde.StatusNotifierWatcher` on the
//! session bus. Waybar / Plasma do this; bare Sway+this-shell does not, which
//! is why SNI apps silently disappear without a host. This module is that host.
//!
//! It's a pure D-Bus background thread — no window, no layer surface. The
//! tray_popup overlay queries `RegisteredStatusNotifierItems` on startup.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct TrayWatcher {
    items: Arc<Mutex<HashSet<String>>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl TrayWatcher {
    fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] hdr: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        // Some clients (ayatana-style, e.g. Steam) send just the object path.
        // Fall back to the sender's unique bus name so the item is reachable.
        let entry = if service.starts_with('/') {
            let sender = hdr.sender().map(|s| s.as_str().to_string()).unwrap_or_default();
            format!("{}{}", sender, service)
        } else {
            normalize_service(&service)
        };
        let is_new = self.items.lock().unwrap().insert(entry.clone());
        if is_new {
            eprintln!("[tray-watcher] registered: {}", entry);
        }
        Ok(())
    }

    fn register_status_notifier_host(&self, service: String) -> zbus::fdo::Result<()> {
        eprintln!("[tray-watcher] host registered: {}", service);
        Ok(())
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().iter().cloned().collect()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool { true }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 { 0 }
}

fn normalize_service(s: &str) -> String {
    // Registration strings are one of:
    //   ":1.42"                            (well-known bus name, default path)
    //   "org.kde.StatusNotifierItem-1234"  (well-known service name)
    //   ":1.42/org/ayatana/.../steam"      (explicit object path)
    if let Some(idx) = s.find('/') {
        format!("{}{}", &s[..idx], &s[idx..])
    } else {
        format!("{}/StatusNotifierItem", s)
    }
}

pub fn spawn() {
    std::thread::spawn(|| {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => { eprintln!("[tray-watcher] rt build: {e}"); return; }
        };
        rt.block_on(async move {
            let watcher = TrayWatcher { items: Arc::new(Mutex::new(HashSet::new())) };
            let conn = zbus::connection::Builder::session()
                .and_then(|b| b.name("org.kde.StatusNotifierWatcher"))
                .and_then(|b| b.serve_at("/StatusNotifierWatcher", watcher))
                .expect("zbus builder")
                .build()
                .await;
            match conn {
                Ok(_c) => {
                    eprintln!("[tray-watcher] org.kde.StatusNotifierWatcher owned");
                    std::future::pending::<()>().await;
                    let _ = _c;
                }
                Err(e) => eprintln!("[tray-watcher] serve failed: {e}"),
            }
        });
    });
}

// ── SNI client side: enumeration, icons, activation ────────────────

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TrayItem {
    pub bus: String,
    pub path: String,
    pub id: String,
    pub title: String,
    pub icon_path: Option<String>,
    pub tooltip: String,
}

pub async fn fetch_tray_items() -> Vec<TrayItem> {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => { eprintln!("[tray-fetch] session bus: {e}"); return Vec::new(); }
    };
    let watcher = match zbus::Proxy::new(
        &conn,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    ).await {
        Ok(p) => p,
        Err(e) => { eprintln!("[tray-fetch] watcher proxy: {e}"); return Vec::new(); }
    };
    let raw_items: Vec<String> = match watcher.get_property("RegisteredStatusNotifierItems").await {
        Ok(v) => v,
        Err(e) => { eprintln!("[tray-fetch] RegisteredStatusNotifierItems: {e}"); return Vec::new(); }
    };

    let mut out = Vec::new();
    for raw in raw_items {
        let Some((bus, path)) = parse_registered(&raw) else { continue };
        let proxy = match zbus::Proxy::new(&conn, bus.as_str(), path.as_str(), "org.kde.StatusNotifierItem").await {
            Ok(p) => p,
            Err(e) => { eprintln!("[tray-fetch] {} proxy: {e}", raw); continue; }
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
        let tooltip = match proxy
            .get_property::<(String, Vec<(i32, i32, Vec<u8>)>, String, String)>("ToolTip")
            .await
        {
            Ok((_icon, _pixmaps, ttl, sub)) => {
                let ttl = ttl.trim().to_string();
                let sub = sub.trim().to_string();
                match (ttl.is_empty(), sub.is_empty()) {
                    (true, true) => String::new(),
                    (false, true) => ttl,
                    (true, false) => sub,
                    (false, false) => format!("{} — {}", ttl, sub),
                }
            }
            Err(_) => String::new(),
        };
        out.push(TrayItem { bus, path, id, title, icon_path, tooltip });
    }
    out
}

pub async fn activate_tray_item(bus: &str, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, bus, path, "org.kde.StatusNotifierItem").await?;
    proxy.call_method("Activate", &(0i32, 0i32)).await?;
    Ok(())
}

pub async fn context_menu_tray_item(bus: &str, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, bus, path, "org.kde.StatusNotifierItem").await?;
    proxy.call_method("ContextMenu", &(0i32, 0i32)).await?;
    Ok(())
}

fn parse_registered(s: &str) -> Option<(String, String)> {
    if let Some(idx) = s.find('/') {
        let (bus, path) = (&s[..idx], &s[idx..]);
        if bus.is_empty() { None } else { Some((bus.to_string(), path.to_string())) }
    } else if !s.is_empty() {
        Some((s.to_string(), "/StatusNotifierItem".to_string()))
    } else {
        None
    }
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
    // SNI IconPixmap is ARGB32 network order: [A, R, G, B]
    for chunk in data.as_chunks::<4>().0 {
        rgba.push(chunk[1]);
        rgba.push(chunk[2]);
        rgba.push(chunk[3]);
        rgba.push(chunk[0]);
    }
    let img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba)?;
    let safe: String = id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let icon_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("mbs-tray");
    let _ = std::fs::create_dir_all(&icon_dir);
    let path = icon_dir.join(format!("mbs-tray-{}.png", safe));
    img.save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}
