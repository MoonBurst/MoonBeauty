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
        // Bound input length + item count: a same-user process calling this in a
        // loop with unique strings would otherwise OOM the bar via HashSet growth.
        const MAX_SERVICE_LEN: usize = 250;
        const MAX_ITEMS: usize = 500;
        if service.len() > MAX_SERVICE_LEN {
            crate::dbg_log!(
                "[tray-watcher] reject oversize service ({} bytes)",
                service.len()
            );
            return Ok(());
        }
        {
            let g = self.items.lock().unwrap();
            if g.len() >= MAX_ITEMS && !g.contains(&normalize_service(&service)) {
                crate::dbg_log!("[tray-watcher] reject registration: at cap ({})", MAX_ITEMS);
                return Ok(());
            }
        }
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
pub enum TrayIconHandle {
    Svg(iced::widget::svg::Handle),
    Rgba(iced::widget::image::Handle),
}

#[derive(Debug, Clone)]
pub struct TrayItem {
    pub bus: String,
    pub path: String,
    pub id: String,
    pub title: String,
    pub icon_path: Option<String>,
    pub icon_handle: Option<TrayIconHandle>,
    pub tooltip: String,
}

fn load_tray_icon_handle(path: &str) -> Option<TrayIconHandle> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return None;
    }
    if path.ends_with(".svg") {
        return Some(TrayIconHandle::Svg(iced::widget::svg::Handle::from_path(
            path,
        )));
    }
    let bytes = std::fs::read(p).ok()?;
    let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
    if !rgba.pixels().any(|px| px[3] > 10) {
        eprintln!(
            "[tray-render] image {:?} is fully transparent, using fallback",
            path
        );
        return None;
    }
    let (w, h) = rgba.dimensions();
    Some(TrayIconHandle::Rgba(
        iced::widget::image::Handle::from_rgba(w, h, rgba.into_raw()),
    ))
}

async fn resolve_icon_handle(
    cache: &[TrayItem],
    icon_path: Option<&str>,
) -> Option<TrayIconHandle> {
    let path = icon_path?;
    if let Some(cached) = cache
        .iter()
        .find(|i| i.icon_path.as_deref() == Some(path))
        .and_then(|i| i.icon_handle.clone())
    {
        return Some(cached);
    }
    let owned = path.to_string();
    tokio::task::spawn_blocking(move || load_tray_icon_handle(&owned))
        .await
        .ok()
        .flatten()
}

