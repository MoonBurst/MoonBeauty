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
            let sender = hdr
                .sender()
                .map(|s| s.as_str().to_string())
                .unwrap_or_default();
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
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
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
            Err(e) => {
                eprintln!("[tray-watcher] rt build: {e}");
                return;
            }
        };
        rt.block_on(async move {
            let watcher = TrayWatcher {
                items: Arc::new(Mutex::new(HashSet::new())),
            };
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

use sha2::Digest;

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
        Err(e) => {
            crate::dbg_log!("[tray-fetch] session bus: {e}");
            return Vec::new();
        }
    };
    let watcher = match zbus::Proxy::new(
        &conn,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            crate::dbg_log!("[tray-fetch] watcher proxy: {e}");
            return Vec::new();
        }
    };
    let raw_items: Vec<String> = match watcher.get_property("RegisteredStatusNotifierItems").await {
        Ok(v) => v,
        Err(e) => {
            crate::dbg_log!("[tray-fetch] RegisteredStatusNotifierItems: {e}");
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for raw in raw_items {
        let Some((bus, path)) = parse_registered(&raw) else {
            continue;
        };
        let proxy = match zbus::Proxy::new(
            &conn,
            bus.as_str(),
            path.as_str(),
            "org.kde.StatusNotifierItem",
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                crate::dbg_log!("[tray-fetch] {} proxy: {e}", raw);
                continue;
            }
        };
        let id: String = proxy.get_property("Id").await.unwrap_or_default();
        let title: String = proxy.get_property("Title").await.unwrap_or_default();
        let status: String = proxy.get_property("Status").await.unwrap_or_default();
        if status.eq_ignore_ascii_case("Passive") {
            continue;
        }

        let icon_name: String = proxy.get_property("IconName").await.unwrap_or_default();
        let mut icon_path = if !icon_name.is_empty() {
            resolve_icon(&icon_name)
        } else {
            None
        };
        if icon_path.is_none() {
            if let Ok(pixmaps) = proxy
                .get_property::<Vec<(i32, i32, Vec<u8>)>>("IconPixmap")
                .await
            {
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
        crate::dbg_log!("[tray-fetch] item: {} path: {:?}", title, icon_path);
        out.push(TrayItem {
            bus,
            path,
            id,
            title,
            icon_path,
            tooltip,
        });
    }
    out
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrayMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
}

pub async fn get_menu_tree(bus: &str, path: &str) -> Option<(String, Vec<TrayMenuItem>)> {
    let conn = zbus::Connection::session().await.ok()?;
    let proxy = zbus::Proxy::new(&conn, bus, path, "org.kde.StatusNotifierItem")
        .await
        .ok()?;

    // Menu property is ObjectPath, not String.
    let menu_obj: zbus::zvariant::OwnedObjectPath = proxy.get_property("Menu").await.ok()?;
    let menu_path: String = menu_obj.as_str().to_string();
    if menu_path.is_empty() {
        return None;
    }

    let menu_proxy = zbus::Proxy::new(&conn, bus, menu_path.as_str(), "com.canonical.dbusmenu")
        .await
        .ok()?;
    let reply = menu_proxy
        .call_method(
            "GetLayout",
            &(0i32, -1i32, vec!["label", "enabled", "visible"]),
        )
        .await
        .ok()?;

    type RawItem = (
        i32,
        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        Vec<zbus::zvariant::OwnedValue>,
    );
    let (_rev, layout): (u32, RawItem) = reply.body().deserialize().ok()?;

    fn parse_item(
        id: i32,
        props: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        children: Vec<zbus::zvariant::OwnedValue>,
    ) -> Vec<TrayMenuItem> {
        let mut items = Vec::new();

        let label = match props.get("label").map(|v| &**v) {
            Some(zbus::zvariant::Value::Str(s)) => s.as_str().to_string(),
            _ => String::new(),
        };
        let enabled = match props.get("enabled").map(|v| &**v) {
            Some(zbus::zvariant::Value::Bool(b)) => *b,
            _ => true,
        };
        let visible = match props.get("visible").map(|v| &**v) {
            Some(zbus::zvariant::Value::Bool(b)) => *b,
            _ => true,
        };
        let label_clean = label.replace('_', "");

        if label_clean == "-" {
            items.push(TrayMenuItem {
                id: -1,
                label: "-".into(),
                enabled: false,
                visible: true,
            });
        } else if !label_clean.is_empty() {
            items.push(TrayMenuItem {
                id,
                label: label_clean,
                enabled,
                visible,
            });
        }

        for c in children {
            let val: zbus::zvariant::Value = c.into();
            if let Ok((cid, cprops, ckids)) = val.downcast::<RawItem>() {
                items.extend(parse_item(cid, cprops, ckids));
            }
        }
        items
    }

    let items = parse_item(layout.0, layout.1, layout.2);
    eprintln!("[tray-menu] found {} items", items.len());
    Some((menu_path, items))
}

pub async fn trigger_menu_item(
    bus: &str,
    menu_path: &str,
    id: i32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, bus, menu_path, "com.canonical.dbusmenu").await?;
    proxy
        .call_method(
            "Event",
            &(id, "clicked", zbus::zvariant::Value::from(0i32), 0u32),
        )
        .await?;
    Ok(())
}

pub async fn activate_tray_item(
    bus: &str,
    path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, bus, path, "org.kde.StatusNotifierItem").await?;
    proxy.call_method("Activate", &(0i32, 0i32)).await?;
    Ok(())
}

