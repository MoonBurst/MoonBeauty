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
        // Finding 54: bound input length + item count. A same-user process
        // can call this in a loop with unique strings; unbounded HashSet
        // growth OOMs the bar → lockscreen children die.
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
                crate::dbg_log!(
                    "[tray-watcher] reject registration: at cap ({})",
                    MAX_ITEMS
                );
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

    // Finding 56: cap enumeration. Chained from finding 54 — a hostile
    // process that registered N names now drives N Proxy::new calls,
    // each blocking up to 25s on timeout. 200 is 10× real systems; caps
    // worst-case to 200 * 25s = 83 min. Combined with the refetch
    // cadence, only one stalled fetch at a time.
    const MAX_ITEMS: usize = 200;
    if raw_items.len() > MAX_ITEMS {
        crate::dbg_log!(
            "[tray-fetch] capping item enumeration {} → {}",
            raw_items.len(), MAX_ITEMS
        );
    }
    let mut out = Vec::new();
    for raw in raw_items.into_iter().take(MAX_ITEMS) {
        let Some((bus, path)) = parse_registered(&raw) else {
            continue;
        };
        // Finding 56: per-item timeout (3s). Bounds the total fetch time
        // when combined with MAX_ITEMS. A hung peer no longer stalls the
        // whole enumeration at the D-Bus default 25s.
        let proxy_result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
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
                crate::dbg_log!("[tray-fetch] {} proxy: {e}", raw);
                continue;
            }
            Err(_) => {
                crate::dbg_log!("[tray-fetch] {} proxy: timeout", raw);
                continue;
            }
        };
        // Finding 38: cap SNI strings before display.
        const MAX_ID: usize = 128;
        const MAX_TITLE: usize = 512;
        const MAX_ICON_NAME: usize = 128;
        let id: String = proxy
            .get_property::<String>("Id")
            .await
            .ok()
            .map(|s| s.chars().take(MAX_ID).collect())
            .unwrap_or_default();
        let title: String = proxy
            .get_property::<String>("Title")
            .await
            .ok()
            .map(|s| s.chars().take(MAX_TITLE).collect())
            .unwrap_or_default();
        let status: String = proxy.get_property("Status").await.unwrap_or_default();
        if status.eq_ignore_ascii_case("Passive") {
            continue;
        }

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
        depth: usize,
    ) -> Vec<TrayMenuItem> {
        // Finding 34: cap recursion depth. Real dbusmenu trees are 2-4
        // levels; a hostile provider returns 10000-deep nesting → stack
        // overflow → shell crash → lockscreen child dies → unlock window.
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

        // Finding 57: cap flat width per level. Real dbusmenu trees are
        // <20 items wide; a hostile single-level 1M-children response
        // allocates 1M TrayMenuItem before depth guard (34) can fire.
        // Cap at 1000 per level — recursion depth 16 with cap 1000 means
        // worst case is 16 × 1000 = 16K items in memory regardless of
        // tree shape.
        const MAX_ITEMS_PER_LEVEL: usize = 1000;
        for c in children {
            if items.len() >= MAX_ITEMS_PER_LEVEL {
                crate::dbg_log!(
                    "[tray-menu] level width cap {} at depth {}",
                    MAX_ITEMS_PER_LEVEL, depth
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
/// Finding 06: cap the tray icon cache directory at 20 MB. SNI items are
/// untrusted — without a cap, a hostile app pins arbitrarily many icons
/// into $XDG_RUNTIME_DIR/mbs-tray (tmpfs), growing until session OOM.
/// Cached hits bypass this (they're already counted); only new resolves
/// are declined when over budget.
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

/// Finding 27: canonicalize an icon candidate and verify it stays under
/// its declared root. Blocks symlink-to-sensitive-file: `~/.local/share/
/// icons/hicolor/48x48/apps/foo.png -> /etc/passwd` is rejected rather
/// than handed to `std::fs::read` + `image::load_from_memory`.
///
/// Both canonicalizations can fail (broken symlink, race, missing root);
/// caller treats `None` as "try next candidate", which is the safe default.
fn safe_icon_candidate(
    candidate: &std::path::Path,
    root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let canon = std::fs::canonicalize(candidate).ok()?;
    let canon_root = std::fs::canonicalize(root).ok()?;
    // Finding 64: allow canonical targets under /nix/store/ (NixOS icons
    // are symlinks into the immutable store). Hostile symlinks to
    // user/system paths outside the store and outside the declared root
    // are still rejected.
    const NIX_STORE: &str = "/nix/store/";
    if canon.starts_with(&canon_root) || canon.starts_with(NIX_STORE) {
        Some(canon)
    } else {
        eprintln!(
            "[tray-icon] symlink escape blocked: {:?} -> {:?}",
            candidate, canon
        );
        None
    }
}

fn resolve_icon(icon_name: &str) -> Option<String> {
    if icon_name.is_empty() {
        return None;
    }
    // Finding 09: SNI items come from untrusted D-Bus peers. Reject
    // absolute paths (a read primitive via existence check + decode).
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
        eprintln!("[tray-icon] rejecting icon_name with invalid chars: {:?}", icon_name);
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
            return Some(cached.to_string_lossy().into_owned());
        }
    }

    // Finding 06: refuse NEW resolutions when the cache is over budget.
    // Cached hits above already returned; existing icons stay visible.
    if !tray_cache_budget_ok(&runtime_dir) {
        return None;
    }

    // Locate the source file — theme search only (finding 09).
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
                        // Finding 27: canonicalize + verify under root.
                        if let Some(canon) = safe_icon_candidate(&candidate, root) {
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
                        if let Some(canon) = safe_icon_candidate(&candidate, &pix_root) {
                            found = Some(canon);
                            break 'search2;
                        }
                    }
                }
            }
        }
        found?
    };

    // Finding 27: cap source file size before decode. `image::load_from_memory`
    // fully decodes into memory; a 500 MB PNG in an icon theme would OOM
    // the shell before any post-decode check could fire. 4 MB is generous
    // for a real tray icon (typical: 2-40 KB).
    const MAX_ICON_BYTES: u64 = 4 * 1024 * 1024;
    match std::fs::metadata(&source) {
        Ok(meta) if meta.len() > MAX_ICON_BYTES => {
            eprintln!(
                "[tray-icon] {} exceeds {} byte cap ({} bytes), skipping",
                icon_name, MAX_ICON_BYTES, meta.len()
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

    // Finding 07: ImageMagick availability probe removed entirely.
    // The tray renderer now has zero subprocess decoders.

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
    // Finding 27-residual: dimension gate BEFORE decode. A 4 MB PNG can
    // decompress to hundreds of MB of RGBA (bomb ratio ~1000:1). The
    // 4 MB input cap (batch 4) bounds the compressed size but not the
    // decoded output. `into_dimensions()` reads only the header —
    // typically <1KB — so rejecting early avoids the pixel allocation.
    // Real tray icons are ≤256×256; 1024×1024 gives 4× headroom.
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

    // Finding 07: no ImageMagick fallback. `image` crate is the only
    // decoder. If it fails, we return None and the renderer shows the
    // item title's first letter. No subprocess, no RCE surface.
    eprintln!(
        "[tray-icon] {} `image` crate failed to decode; no fallback (finding 07)",
        icon_name
    );
    None
}

fn save_pixmap(id: &str, w: u32, h: u32, data: &[u8]) -> Option<String> {
    if w == 0 || h == 0 {
        return None;
    }
    // Finding 06: hard dimension cap. SNI items are attacker-controlled.
    // Real tray icons are ≤256×256; 512×512 gives generous headroom.
    // Without this, a hostile IconPixmap (10000×10000 × 4 = 400MB) hits
    // Vec::with_capacity() below and OOMs the process.
    if w > 512 || h > 512 {
        eprintln!("[tray-icon] rejecting oversize pixmap {}×{}", w, h);
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
    let icon_dir = crate::libraries::process::mbs_subdir("mbs-tray");
    let path = icon_dir.join(format!("mbs-tray-{}.png", safe));
    img.save(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}