pub async fn fetch_tray_items(cache: Vec<TrayItem>) -> Vec<TrayItem> {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[tray-fetch] session bus: {e}");
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
            eprintln!("[tray-fetch] watcher proxy: {e}");
            return Vec::new();
        }
    };
    let raw_items: Vec<String> = match watcher.get_property("RegisteredStatusNotifierItems").await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[tray-fetch] RegisteredStatusNotifierItems: {e}");
            return Vec::new();
        }
    };

    // Cap enumeration so a peer registering many names cannot drive unbounded
    // Proxy::new calls; 200 is ~10x real systems.
    const MAX_ITEMS: usize = 200;
    if raw_items.len() > MAX_ITEMS {
        crate::dbg_log!(
            "[tray-fetch] capping item enumeration {} → {}",
            raw_items.len(),
            MAX_ITEMS
        );
    }
    // Deduplicate raw items by their object path. Steam and other apps
    // leak old bus names, which causes timeouts and blocked fetches.
    let mut seen_paths = std::collections::HashSet::new();
    let mut filtered_raw_items = Vec::new();
    for raw in raw_items {
        let path = if let Some(idx) = raw.find('/') { &raw[idx..] } else { &raw };
        if seen_paths.insert(path.to_string()) {
            filtered_raw_items.push(raw);
        }
    }

    let mut out: Vec<TrayItem> = Vec::new();
    let mut seen_names: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for raw in filtered_raw_items.into_iter().take(MAX_ITEMS) {
        let Some((bus, path)) = parse_registered(&raw) else {
            continue;
        };
        // Per-item timeout (2s): bounds total fetch time when combined with
        // MAX_ITEMS so a hung peer does not stall at the D-Bus default 25s.
        let proxy_result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            zbus::Proxy::new(
                &conn,
                bus.as_str(),
                path.as_str(),
                "org.kde.StatusNotifierItem",
            ),
        )
        .await;
        
        let proxy = match proxy_result {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                eprintln!("[tray-fetch] {} proxy error: {e}", raw);
                if let Some(c) = cache.iter().find(|i| i.bus == bus && i.path == path) {
                    out.push(c.clone());
                }
                continue;
            }
            Err(_) => {
                eprintln!("[tray-fetch] {} proxy timeout", raw);
                if let Some(c) = cache.iter().find(|i| i.bus == bus && i.path == path) {
                    out.push(c.clone());
                }
                continue;
            }
        };
        // Cap SNI strings before display.
        const MAX_ID: usize = 128;
        const MAX_TITLE: usize = 512;
        const MAX_ICON_NAME: usize = 128;
        let id_res = proxy.get_property::<String>("Id").await;
        let id: String = id_res.ok().map(|s| s.chars().take(MAX_ID).collect()).unwrap_or_default();
        
        if id.is_empty() {
            eprintln!("[tray-fetch] empty id for {}, using cache", raw);
            if let Some(c) = cache.iter().find(|i| i.bus == bus && i.path == path) {
                out.push(c.clone());
            }
            continue;
        }

        let mut title: String = proxy
            .get_property::<String>("Title")
            .await
            .ok()
            .map(|s| s.chars().take(MAX_TITLE).collect())
            .unwrap_or_default();



        let icon_name: String = proxy
            .get_property::<String>("IconName")
            .await
            .ok()
            .map(|s| s.chars().take(MAX_ICON_NAME).collect())
            .unwrap_or_default();
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
        let icon_handle = resolve_icon_handle(&cache, icon_path.as_deref()).await;
        let tooltip = match proxy
            .get_property::<(String, Vec<(i32, i32, Vec<u8>)>, String, String)>("ToolTip")
            .await
        {
            Ok((_icon, _pixmaps, ttl, sub)) => {
                let ttl = ttl.trim().to_string();
                let sub = sub.trim().to_string();
                let combined = match (ttl.is_empty(), sub.is_empty()) {
                    (true, true) => String::new(),
                    (false, true) => ttl,
                    (true, false) => sub,
                    (false, false) => format!("{} — {}", ttl, sub),
                };
                const MAX_TOOLTIP: usize = 4096;
                if combined.chars().count() > MAX_TOOLTIP {
                    combined.chars().take(MAX_TOOLTIP - 3).collect::<String>() + "..."
                } else {
                    combined
                }
            }
            Err(_) => String::new(),
        };
        eprintln!("[tray-fetch] item: {} path: {:?}", title, icon_path);
        if title.trim().is_empty() {
            title = id.clone();
        }
        
        // Skip ghost items that have no title and no icon
        if title.trim().is_empty() && icon_path.is_none() {
            eprintln!("[tray-fetch] skipping ghost item: {}", raw);
            continue;
        }

        // Deduplicate: Steam and other apps sometimes register twice
        let title_clean = title.trim().to_string();
        if let Some(&idx) = seen_names.get(&title_clean) {
            if icon_path.is_some() && out[idx].icon_path.is_none() {
                eprintln!("[tray-fetch] replacing duplicate {} with better icon", title_clean);
                out[idx] = TrayItem {
                    bus,
                    path,
                    id,
                    title: title_clean.clone(),
                    icon_path,
                    icon_handle,
                    tooltip,
                };
            }
            continue;
        }
        
        seen_names.insert(title_clean, out.len());
        out.push(TrayItem {
            bus,
            path,
            id,
            title,
            icon_path,
            icon_handle,
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
        depth: usize,
    ) -> Vec<TrayMenuItem> {
        // Cap recursion depth; real dbusmenu trees are 2-4 levels deep and
        // excessive nesting would overflow the stack.
        const MAX_DEPTH: usize = 16;
        if depth > MAX_DEPTH {
            eprintln!(
                "[tray-menu] depth {} exceeds cap {}, truncating",
                depth, MAX_DEPTH
            );
            return Vec::new();
        }
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

        // Cap flat width per level (depth 16 × width 1000 = 16K items max) so a
        // wide single-level response cannot exhaust memory before the depth guard fires.
        const MAX_ITEMS_PER_LEVEL: usize = 1000;
        for c in children {
            if items.len() >= MAX_ITEMS_PER_LEVEL {
                crate::dbg_log!(
                    "[tray-menu] level width cap {} at depth {}",
                    MAX_ITEMS_PER_LEVEL,
                    depth
                );
                break;
            }
            let val: zbus::zvariant::Value = c.into();
            if let Ok((cid, cprops, ckids)) = val.downcast::<RawItem>() {
                items.extend(parse_item(cid, cprops, ckids, depth + 1));
            }
        }
        items
    }

    let items = parse_item(layout.0, layout.1, layout.2, 0);
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

/// Cap the tray icon cache directory at 20 MB. SNI items are untrusted —
/// without a cap, a hostile app could pin arbitrarily many icons into
/// `$XDG_RUNTIME_DIR/mbs-tray` (tmpfs) until session OOM.
/// Cached hits bypass this check; only new resolutions are declined when over budget.
fn tray_cache_budget_ok(dir: &std::path::Path) -> bool {
    const MAX: u64 = 20 * 1024 * 1024;
    let total: u64 = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum();
    if total >= MAX {
        eprintln!(
            "[tray-icon] cache budget exceeded ({}/{}) — refusing new icons",
            total, MAX
        );
        return false;
    }
    true
}

/// Resolve an SNI icon name to a normalized, content-hashed PNG/SVG in
/// `$XDG_RUNTIME_DIR/mbs-tray/`.
///
/// Output filenames include a content hash (`icon-<safe>-<sha8>.png`) so
/// iced's path-keyed image cache invalidates when file contents change.
/// Falls back to `None` (initial letter rendered) if decoding fails.
fn resolve_icon(icon_name: &str) -> Option<String> {
    if icon_name.is_empty() {
        return None;
    }
    // SNI items come from untrusted D-Bus peers; reject absolute paths and traversal.
    if icon_name.starts_with('/')
        || icon_name.contains("..")
        || icon_name.contains('/')
        || icon_name.contains('\\')
    {
        eprintln!("[tray-icon] rejecting unsafe icon_name: {:?}", icon_name);
        return None;
    }
    if !icon_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        eprintln!(
            "[tray-icon] rejecting icon_name with invalid chars: {:?}",
            icon_name
        );
        return None;
    }

    // Cache directory. Content-hashed filenames below ensure iced's
    // internal image-handle cache invalidates whenever the bytes change.
    let runtime_dir = crate::libraries::process::mbs_subdir("mbs-tray");

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
            // Validate that the file is actually a fully decodable image
            // AND that it's larger than a minimum threshold (Steam's is 3KB but structurally broken)
            let is_valid = std::fs::metadata(&cached).map(|m| m.len() > 100).unwrap_or(false)
                && std::fs::read(&cached)
                    .ok()
                    .and_then(|bytes| image::load_from_memory(&bytes).ok())
                    .is_some();
            
            if is_valid {
                return Some(cached.to_string_lossy().into_owned());
            } else {
                eprintln!("[tray-icon] cached file {:?} is corrupt or too small, deleting", cached);
                let _ = std::fs::remove_file(&cached);
            }
        }
    }

    // Refuse new resolutions when the cache is over budget; cached hits above still render.
    if !tray_cache_budget_ok(&runtime_dir) {
        return None;
    }

    // Locate the source file via icon theme search only.
    let source: std::path::PathBuf = {
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
                        // Canonicalize + verify under root to block symlink escapes.
                        if let Some(canon) = crate::libraries::process::safe_icon_candidate(&candidate, root) {
                            found = Some(canon);
                            break 'search;
                        }
                    }
                }
            }
        }
        if found.is_none() {
            'search2: for pix in ["/run/current-system/sw/share/pixmaps", "/usr/share/pixmaps"] {
                let pix_root = std::path::PathBuf::from(pix);
                for ext in &exts {
                    let candidate = pix_root.join(format!("{}.{}", icon_name, ext));
                    if candidate.exists() {
                        if let Some(canon) = crate::libraries::process::safe_icon_candidate(&candidate, &pix_root) {
                            found = Some(canon);
                            break 'search2;
                        }
                    }
                }
            }
        }
        found?
    };

    // Cap source file size before decode so oversized theme assets cannot OOM the shell.
    const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;
    match std::fs::metadata(&source) {
        Ok(meta) if meta.len() > MAX_ICON_BYTES => {
            eprintln!(
                "[tray-icon] {} exceeds {} byte cap ({} bytes), skipping",
                icon_name,
                MAX_ICON_BYTES,
                meta.len()
            );
            return None;
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("[tray-icon] {} metadata failed: {}", icon_name, e);
            return None;
        }
    }

    // Decide destination extension. SVG → keep as SVG (iced has svg handle).
    // Everything else → normalize to PNG.
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

    // ── Raster path: in-process decode + re-encode via `image` crate ──
    // Dimension gate BEFORE decode: a small compressed PNG can expand to hundreds
    // of MB of RGBA; `into_dimensions()` reads only the header so rejecting >1024×1024
    // avoids the pixel allocation.
    if let Ok(bytes) = std::fs::read(&source) {
        let dims = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok());
        match dims {
            Some((w, h)) if w <= 1024 && h <= 1024 => {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let _ = std::fs::remove_file(&base_dest);
                    if img.save(&base_dest).is_ok() {
                        return hash_and_finalize(&base_dest);
                    }
                }
            }
            other => {
                eprintln!(
                    "[tray-icon] {} dimension gate rejected {:?} (cap 1024x1024)",
                    icon_name, other
                );
            }
        }
    }

    eprintln!("[tray-icon] {} `image` crate failed to decode", icon_name);
    None
}

fn save_pixmap(id: &str, w: u32, h: u32, data: &[u8]) -> Option<String> {
    if w == 0 || h == 0 {
        return None;
    }
    if w > 512 || h > 512 {
        eprintln!("[tray-icon] rejecting oversize pixmap {}×{}", w, h);
        return None;
    }
    if data.len() != (w as usize * h as usize * 4) {
        return None;
    }

    let mut rgba = Vec::with_capacity(data.len());
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
    let icon_dir = crate::libraries::process::mbs_subdir("mbs-tray");

    // Append a content hash so Iced's image cache invalidates when the data changes
    use sha2::Digest;
    let hash = sha2::Sha256::digest(data);
    let short = format!("{:x}", hash)[..8].to_string();
    let path = icon_dir.join(format!("mbs-tray-{}-{}.png", safe, short));

    if path.exists() {
        return Some(path.to_string_lossy().into_owned());
    }

    // Write atomically to prevent Iced from reading a half-written file
    let tmp_path = path.with_file_name(format!(".tmp-{}.png", short));
    img.save(&tmp_path).ok()?;
    std::fs::rename(&tmp_path, &path).ok()?;
    Some(path.to_string_lossy().into_owned())
}