fn parse_registered(s: &str) -> Option<(String, String)> {
    if let Some(idx) = s.find('/') {
        let (bus, path) = (&s[..idx], &s[idx..]);
        if bus.is_empty() {
            None
        } else {
            Some((bus.to_string(), path.to_string()))
        }
    } else if !s.is_empty() {
        Some((s.to_string(), "/StatusNotifierItem".to_string()))
    } else {
        None
    }
}

/// Resolve an SNI icon name to a normalized, content-hashed PNG/SVG in
/// `$XDG_RUNTIME_DIR/mbs-tray/`.
///
/// **When to use this:** for tray icons (SNI items registered with
/// `org.kde.StatusNotifierWatcher`) and any other caller that needs a
/// **render-stable** icon path for the whole session.
///
/// **Difference from `overlays::notifications::resolve_icon_name`:**
///   - Raster icons are ALWAYS re-encoded through ImageMagick
///     (`-strip`, `png:color-type=6`). This strips malformed chunks
///     (e.g. Steam's broken iCCP) that iced's strict decoder rejects
///     but libpng-adjacent decoders tolerate. Tray slots are rendered
///     continuously for the whole session — a broken decode means a
///     permanently blank slot.
///   - The output filename gets a content hash (`icon-<safe>-<sha8>.png`).
///     Iced's image cache is keyed on path; without a hash, changing
///     file contents in place leaves iced serving stale 1x1 decodes.
///   - SVG files bypass ImageMagick (iced has native SVG support),
///     but still get hashed for the same cache-bust reason.
///   - The fast path (`$XDG_RUNTIME_DIR/mbs-tray/icon-<safe>.<ext>` exists
///     and has non-zero size) skips all of the above; subsequent refetches
///     of the same icon are O(1) stat calls.
///
/// **Cost vs notifications:** one ImageMagick subprocess per *new* icon,
/// amortized to zero across the session. Notifications skip this entirely
/// because they're transient — a broken avatar just falls back to the
/// default icon on the next toast.
///
/// **Fallback chain** if the source can't be decoded after re-encode:
///   → return `None`, renderer shows the initial letter (from `title`,
///     falling back to `id` and then `tooltip`).
fn resolve_icon(icon_name: &str) -> Option<String> {
    use std::path::Path;
    if icon_name.is_empty() {
        return None;
    }

    // Cache directory. Content-hashed filenames below ensure iced's
    // internal image-handle cache invalidates whenever the bytes change.
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("mbs-tray");
    let _ = std::fs::create_dir_all(&runtime_dir);

    let safe_name: String = icon_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Fast path: if a previously-normalized version is already cached, reuse.
    // Files are named "icon-<safe>.<ext>" — we check all possible extensions.
    for ext in ["png", "svg", "xpm"] {
        let cached = runtime_dir.join(format!("icon-{}.{}", safe_name, ext));
        if cached.exists()
            && std::fs::metadata(&cached)
                .map(|m| m.len() > 0)
                .unwrap_or(false)
        {
            return Some(cached.to_string_lossy().into_owned());
        }
    }

    // Locate the source file
    let source: std::path::PathBuf = if icon_name.starts_with('/') {
        if !Path::new(icon_name).exists() {
            return None;
        }
        std::path::PathBuf::from(icon_name)
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let roots = [
            std::path::PathBuf::from("/run/current-system/sw/share/icons/hicolor"),
            std::path::PathBuf::from("/usr/share/icons/hicolor"),
            std::path::PathBuf::from("/usr/local/share/icons/hicolor"),
            home.join(".local/share/icons/hicolor"),
            home.join(".local/share/flatpak/exports/share/icons/hicolor"),
            home.join(".nix-profile/share/icons/hicolor"),
        ];
        let sizes = [
            "48x48", "64x64", "128x128", "256x256", "512x512", "scalable", "symbolic",
        ];
        let exts = ["png", "svg", "xpm"];

        let mut found: Option<std::path::PathBuf> = None;
        'search: for root in &roots {
            for size in &sizes {
                for ext in &exts {
                    let candidate = root
                        .join(size)
                        .join("apps")
                        .join(format!("{}.{}", icon_name, ext));
                    if candidate.exists() {
                        found = Some(candidate);
                        break 'search;
                    }
                }
            }
        }
        if found.is_none() {
            'search2: for pix in ["/run/current-system/sw/share/pixmaps", "/usr/share/pixmaps"] {
                for ext in &exts {
                    let candidate =
                        std::path::PathBuf::from(pix).join(format!("{}.{}", icon_name, ext));
                    if candidate.exists() {
                        found = Some(candidate);
                        break 'search2;
                    }
                }
            }
        }
        found?
    };

    // Decide destination extension. SVG → keep as SVG (iced has svg handle).
    // Everything else → normalize to PNG (ImageMagick rewrites cleanly).
    let src_ext = source.extension().and_then(|s| s.to_str()).unwrap_or("png");
    let is_svg_src = src_ext.eq_ignore_ascii_case("svg");

    // Base dest paths (no hash yet)
    let base_dest = if is_svg_src {
        runtime_dir.join(format!("icon-{}.svg", safe_name))
    } else {
        runtime_dir.join(format!("icon-{}.png", safe_name))
    };

    // Helper: rename file to include a content hash, invalidating iced's
    // image cache whenever bytes change. Returns the final path.
    let hash_and_finalize = |path: &std::path::Path| -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        let hash = sha2::Sha256::digest(&bytes);
        let hex = format!("{:x}", hash);
        let short = &hex[..8];
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png");
        let stem = path.file_stem().and_then(|s| s.to_str())?;
        let final_name = format!("{}-{}.{}", stem, short, ext);
        let final_path = path.with_file_name(final_name);
        if final_path != path {
            std::fs::rename(path, &final_path).ok()?;
        }
        Some(final_path.to_string_lossy().into_owned())
    };

    // ImageMagick availability check (cached once per call for simplicity;
    // it's cheap — just which())
    let magick = if std::process::Command::new("magick")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("magick")
    } else if std::process::Command::new("convert")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("convert")
    } else {
        None
    };

    // ── SVG path: no re-encode, just copy ──────────────────────────
    if is_svg_src {
        match std::fs::copy(&source, &base_dest) {
            Ok(_) => return hash_and_finalize(&base_dest),
            Err(e) => {
                eprintln!("[tray-icon] {} svg copy failed: {}", icon_name, e);
                return None;
            }
        }
    }

    // ── Raster path: ALWAYS normalize via ImageMagick ──────────────
    // Rationale: some shipped PNGs (notably Steam's steam_tray_mono.png)
    // are structurally valid enough that `image` crate's header parser
    // accepts them, but iced's render pipeline fails at pixel data (or
    // fails to render silently). Re-encoding through ImageMagick strips
    // malformed chunks (broken iCCP, etc.) and guarantees a clean PNG
    // iced can render.
    // Try to re-encode using the `image` crate first (fast, no subprocess).
    // The `image` crate is lenient with malformed chunks (like Steam's
    // broken iCCP) and saving it writes a clean PNG.
    if let Ok(bytes) = std::fs::read(&source) {
        if let Ok(img) = image::load_from_memory(&bytes) {
            // Overwrite base_dest with a clean PNG
            let _ = std::fs::remove_file(&base_dest);
            if img.save(&base_dest).is_ok() {
                return hash_and_finalize(&base_dest);
            }
        }
    }

    eprintln!(
        "[tray-icon] {} `image` crate failed to decode; falling back to magick",
        icon_name
    );

    if let Some(bin) = magick {
        let _ = std::fs::remove_file(&base_dest);
        let output = std::process::Command::new(bin)
            .args([
                source.to_str().unwrap_or(""),
                "-strip",
                base_dest.to_str().unwrap_or(""),
            ])
            .output();
        match output {
            Ok(out) if out.status.success() => {
                if base_dest.exists()
                    && std::fs::metadata(&base_dest)
                        .map(|m| m.len() > 0)
                        .unwrap_or(false)
                {
                    return hash_and_finalize(&base_dest);
                }
                eprintln!(
                    "[tray-icon] {} magick produced empty output. stderr: {}",
                    icon_name,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Ok(out) => eprintln!(
                "[tray-icon] {} magick exited {:?}. stderr: {}",
                icon_name,
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(e) => eprintln!("[tray-icon] {} magick spawn: {}", icon_name, e),
        }
    } else {
        eprintln!(
            "[tray-icon] {} no ImageMagick; copying raw (may not render)",
            icon_name
        );
    }

    // Fallback: raw copy (may fail to render, but at least icon_path exists)
    match std::fs::copy(&source, &base_dest) {
        Ok(_) => hash_and_finalize(&base_dest),
        Err(_) => None,
    }
}

fn save_pixmap(id: &str, w: u32, h: u32, data: &[u8]) -> Option<String> {
    if w == 0 || h == 0 {
        return None;
    }
    if data.len() != (w as usize * h as usize * 4) {
        return None;
    }
    let mut rgba = Vec::with_capacity(data.len());
    // SNI IconPixmap is ARGB32 network order: [A, R, G, B]
    for chunk in data.as_chunks::<4>().0 {
        rgba.push(chunk[1]);
        rgba.push(chunk[2]);
        rgba.push(chunk[3]);
        rgba.push(chunk[0]);
    }
    let img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba)?;
    let safe: String = id
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let icon_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("mbs-tray");
    let _ = std::fs::create_dir_all(&icon_dir);
    let path = icon_dir.join(format!("mbs-tray-{}.png", safe));
    img.save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}
