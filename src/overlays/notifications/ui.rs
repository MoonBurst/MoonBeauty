use chrono::Local;
use iced::widget::{button, column, container, pin, row, scrollable, text, MouseArea, Stack};
use iced::{Alignment, Color, Element, Length, Padding};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::{hex_to_color, ShellTheme};
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::libraries::style::TextExt;

// ── Icon resolution from desktop entries ──────────────────────────
//
// Real system notifications don't carry file paths — they carry a
// `desktop_entry` hint like "firefox" or "org.gnome.Nautilus".
// Resolve that to the actual icon PNG/SVG by:
//   1. Finding the .desktop file
//   2. Parsing its Icon= field
//   3. Locating the icon in the theme search paths
//
// Falls back to app_name-based lookup if desktop_entry is missing.

/// Search the standard freedesktop icon theme dirs for a file named
/// `<name>.{png,svg,xpm}`.
///
/// **When to use this:** for notification avatars (chat app icons, email
/// client icons) and anywhere else that has an icon *name* to resolve
/// (e.g. from a `.desktop` file's `Icon=` field).
///
/// **Difference from `bar::tray::watcher::resolve_icon`:**
///   - This function does NOT normalize the icon bytes. It returns the
///     source path directly.
///   - The tray version ALWAYS runs the file through ImageMagick
///     (`-strip`) and appends a content hash, because iced's image
///     cache is keyed on path and the tray's files live for the whole
///     session. Notifications are rendered once per toast — a cache
///     hit is less critical, so we skip the normalization cost.
///   - The tray version is stricter about decode validity because a
///     broken icon means a permanently-broken capsule slot; a broken
///     notification avatar just falls back to the default icon.
///
/// **Theme coverage:** hicolor first, then Adwaita/breeze/Papirus/Yaru/
/// Numix. Also `/usr/share/pixmaps` and Nix paths for flat/old apps.
fn resolve_icon_name(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    // Absolute path: only accept under a known icon root (finding 08).
    if name.starts_with('/') {
        let known = [
            "/usr/share/icons/",
            "/usr/share/pixmaps/",
            "/usr/local/share/icons/",
            "/usr/local/share/pixmaps/",
            "/run/current-system/sw/share/icons/",
            "/run/current-system/sw/share/pixmaps/",
            "/var/lib/flatpak/exports/share/icons/",
            "/var/lib/flatpak/exports/share/pixmaps/",
            "/var/lib/snapd/desktop/icons/",
        ];
        let mut ok = known.iter().any(|p| name.starts_with(p));
        if !ok {
            if let Some(home) = dirs::home_dir() {
                let h = home.to_string_lossy();
                ok = name.starts_with(&format!("{}/.local/share/icons/", h))
                    || name.starts_with(&format!("{}/.nix-profile/share/icons/", h))
                    || name
                        .starts_with(&format!("{}/.local/share/flatpak/exports/share/icons/", h));
            }
        }
        if !ok {
            crate::dbg_log!(
                "[notif-icon] reject absolute icon path outside known roots: {:?}",
                name
            );
            return None;
        }
        return if std::path::Path::new(name).exists() {
            Some(name.to_string())
        } else {
            None
        };
    }
    // Relative name: reject separators and traversal.
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        crate::dbg_log!("[notif-icon] reject unsafe icon name: {:?}", name);
        return None;
    }
    let home = dirs::home_dir()?;
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());

    // Theme roots: hicolor first, then common alt themes, then flatpak
    // exports. Some app packs only install into a non-hicolor theme.
    let theme_roots = vec![
        PathBuf::from("/etc/profiles/per-user")
            .join(&user)
            .join("share/icons"),
        PathBuf::from("/run/current-system/sw/share/icons"),
        PathBuf::from("/usr/share/icons"),
        home.join(".local/share/icons"),
        home.join(".nix-profile/share/icons"),
        PathBuf::from("/var/lib/flatpak/exports/share/icons"),
        home.join(".local/share/flatpak/exports/share/icons"),
    ];
    let themes = [
        "hicolor",
        "Adwaita",
        "breeze",
        "Papirus",
        "Papirus-Dark",
        "Yaru",
        "Numix",
    ];
    let sizes = [
        "256x256", "128x128", "512x512", "64x64", "48x48", "32x32", "scalable", "symbolic",
    ];
    let exts = ["png", "svg", "xpm"];

    // Try each (root, theme, size, ext) combination. Stop at first hit.
    for root in &theme_roots {
        for theme in &themes {
            for size in &sizes {
                for ext in &exts {
                    let p = root
                        .join(theme)
                        .join(size)
                        .join("apps")
                        .join(format!("{}.{}", name, ext));
                    if p.exists() {
                        return Some(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }

    // Non-themed flat dirs (older apps, system icons)
    let flat_dirs = vec![
        PathBuf::from("/run/current-system/sw/share/pixmaps"),
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/etc/profiles/per-user")
            .join(&user)
            .join("share/pixmaps"),
        home.join(".nix-profile/share/pixmaps"),
    ];
    for dir in &flat_dirs {
        for ext in &exts {
            let p = dir.join(format!("{}.{}", name, ext));
            if p.exists() {
                return Some(p.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Given a desktop-entry hint (e.g. `"firefox"`, `"org.mozilla.firefox"`),
/// find its `.desktop` file under the standard applications dirs, read
/// the `Icon=` field from `[Desktop Entry]`, then resolve that icon name
/// via `resolve_icon_name`.
///
/// **When to use this:** when the notification's `desktop_entry` hint is
/// available (from the `desktop-entry` hint field of the D-Bus Notify
/// call). This is the most reliable way to get an app's *actual* icon —
/// SNI/tray apps sometimes use a different icon than the launcher entry,
/// but for notifications the desktop-entry is authoritative.
///
/// **Fallback order** for notification avatars in push_notification:
///   1. Friend profile avatar (`resources/<name>/<name>.png`)
///   2. `resources/fallback.png`
///   3. `resolve_desktop_icon(desktop_entry)` — this function
///   4. `resolve_icon_name(app_name)` — pure name lookup
///
/// So this is called before the name-only fallback because a matching
/// `.desktop` file gives higher confidence than a guessed icon name.
fn resolve_desktop_icon(entry: &str) -> Option<String> {
    let clean = entry.trim_end_matches(".desktop").trim();
    if clean.is_empty() {
        return None;
    }
    // Reject traversal and absolute paths in desktop_entry hint (finding 08).
    if clean.starts_with('/') || clean.contains("..") || clean.contains('/') || clean.contains('\\')
    {
        crate::dbg_log!("[notif-icon] reject unsafe desktop_entry: {:?}", entry);
        return None;
    }

    let home = dirs::home_dir()?;
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let dirs = vec![
        PathBuf::from("/etc/profiles/per-user")
            .join(&user)
            .join("share/applications"),
        PathBuf::from("/run/current-system/sw/share/applications"),
        PathBuf::from("/usr/share/applications"),
        home.join(".local/share/applications"),
        home.join(".nix-profile/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
    ];

    // Try direct filename matches first (fast path). Cover common variants:
    //   firefox          → firefox.desktop
    //   org.mozilla.firefox → org.mozilla.firefox.desktop
    //   org.mozilla.firefox.desktop → already has extension
    let candidates = [
        format!("{}.desktop", clean),
        format!("{}.desktop", clean.to_lowercase()),
    ];

    for dir in &dirs {
        for fname in &candidates {
            let path = dir.join(fname);
            if let Some(icon) = read_icon_from_desktop(&path) {
                return Some(icon);
            }
        }
    }

    // Fallback: scan ALL .desktop files in these dirs and look for one
    // whose Name= (case-insensitive) matches `clean` or whose filename
    // stem equals `clean`. Handles cases like app_name="Mozilla Firefox"
    // matching "firefox.desktop" whose Name="Firefox".
    let lower_target = clean.to_lowercase();
    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let stem_matches = stem == lower_target
                || stem.ends_with(&format!(".{}", lower_target))
                || lower_target.ends_with(&format!(".{}", stem));

            // Read the Name= field from the main section
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut name_match = false;
            let mut in_main = false;
            let mut icon_line: Option<String> = None;
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('[') {
                    in_main = line.eq_ignore_ascii_case("[Desktop Entry]");
                    continue;
                }
                if !in_main {
                    continue;
                }
                if let Some(v) = line.strip_prefix("Name=") {
                    let n = v.trim().to_lowercase();
                    if n == lower_target || lower_target.contains(&n) || n.contains(&lower_target) {
                        name_match = true;
                    }
                }
                if let Some(v) = line.strip_prefix("Icon=") {
                    if icon_line.is_none() {
                        icon_line = Some(v.trim().to_string());
                    }
                }
            }
            if !(stem_matches || name_match) {
                continue;
            }
            if let Some(icon_name) = icon_line {
                if let Some(p) = resolve_icon_name(&icon_name) {
                    return Some(p);
                }
            }
        }
    }

    // Last resort: use the entry itself as an icon name (covers cases
    // where the icon exists but no .desktop file does).
    resolve_icon_name(clean)
}

/// Helper: read Icon= from a specific .desktop file's [Desktop Entry]
/// section, then resolve the icon name to a path.
fn read_icon_from_desktop(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_main = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line.eq_ignore_ascii_case("[Desktop Entry]");
            continue;
        }
        if !in_main {
            continue;
        }
        if let Some(v) = line.strip_prefix("Icon=") {
            let icon_name = v.trim();
            if icon_name.is_empty() {
                continue;
            }
            return resolve_icon_name(icon_name);
        }
    }
    None
}

// ── History row geometry (single source of truth) ─────────────────
// Used by both view_history() and scroll_offset_for_selected() so
// scroll always lands on the correct row regardless of text length.
// Icon size is now dynamic — passed in from settings.launcher_icon_size
// so the history icons match the launcher exactly.
const HISTORY_ROW_PAD_X: f32 = 20.0;
const HISTORY_ROW_PAD_Y: f32 = 12.0;
const HISTORY_ROW_GAP: f32 = 10.0;
const HISTORY_ROW_MIN_H: f32 = 72.0;

/// Hexagon wrap size around an icon. Must satisfy:
///   interior = hex_size - 2*cut ≥ icon_size
///   cut = 10% of hex_size  →  interior = 0.80 * hex_size
///   hex_size ≥ 1.25 * icon_size.
/// Use 1.40 for visible margin.
fn history_icon_hex_size(icon_size: f32) -> f32 {
    icon_size * 1.40
}

fn history_row_height(
    toast: &NotificationToast,
    row_w: f32,
    fs: f32,
    fs_sm: f32,
    icon_size: f32,
) -> f32 {
    let icon_hex_size = history_icon_hex_size(icon_size);
    let text_area_w = (row_w - icon_hex_size - 16.0 - HISTORY_ROW_PAD_X * 2.0).max(100.0);
    let char_w = (fs_sm * 0.55).max(6.0);
    let chars_per_line = (text_area_w / char_w).max(20.0) as usize;
    let body_len = toast.body.chars().count();
    let body_lines = body_len.div_ceil(chars_per_line).max(1);
    let header_h = fs * 1.2;
    let line_h = fs_sm * 1.2;
    let text_h =
        HISTORY_ROW_PAD_Y + header_h + 4.0 + (body_lines as f32) * line_h + HISTORY_ROW_PAD_Y;
    // Row must fit the icon hexagon + padding, otherwise the hexagon
    // extends past the row background and looks like it's "breaking out".
    let icon_h = icon_hex_size + HISTORY_ROW_PAD_Y * 2.0;
    text_h.max(HISTORY_ROW_MIN_H).max(icon_h)
}

use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

/// IDs we've already logged icon decisions for. Prevents per-frame
/// log spam (view_history is called on every Tick message, ~1/sec).
static LOGGED_ICONS: Mutex<Option<HashSet<u32>>> = Mutex::new(None);

fn should_log_icon(id: u32) -> bool {
    let mut guard = match LOGGED_ICONS.lock() {
        Ok(g) => g,
        Err(_) => return true, // poisoned → log
    };
    let set = guard.get_or_insert_with(HashSet::new);
    set.insert(id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationToast {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub timestamp: String,
    pub is_read: bool,
    pub is_urgent: bool,
    pub border_color: String,
    #[serde(default = "default_toast_text_color")]
    pub text_color: String,
    #[serde(default)]
    pub avatar_path: Option<String>,
    #[serde(default)]
    pub desktop_entry: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip)]
    pub exit_started_at: Option<Instant>,
    #[serde(skip)]
    pub hold_reset_at: Option<Instant>,
    #[serde(skip)]
    pub slot_shift_start: Option<Instant>,
    #[serde(default)]
    pub tts_summary: String,
    #[serde(default)]
    pub tts_body: String,
    #[serde(default)]
    pub tts_readout: Option<String>,
    #[serde(default)]
    pub tts_command: Option<String>,
    #[serde(default)]
    pub tts_pending: bool,
    #[serde(default)]
    pub tts_spoken: bool,
}

fn default_toast_text_color() -> String {
    "#f7f700".to_string()
}

#[derive(Debug, Clone)]
pub enum NotifMessage {
    DismissToast(u32),
    ActivateToast(u32),
    ClearHistory,
    DeleteHistory(u32),
    HistorySelectNext,
    HistorySelectPrev,
    HistoryDeleteSelected,
    HistoryScrollTo(f32),
    ToggleHistoryDrawer,
    DismissDrawer,
}

pub struct NotificationOverlay {
    pub active_toasts: Vec<NotificationToast>,
    /// Overflow queue. When `active_toasts` hits the visible limit (4),
    /// new toasts land here instead of evicting the oldest. `tick_toasts`
    /// pulls from the front of this queue whenever an active toast
    /// expires or is dismissed, restoring the "wait your turn" behavior.
    pub pending_toasts: VecDeque<NotificationToast>,
    pub history: Vec<NotificationToast>,
    pub show_history: bool,
    pub stack_target_count: usize,
    pub stack_from_count: usize,
    pub stack_anim_start: Instant,
    pub selected_history_id: Option<u32>,
    pub last_saved: Instant,
    pub history_dirty: bool,
}

impl Default for NotificationOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationOverlay {
    pub fn new() -> Self {
        let history = Self::load_history();
        let selected_history_id = history.first().map(|t| t.id);
        Self {
            active_toasts: Vec::new(),
            pending_toasts: VecDeque::new(),
            history,
            show_history: false,
            stack_target_count: 0,
            stack_from_count: 0,
            stack_anim_start: Instant::now(),
            selected_history_id,
            last_saved: Instant::now(),
            history_dirty: false,
        }
    }

    pub fn empty() -> Self {
        Self {
            active_toasts: Vec::new(),
            pending_toasts: VecDeque::new(),
            history: Vec::new(),
            show_history: false,
            stack_target_count: 0,
            stack_from_count: 0,
            stack_anim_start: Instant::now(),
            selected_history_id: None,
            last_saved: Instant::now(),
            history_dirty: false,
        }
    }

    fn history_path() -> PathBuf {
        // Finding 23: OTP codes / private URLs must not land in world-readable /tmp.
        dirs::cache_dir()
            .expect("XDG_CACHE_HOME or HOME must be set; refusing to persist notification history to /tmp")
            .join("moonbeauty_notification_history.json")
    }

    /// Compute the pixel offset needed to scroll the given history
    /// item into view, using the actual computed row heights. Result is
    /// fed directly into iced's `scroll_to` AbsoluteOffset.
    ///
    /// Returns None if no item is selected. Result is measured from the
    /// top of the scrollable's content; view_history uses the same
    /// row geometry so the two always agree.
    pub fn scroll_offset_for_selected(
        &self,
        theme: &ShellTheme,
        settings: &crate::libraries::config::settings::ShellSettings,
    ) -> Option<f32> {
        let id = self.selected_history_id?;

        let (win_w, _) = settings.get_window_size("history", 960, 720);
        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_w * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let header_inset = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let body_inset = header_inset + 4.0;
        let row_w = win_w - body_inset * 2.0;
        let fs = theme.font_size_overlay as f32;
        let fs_sm = fs * 0.85;
        let icon_size = settings.launcher_icon_size as f32;

        let mut y: f32 = 0.0;
        for toast in self.history.iter().rev() {
            if toast.id == id {
                // Show a bit of context above the selected row.
                let context = (history_icon_hex_size(icon_size)).max(80.0);
                return Some((y - context).max(0.0));
            }
            y += history_row_height(toast, row_w, fs, fs_sm, icon_size) + HISTORY_ROW_GAP;
        }
        Some(y)
    }

    pub fn load_history() -> Vec<NotificationToast> {
        let path = Self::history_path();
        // Finding 45: file-size pre-check. A pre-fix history file with
        // 8 GB of notifications would OOM on read_to_string + serde +
        // to_string_pretty-for-migration. Reset if over cap.
        const MAX_HISTORY_BYTES: u64 = 8 * 1024 * 1024;
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > MAX_HISTORY_BYTES => {
                eprintln!(
                    "[notif] history file {} bytes exceeds {} cap, resetting",
                    m.len(),
                    MAX_HISTORY_BYTES
                );
                let _ = std::fs::remove_file(&path);
                return Vec::new();
            }
            Ok(_) => {}
            Err(_) => return Vec::new(),
        }
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(mut items) = serde_json::from_str::<Vec<NotificationToast>>(&raw) {
                    // Migrate: entries saved before V354 have no avatar_path
                    // (or have a stale one from the old weak resolver).
                    // Retry with the improved chain.
                    let mut migrated = false;
                    let mut still_missing = 0;
                    for item in items.iter_mut() {
                        if item.avatar_path.is_some() {
                            continue;
                        }
                        let lower = item.app_name.to_lowercase();
                        let variants = [
                            lower.clone(),
                            lower.replace(' ', "-"),
                            lower.replace(' ', "_"),
                            lower.split_whitespace().next().unwrap_or("").to_string(),
                        ];

                        let mut resolved =
                            item.desktop_entry.as_deref().and_then(resolve_desktop_icon);

                        if resolved.is_none() {
                            for v in &variants {
                                if v.is_empty() {
                                    continue;
                                }
                                if let Some(p) = resolve_icon_name(v) {
                                    resolved = Some(p);
                                    break;
                                }
                            }
                        }

                        if let Some(p) = resolved {
                            eprintln!("[notif-migrate] id={} {} → {}", item.id, item.app_name, p);
                            item.avatar_path = Some(p);
                            migrated = true;
                        } else {
                            still_missing += 1;
                        }
                    }
                    if still_missing > 0 && std::env::var_os("MBS_DEBUG").is_some() {
                        eprintln!(
                            "[notif-migrate] {} entries still have no resolvable icon",
                            still_missing
                        );
                    }
                    if migrated {
                        if let Ok(data) = serde_json::to_string_pretty(&items) {
                            let tmp = path.with_extension("tmp");
                            if fs::write(&tmp, data).is_ok() {
                                let _ = fs::rename(tmp, &path);
                            }
                        }
                    }
                    return items;
                }
            }
        }
        Vec::new()
    }

    pub fn mark_history_dirty(&mut self) {
        self.history_dirty = true;
    }

    pub fn flush_history(&mut self) {
        if self.history_dirty && self.last_saved.elapsed().as_millis() > 2000 {
            self.history_dirty = false;
            self.last_saved = Instant::now();
            self.save_history_immediate();
        }
    }

    fn save_history_immediate(&self) {
        let path = Self::history_path();
        if let Ok(data) = serde_json::to_string_pretty(&self.history) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, path);
            }
        }
    }

    pub fn push_notification(
        &mut self,
        app_name: &str,
        summary: &str,
        body: &str,
        is_urgent: bool,
        is_test: bool,
        desktop_entry: Option<&str>,
        actions: Vec<String>,
        settings: &ShellSettings,
    ) {
        // Finding 45: cap attacker-controlled fields. Notification
        // payloads arrive from any same-user D-Bus client (any app
        // calling notify-send, or D-Bus directly). A 100 MB body is
        // bounded to 4 KB; history stays small; load_history can
        // never OOM. Caps generous for real content.
        const MAX_APP_NAME: usize = 128;
        const MAX_SUMMARY: usize = 512;
        const MAX_BODY: usize = 4096;
        const MAX_ACTIONS: usize = 16;
        const MAX_ACTION_LEN: usize = 256;
        let cap = |s: &str, n: usize| -> String {
            if s.chars().count() <= n {
                s.to_string()
            } else {
                s.chars().take(n).collect()
            }
        };
        let app_name: String = cap(app_name, MAX_APP_NAME);
        let summary: String = cap(summary, MAX_SUMMARY);
        let body: String = cap(body, MAX_BODY);
        let actions: Vec<String> = actions
            .into_iter()
            .take(MAX_ACTIONS)
            .map(|a| cap(&a, MAX_ACTION_LEN))
            .collect();

        let id = rand_id();

        let matched_profile = settings.friend_profiles.iter().find(|p| {
            summary.to_lowercase().contains(&p.summary.to_lowercase())
                || app_name.to_lowercase().contains(&p.name.to_lowercase())
        });

        if let Some(p) = matched_profile {
            if p.sound && !is_test {
                if let Some(path) = &p.sound_path {
                    Self::play_sound(path.clone(), settings.notif_volume);
                }
            }
        }

        let border_color = if is_urgent {
            "#ff0000".to_string()
        } else if let Some(p) = matched_profile {
            p.color.clone()
        } else {
            settings.palette.base05.clone()
        };

        let text_color = if is_urgent {
            settings.palette.base05.clone()
        } else if let Some(p) = matched_profile {
            p.color.clone()
        } else {
            settings.palette.base05.clone()
        };
        // Resolve avatar. Order of preference:
        //   1. Friend-profile specific avatar
        //   2. Fallback avatar file
        //   3. Desktop-entry icon (real app icon from .desktop → Icon=)
        //   4. App-name based icon lookup
        // Logs every decision so we can debug missing icons.
        let avatar_path: Option<String> = match matched_profile {
            Some(p) => {
                let specific = format!("resources/{0}/{0}.png", p.name);
                if std::path::Path::new(&specific).exists() {
                    crate::dbg_log!("[notif-icon] {} → friend avatar: {}", app_name, specific);
                    Some(specific)
                } else if std::path::Path::new("resources/fallback.png").exists() {
                    crate::dbg_log!(
                        "[notif-icon] {} → friend fallback (missing {})",
                        app_name,
                        specific
                    );
                    Some("resources/fallback.png".to_string())
                } else if let Some(p) = desktop_entry.and_then(resolve_desktop_icon) {
                    crate::dbg_log!("[notif-icon] {} → desktop icon fallback: {}", app_name, p);
                    Some(p)
                } else {
                    let p = resolve_icon_name(&app_name.to_lowercase().replace(' ', "-"));
                    if let Some(p) = p {
                        crate::dbg_log!("[notif-icon] {} → app-name icon: {}", app_name, p);
                        Some(p)
                    } else {
                        crate::dbg_log!("[notif-icon] {} → NONE", app_name);
                        None
                    }
                }
            }
            None => {
                if let Some(p) = desktop_entry.and_then(resolve_desktop_icon) {
                    crate::dbg_log!("[notif-icon] {} → desktop icon: {}", app_name, p);
                    Some(p)
                } else {
                    // Try multiple name variants for the app_name fallback.
                    // Notifications commonly send "Mozilla Firefox", "Brave
                    // Browser", "System Monitor" — the icon filename is
                    // usually the de-hyphenated lowercase form, but sometimes
                    // it's just the first word or a reverse-DNS variant.
                    let lower = app_name.to_lowercase();
                    let hyphenated = lower.replace(' ', "-");
                    let underscored = lower.replace(' ', "_");
                    let first_word = lower.split_whitespace().next().unwrap_or("").to_string();
                    let variants = [
                        lower.clone(),
                        hyphenated.clone(),
                        underscored.clone(),
                        first_word,
                    ];

                    let mut found: Option<String> = None;
                    for v in &variants {
                        if v.is_empty() {
                            continue;
                        }
                        if let Some(p) = resolve_icon_name(v) {
                            crate::dbg_log!(
                                "[notif-icon] {} → app-name variant '{}': {}",
                                app_name,
                                v,
                                p
                            );
                            found = Some(p);
                            break;
                        }
                    }
                    if found.is_none() {
                        crate::dbg_log!(
                            "[notif-icon] {} → NONE (desktop_entry={:?}, tried={:?})",
                            app_name,
                            desktop_entry,
                            variants
                        );
                    }
                    found
                }
            }
        };
        let item = NotificationToast {
            id,
            app_name: app_name.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            timestamp: Local::now().format("%I:%M %p").to_string(),
            is_read: false,
            is_urgent,
            border_color,
            text_color,
            avatar_path,
            desktop_entry: desktop_entry.map(|s| s.to_string()),
            actions,
            created_at: Instant::now(),
            exit_started_at: None,
            hold_reset_at: None,
            slot_shift_start: None,
            tts_summary: String::new(),
            tts_body: String::new(),
            tts_readout: None,
            tts_command: None,
            tts_pending: false,
            tts_spoken: false,
        };

        let tts_pending = !is_test && matched_profile.map(|p| p.tts).unwrap_or(false);
        let mut tts_readout = matched_profile.and_then(|p| p.default_readout.clone());

        // Apply custom content rules if they match the summary or body
        if let Some(p) = matched_profile {
            for rule in &p.content_rules {
                let pat = rule.pattern.to_lowercase();
                if summary.to_lowercase().contains(&pat) || body.to_lowercase().contains(&pat) {
                    tts_readout = Some(rule.readout.clone());
                    break;
                }
            }
        }
        let tts_command = settings.tts_command.clone();
        eprintln!(
            "[tts-defer] app={:?} pending={} summary={:?}",
            app_name, tts_pending, summary,
        );

        // Populate TTS fields on the item before routing it. Previously
        // this happened via `active_toasts.last_mut()` after push; the
        // queue routing below makes that unsafe, so we set it here.
        let mut item = item;
        item.tts_summary = summary.to_string();
        item.tts_body = body.to_string();
        item.tts_readout = tts_readout;
        item.tts_command = tts_command;
        item.tts_pending = tts_pending;

        if dnd_active() {
            eprintln!(
                "[dnd] holding notification back while paused: {:?}",
                summary
            );
            self.history.insert(0, item.clone());
            if self.history.len() > 80 {
                self.history.truncate(80);
            }
            self.mark_history_dirty();
            let mut held = load_held_notifs();
            held.push(item);
            save_held_notifs(&held);
            eprintln!("[dnd] total notifications held: {}", held.len());
            return;
        }

        // Route to active if there's room, else to the pending queue.
        // 4 concurrent toasts is the visible limit; the 5th+ wait their
        // turn and are pulled in by `tick_toasts` when a slot frees.
        if self.active_toasts.len() < 4 {
            self.active_toasts.push(item.clone());
        } else {
            self.pending_toasts.push_back(item.clone());
            eprintln!(
                "[queue] overflow: {:?} queued (pending={})",
                summary,
                self.pending_toasts.len()
            );
        }

        self.history.insert(0, item);
        if self.history.len() > 80 {
            self.history.truncate(80);
        }
        self.mark_history_dirty();
    }

    pub fn play_sound(path: String, volume: u32) {
        std::thread::spawn(move || {
            let vol_f = (volume as f32) / 100.0;
            // Try pipewire
            if let Ok(o) = std::process::Command::new("pw-play")
                .arg(format!("--volume={:.2}", vol_f))
                .arg(&path)
                .output()
            {
                if o.status.success() {
                    return;
                }
            }
            // Try pulseaudio
            let vol_pa = (vol_f * 65536.0) as u32;
            if let Ok(o) = std::process::Command::new("paplay")
                .arg(format!("--volume={}", vol_pa))
                .arg(&path)
                .output()
            {
                if o.status.success() {
                    return;
                }
            }
            // Fallback to ALSA (no software volume)
            let _ = std::process::Command::new("aplay").arg(&path).output();
        });
    }

    pub fn speak_tts(
        app_name: &str,
        summary: &str,
        body: &str,
        _custom_readout: Option<&str>,
        tts_command: Option<String>,
        volume: u32,
    ) {
        let a = app_name.to_string();
        let s = summary.to_string();
        let b = body.to_string();
        let readout_template = _custom_readout.map(|s| s.to_string()).unwrap_or_else(|| {
            let lower = format!("{} {}", s, b).to_lowercase();
            if lower.contains("logged in") || lower.contains("signed in") {
                "{sender} has logged in".to_string()
            } else {
                "Message from {sender}".to_string()
            }
        });

        std::thread::spawn(move || {
            // Smart sender extraction
            let mut sender = String::new();
            let mut msg = b.clone();

            // 1. Try to parse "Alice: Hello" from the body
            if let Some(idx) = b.find(':') {
                let prefix = b[..idx].trim();
                if !prefix.is_empty() && prefix.len() < 30 && !prefix.contains("http") {
                    sender = prefix.to_string();
                    msg = b[idx + 1..].trim().to_string();
                }
            }

            // 2. If no sender found, check if the summary is a real name
            if sender.is_empty() {
                let lower = s.to_lowercase();
                let is_generic = lower.contains("new message")
                    || lower.contains("new voice")
                    || lower.contains("notification")
                    || lower == "test";

                if !is_generic {
                    sender = s.clone();
                    // Strip common login suffixes so "{sender} has logged in"
                    // doesn't become "Player has logged in has logged in".
                    let lower_sender = sender.to_lowercase();
                    for suffix in [
                        " has logged in",
                        " has signed in",
                        " is now online",
                        " logged in",
                    ] {
                        if lower_sender.ends_with(suffix) {
                            sender = sender[..sender.len() - suffix.len()].trim().to_string();
                            break;
                        }
                    }
                } else if !b.is_empty() && b.len() < 30 && !b.contains(':') {
                    let b_lower = b.to_lowercase();
                    if !b_lower.contains("voice") && !b_lower.contains("new message") {
                        sender = b.clone();
                        msg = "".to_string();
                    } else {
                        sender = a.clone();
                    }
                } else {
                    sender = a.clone(); // Fallback to app name
                }
            }

            let mut spoken = readout_template.clone();
            for (key, val) in [
                // The new smart sender variables
                ("{sender}", &sender),
                ("$sender", &sender),
                ("{user}", &sender),
                ("$user", &sender),
                ("{name}", &sender),
                ("$name", &sender),
                // The raw message body (without "Alice: ")
                ("{message}", &msg),
                ("$message", &msg),
                ("{body}", &msg),
                ("$body", &msg),
                // The raw summary/title
                ("{summary}", &s),
                ("$summary", &s),
                // The app name (Horizon, Discord, etc.)
                ("{app}", &a),
                ("$app", &a),
                ("{app_name}", &a),
                ("$app_name", &a),
                ("{appname}", &a),
                ("$appname", &a),
            ] {
                spoken = spoken.replace(key, val);
            }

            crate::dbg_log!("[tts-trace] app={:?} summary={:?} body={:?}", a, s, b);
            crate::dbg_log!(
                "[tts-trace] extracted sender={:?} message={:?}",
                sender,
                msg
            );
            crate::dbg_log!(
                "[tts-trace] template={:?} -> spoken={:?}",
                readout_template,
                spoken
            );

            if let Some(cmd) = tts_command {
                let vol_str = format!("{:.2}", (volume as f32) / 100.0);
                let script = if cmd.contains("{text}") {
                    cmd.replace("{text}", "\"$1\"")
                        .replace("{volume}", &vol_str)
                } else {
                    format!("{} \"$1\"", cmd)
                };
                match std::process::Command::new("sh")
                    .args(["-c", &script, "--", &spoken])
                    .output()
                {
                    Ok(o) => eprintln!(
                        "[tts] custom exit={:?} stderr={}",
                        o.status.code(),
                        String::from_utf8_lossy(&o.stderr)
                            .trim()
                            .chars()
                            .take(200)
                            .collect::<String>(),
                    ),
                    Err(e) => eprintln!("[tts] custom spawn failed: {e}"),
                }
                return;
            }

            let candidates: Vec<String> = vec![
                "/run/current-system/sw/bin/sage-tts".into(),
                "sage-tts".into(),
                "spd-say".into(),
                "espeak-ng".into(),
                "espeak".into(),
            ];
            let mut tried: Vec<String> = Vec::new();
            for bin in &candidates {
                tried.push(bin.clone());
                let mut cmd = std::process::Command::new(bin);
                if bin.contains("espeak") {
                    let amp = (volume * 2).min(200).to_string();
                    cmd.args(["-a", &amp]);
                }
                match cmd.arg(&spoken).output() {
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                        let really_ok =
                            o.status.success() && !stderr.contains("daemon not running");
                        if really_ok {
                            return;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        eprintln!("[tts] spawn {} failed: {e}", bin);
                        return;
                    }
                }
            }
            eprintln!("[tts] NO TTS BINARY FOUND; tried {:?}", tried);
        });
    }

    pub fn tick_toasts(&mut self, hold_duration_sec: u64, volume: u32) {
        let hold_limit = std::time::Duration::from_secs(hold_duration_sec.max(1));
        let exit_duration = std::time::Duration::from_millis(300);
        let now = Instant::now();

        let any_exiting = self
            .active_toasts
            .iter()
            .any(|t| t.exit_started_at.is_some());
        if !any_exiting {
            let oldest_expired = self
                .active_toasts
                .iter_mut()
                .filter(|t| {
                    let hold_ref = t.hold_reset_at.unwrap_or(t.created_at);
                    !t.is_urgent && t.exit_started_at.is_none() && hold_ref.elapsed() >= hold_limit
                })
                .min_by_key(|t| t.hold_reset_at.unwrap_or(t.created_at));
            if let Some(t) = oldest_expired {
                t.exit_started_at = Some(now);
            }
        }

        let mut removed_any = false;
        self.active_toasts.retain(|t| {
            if t.is_urgent {
                return true;
            }
            match t.exit_started_at {
                Some(start) => {
                    if start.elapsed() < exit_duration {
                        true
                    } else {
                        removed_any = true;
                        false
                    }
                }
                None => true,
            }
        });

        if removed_any {
            let now = Instant::now();
            for t in self.active_toasts.iter_mut() {
                t.hold_reset_at = Some(now);
                t.exit_started_at = None;
                t.slot_shift_start = Some(now);
            }

            // Fill freed slots from the pending queue. Reset timers so the
            // newly-promoted toast starts its full hold duration and its
            // slide-in animation from the current instant, not from when
            // it was originally queued.
            while self.active_toasts.len() < 4 {
                let Some(mut next) = self.pending_toasts.pop_front() else {
                    break;
                };
                next.created_at = Instant::now();
                next.hold_reset_at = Some(Instant::now());
                next.exit_started_at = None;
                next.slot_shift_start = Some(Instant::now());
                eprintln!(
                    "[queue] promoting queued toast to active: {:?}",
                    next.summary
                );
                self.active_toasts.push(next);
            }
        }

        let front_id = self
            .active_toasts
            .iter()
            .filter(|t| t.exit_started_at.is_none())
            .min_by_key(|t| t.created_at)
            .map(|t| t.id);
        if let Some(fid) = front_id {
            let payload = if let Some(t) = self.active_toasts.iter_mut().find(|t| t.id == fid) {
                if t.tts_pending && !t.tts_spoken {
                    t.tts_spoken = true;
                    Some((
                        t.app_name.clone(),
                        t.tts_summary.clone(),
                        t.tts_body.clone(),
                        t.tts_readout.clone(),
                        t.tts_command.clone(),
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((a, s, b, r, c)) = payload {
                crate::dbg_log!("[tts-fire] id={} app={:?}", fid, a);
                Self::speak_tts(&a, &s, &b, r.as_deref(), c, volume);
            }
        }

        let new_count = self.active_toasts.len();
        if new_count != self.stack_target_count {
            self.stack_from_count = self.stack_target_count;
            self.stack_target_count = new_count;
            self.stack_anim_start = Instant::now();
        }
    }

    pub fn update(&mut self, message: NotifMessage) {
        match message {
            NotifMessage::DismissToast(id) => {
                let now = Instant::now();
                for t in self.active_toasts.iter_mut() {
                    if t.id == id && t.exit_started_at.is_none() {
                        t.exit_started_at = Some(now);
                    }
                }
            }
            NotifMessage::ActivateToast(id) => {
                let now = Instant::now();
                for t in self.active_toasts.iter_mut() {
                    if t.id == id && t.exit_started_at.is_none() {
                        t.exit_started_at = Some(now);
                    }
                }
            }
            NotifMessage::ClearHistory => {
                self.history.clear();
                self.pending_toasts.clear();
                self.selected_history_id = None;
                self.mark_history_dirty();
            self.save_history_immediate();
                if let Ok(mut g) = LOGGED_ICONS.lock() {
                    if let Some(set) = g.as_mut() {
                        set.clear();
                    }
                }
            }
            NotifMessage::DeleteHistory(id) => {
                self.history.retain(|t| t.id != id);
                if self.selected_history_id == Some(id) {
                    self.selected_history_id = None;
                }
                self.mark_history_dirty();
            self.save_history_immediate();
            }
            NotifMessage::HistorySelectNext => {
                if self.history.is_empty() {
                    return;
                }
                // history is stored newest-first (insert at index 0).
                // Visual order is reversed (oldest top, newest bottom).
                // Moving "down" visually = older index in the array.
                let current_pos = self
                    .selected_history_id
                    .and_then(|id| self.history.iter().position(|t| t.id == id));
                self.selected_history_id = Some(match current_pos {
                    None => self.history[0].id,
                    Some(i) => self.history[i.saturating_sub(1)].id,
                });
            }
            NotifMessage::HistorySelectPrev => {
                if self.history.is_empty() {
                    return;
                }
                let current_pos = self
                    .selected_history_id
                    .and_then(|id| self.history.iter().position(|t| t.id == id));
                self.selected_history_id = Some(match current_pos {
                    None => self.history[self.history.len() - 1].id,
                    Some(i) => self.history[(i + 1).min(self.history.len() - 1)].id,
                });
            }
            NotifMessage::HistoryDeleteSelected => {
                if let Some(id) = self.selected_history_id {
                    let pos = self.history.iter().position(|t| t.id == id);
                    self.history.retain(|t| t.id != id);
                    self.mark_history_dirty();
            self.save_history_immediate();

                    self.selected_history_id = if self.history.is_empty() {
                        None
                    } else {
                        let new_pos = pos.map(|p| p.min(self.history.len() - 1)).unwrap_or(0);
                        Some(self.history[new_pos].id)
                    };
                }
            }
            NotifMessage::HistoryScrollTo(_) => {}
            NotifMessage::ToggleHistoryDrawer => {
                self.show_history = !self.show_history;
            }
            NotifMessage::DismissDrawer => {
                self.show_history = false;
            }
        }
    }

    pub fn view_toasts<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &'a ShellSettings,
    ) -> Element<'a, NotifMessage> {
        let card_w = settings.notif_card_width as f32;
        let card_h = settings.notif_card_height as f32;

        let shape = match settings.notif_shape.to_lowercase().as_str() {
            "hex" | "hexagon" => ShapeType::Hexagon { cut: 14 },
            "slant" => ShapeType::Slant {
                left: SlantSide::Right,
                right: SlantSide::Right,
            },
            "flat" => ShapeType::Flat,
            _ => ShapeType::Rounded(10),
        };

        let from_right = settings.notif_corner.contains("right");

        let mut deck = Stack::new();
        let ordered: Vec<(usize, &NotificationToast)> =
            self.active_toasts.iter().enumerate().collect();

        let strip_w =
            settings.notif_card_width as f32 + 2.0 * settings.notif_margin_x as f32 + 100.0;
        let strip_margin = settings.notif_margin_x as f32;

        // Iterate forward: oldest pushed first (bottom z-order), newest
        // pushed last (top z-order). Combined with y_pos below
        // (i=0 → lowest on screen, i=N-1 → highest), the newest toast
        // ends up at the visual top AND the highest z-order. Standard
        // notification-stack layout: newest fully readable on top,
        // older ones tucked behind/below. Clicking overlap hits the
        // newest toast, and hotkeys targeting .last() correctly resolve
        // to what's visually accessible.
        for (i, toast) in ordered.into_iter().rev() {
            let id = toast.id;
            let border_c = hex_to_color(&toast.border_color);

            let elapsed_ms = toast.created_at.elapsed().as_millis() as f32;
            let slide_ms: f32 = if settings.notif_slide_in_ms == 0 {
                250.0
            } else {
                settings.notif_slide_in_ms.max(50) as f32
            };
            let pause_ms = settings.notif_drop_pause_ms as f32;
            let drop_ms = 250.0_f32;
            let drop_dist = settings.notif_drop_distance_px as f32;
            let exit_ms = 300.0_f32;

            // Phase 1: Slide in horizontally from offscreen
            let slide_progress = (elapsed_ms / slide_ms).clamp(0.0, 1.0);
            let slide_ease = 1.0 - (1.0 - slide_progress).powi(3);

            // Phase 3: Slide away horizontally to the right on expiration / dismissal
            let exit_progress = toast
                .exit_started_at
                .map(|s| (s.elapsed().as_millis() as f32 / exit_ms).clamp(0.0, 1.0))
                .unwrap_or(0.0);
            let exit_ease = exit_progress * exit_progress * exit_progress;

            // Compute resting and offscreen coordinates without compressing the card width
            let (resting_x, offscreen_x) = if from_right {
                (strip_w - card_w - strip_margin, strip_w)
            } else {
                (strip_margin, -card_w)
            };

            // Interpolate position: offscreen -> resting -> offscreen (on exit)
            let enter_x = offscreen_x + (resting_x - offscreen_x) * slide_ease;
            let x_pos = enter_x + (offscreen_x - resting_x) * exit_ease;

            // Phase 2: Vertical drop down (starts after horizontal entry + pause)
            let drop_start = slide_ms + pause_ms;
            let drop_progress = ((elapsed_ms - drop_start) / drop_ms).clamp(0.0, 1.0);
            let drop_ease = 1.0 - (1.0 - drop_progress).powi(3);
            let drop_extra = (1.0 - drop_ease) * (-drop_dist);

            let peek = (settings.notif_stack_overlap as f32).abs().max(4.0);

            let shift_progress = toast
                .slot_shift_start
                .map(|s| (s.elapsed().as_millis() as f32 / 200.0).min(1.0))
                .unwrap_or(1.0);
            let shift_ease = 1.0 - (1.0 - shift_progress).powi(3);
            let shift_offset = (1.0 - shift_ease) * (-peek);
            let fixed_base = drop_dist + 4.0 * peek;
            let slot_y = fixed_base - (i as f32) * peek + shift_offset;
            let y_pos = (slot_y + drop_extra).max(0.0);

            let card_bg = ShapeBox::new()
                .shape(shape)
                .background(theme.background)
                .border(border_c, theme.border_width)
                .size(Length::Fixed(card_w), Length::Fixed(card_h));

            let badge_bg = if toast.is_urgent {
                theme.danger
            } else {
                border_c
            };
            let icon_badge = container(
                text(if toast.is_urgent { "!" } else { "i" })
                    .size(13)
                    .bold()
                    .color(Color::BLACK),
            )
            .padding([3, 7])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(badge_bg)),
                border: iced::Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

            let toast_text_color = hex_to_color(&toast.text_color);
            let avatar: Element<'_, NotifMessage> = if let Some(ref path) = toast.avatar_path {
                iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                    .width(Length::Fixed(48.0))
                    .height(Length::Fixed(48.0))
                    .into()
            } else {
                container(text("")).width(Length::Fixed(0.0)).into()
            };
            // Full-body activate area. Padding lives INSIDE the
            // MouseArea (via the container), and both the container and
            // inner row are Fill height. Result: the entire toast body —
            // including what was previously the 14px border — activates
            // on click. No vertical centering gap.
            let toast_body = MouseArea::new(
                container(
                    row![
                        icon_badge,
                        avatar,
                        column![
                            text(&toast.summary)
                                .size((settings.notif_summary_size) as f32)
                                .bold()
                                .color(toast_text_color)
                                .width(Length::Fill),
                            text(&toast.body)
                                .size((settings.notif_body_size) as f32)
                                .color(toast_text_color)
                                .width(Length::Fill),
                        ]
                        .width(Length::Fill)
                        .spacing(2),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
                .padding(14)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .on_press(NotifMessage::ActivateToast(id));

            // Full-height close column. 48px wide, Fill height, ✕ centered
            // vertically (matches the previous visual), but the whole
            // column is now one big dismiss target.
            let close_btn = button(text("✕").size(12).color(theme.muted))
                .on_press(NotifMessage::DismissToast(id))
                .padding(0)
                .width(Length::Fixed(48.0))
                .height(Length::Fill)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });

            let content = row![toast_body, close_btn]
                .spacing(0)
                .width(Length::Fill)
                .height(Length::Fill);

            let toast_card = container(Stack::new().push(card_bg.view()).push(content))
                .width(Length::Fixed(card_w))
                .height(Length::Fixed(card_h));

            // Pin widget directly positions the card without applying flex compression
            deck = deck.push(pin(toast_card).x(x_pos).y(y_pos));
        }

        // Clip container to ensure the card cleanly enters and exits across the strip boundary
        let clipped_strip = container(deck)
            .width(Length::Fixed(strip_w))
            .height(Length::Fill)
            .clip(true);

        container(clipped_strip)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(if from_right {
                iced::alignment::Horizontal::Right
            } else {
                iced::alignment::Horizontal::Left
            })
            .into()
    }

    pub fn view_history<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &crate::libraries::config::settings::ShellSettings,
    ) -> Element<'a, NotifMessage> {
        let (win_w, win_h) = settings.get_window_size("history", 960, 720);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let footer_inset = if dy > pad_bottom {
            (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let body_inset = header_inset.min(footer_inset) + 4.0;

        let fs = theme.font_size_overlay as f32;
        let fs_sm = fs * 0.85;
        let gap = 10.0;
        let row_w = win_w - body_inset * 2.0;
        // Minimum row height so short notifications don't collapse too
        // small. Rows *grow* past this if the body text needs more space.
        let _row_min_h = 72.0;

        let mut list_col = column![].spacing(gap);

        if self.history.is_empty() {
            list_col = list_col.push(
                container(
                    text("No notifications in history.")
                        .size(fs)
                        .color(theme.muted),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            );
        } else {
            // Render in reverse so newest is at the bottom.
            // Compute approximate row height from body text so ShapeBox
            // and content are both fixed at the same size (Fill+Shrink
            // collapses to 0 in iced 0.14).
            for toast in self.history.iter().rev() {
                let id = toast.id;
                let log_now = should_log_icon(id);
                let is_sel = self.selected_history_id == Some(id);

                // Icon size matches the launcher slider setting.
                // Hexagon sized large enough that its cut corners can't
                // clip the icon's corners (see history_icon_hex_size).
                let icon_size = settings.launcher_icon_size as f32;
                let icon_hex_size = history_icon_hex_size(icon_size);
                let icon_hex_cut: u16 = ((icon_hex_size * 0.10) as u16).max(8);

                let inner_icon: Element<'a, NotifMessage> = if let Some(ref path) =
                    toast.avatar_path
                {
                    if std::path::Path::new(path).exists() {
                        crate::dbg_log!("[notif-render] id={} icon=OK {}", id, path);
                        if path.ends_with(".svg") {
                            iced::widget::svg(iced::widget::svg::Handle::from_path(path.clone()))
                                .width(Length::Fixed(icon_size))
                                .height(Length::Fixed(icon_size))
                                .into()
                        } else {
                            iced::widget::image(iced::widget::image::Handle::from_path(
                                path.clone(),
                            ))
                            .width(Length::Fixed(icon_size))
                            .height(Length::Fixed(icon_size))
                            .content_fit(iced::ContentFit::Contain)
                            .into()
                        }
                    } else {
                        if log_now {
                            crate::dbg_log!("[notif-render] id={} icon=MISSING {}", id, path);
                        }
                        container(text("🔔").size(icon_size * 0.7))
                            .width(Length::Fixed(icon_size))
                            .height(Length::Fixed(icon_size))
                            .center_x(Length::Fill)
                            .center_y(Length::Fill)
                            .into()
                    }
                } else {
                    if log_now {
                        crate::dbg_log!("[notif-render] id={} icon=NONE", id);
                    }
                    container(text("🔔").size(icon_size * 0.7))
                        .width(Length::Fixed(icon_size))
                        .height(Length::Fixed(icon_size))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .into()
                };

                let icon_hex_bg = ShapeBox::new()
                    .shape(ShapeType::Hexagon { cut: icon_hex_cut })
                    .background(theme.background)
                    .border(theme.border, theme.control_border_width)
                    .size(Length::Fixed(icon_hex_size), Length::Fixed(icon_hex_size));

                let icon_clipped = container(inner_icon)
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .clip(true);

                let icon_elem: Element<'a, NotifMessage> = Stack::new()
                    .push(icon_hex_bg.view())
                    .push(
                        container(icon_clipped)
                            .width(Length::Fixed(icon_hex_size))
                            .height(Length::Fixed(icon_hex_size))
                            .center_x(Length::Fill)
                            .center_y(Length::Fill),
                    )
                    .into();

                let time_str = toast.timestamp.clone();
                let app_str = toast.app_name.clone();
                let summary = toast.summary.clone();
                let body = toast.body.clone();

                // Delegate to shared helper — must match scroll_offset_for_selected.
                let row_h = history_row_height(toast, row_w, fs, fs_sm, icon_size);

                // Prominent highlight: bright green border at 2.5× width
                // + surface_alt background. The line style is used
                // everywhere else in the shell for "this is the active
                // item" (RNG, Reminders, Todo, Notes, Unicode) so it reads
                // intuitively.
                let border_col = if is_sel { theme.success } else { theme.border };
                let border_w = if is_sel {
                    theme.control_border_width * 2.0
                } else {
                    theme.control_border_width
                };

                let row_bg = ShapeBox::new()
                    .shape(ShapeType::Hexagon { cut: 16 })
                    .background(if is_sel {
                        theme.surface_alt
                    } else {
                        theme.background
                    })
                    .border(border_col, border_w)
                    .size(Length::Fixed(row_w), Length::Fixed(row_h));

                let text_col = column![
                    row![
                        text(summary).size(fs).bold().color(theme.text),
                        iced::widget::space::horizontal(),
                        text(format!("{} • {}", app_str, time_str))
                            .size(fs_sm)
                            .color(theme.muted),
                    ]
                    .align_y(iced::alignment::Vertical::Center),
                    text(body)
                        .size(fs_sm)
                        .color(theme.text)
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                ]
                .spacing(4)
                .width(Length::Fill);

                let inner_row = row![icon_elem, container(text_col).width(Length::Fill),]
                    .spacing(16)
                    .align_y(iced::alignment::Vertical::Top);

                let row_content = container(inner_row)
                    .width(Length::Fixed(row_w))
                    .height(Length::Fixed(row_h))
                    .padding(iced::Padding {
                        top: 12.0,
                        right: 20.0,
                        bottom: 12.0,
                        left: 20.0,
                    })
                    .align_y(iced::alignment::Vertical::Top);

                let stack = Stack::new().push(row_bg.view()).push(row_content);

                let btn = button(stack)
                    .on_press(NotifMessage::HistoryDeleteSelected)
                    .padding(0)
                    .width(Length::Fixed(row_w))
                    .height(Length::Fixed(row_h))
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    });

                list_col = list_col.push(btn);
            }
        }

        let pos_label = if let Some(id) = self.selected_history_id {
            if let Some(i) = self.history.iter().position(|t| t.id == id) {
                // Render order is reversed: position in list (1-indexed, top-down)
                // = history.len() - i
                format!("{} / {}", self.history.len() - i, self.history.len())
            } else {
                format!("{} items", self.history.len())
            }
        } else if !self.history.is_empty() {
            format!("{} items", self.history.len())
        } else {
            String::new()
        };

        let header_row = container(
            row![
                text("📜 NOTIFICATION HISTORY")
                    .size(fs)
                    .bold()
                    .color(theme.text),
                iced::widget::space::horizontal(),
                text(pos_label).size(fs_sm).bold().color(theme.muted),
                button(text("Clear All").size(fs_sm).bold().color(theme.danger))
                    .on_press(NotifMessage::ClearHistory)
                    .padding([6, 12])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border {
                            color: theme.danger,
                            width: 1.0,
                            radius: 4.0.into()
                        },
                        ..Default::default()
                    }),
            ]
            .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let list_container = container(
            scrollable(list_col)
                .id(iced::widget::Id::new("history_list"))
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: body_inset,
            bottom: 0.0,
            left: body_inset,
        });

        let footer_row = container(
            text("↑/↓ Navigate  •  Delete to remove  •  ESC to dismiss")
                .size(fs_sm)
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: footer_inset,
            bottom: 0.0,
            left: footer_inset,
        });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![header_row, list_container, footer_row]
                .spacing(14)
                .padding(Padding {
                    top: pad_top,
                    right: 0.0,
                    bottom: pad_bottom,
                    left: 0.0,
                }),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h))
        .style(|_t| iced::widget::container::Style::default());

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_t| iced::widget::container::Style::default())
            .into()
    }
}

fn rand_id() -> u32 {
    let mut b = [0u8; 4];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
        u32::from_ne_bytes(b)
    } else {
        1000
    }
}

pub fn dnd_flag_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("moonbeauty-dnd")
}

pub fn dnd_active() -> bool {
    dnd_flag_path().exists()
}

pub fn toggle_dnd() -> bool {
    let p = dnd_flag_path();
    let state = if p.exists() {
        let _ = fs::remove_file(&p);
        eprintln!("[dnd] flag file removed: {:?}", p);
        false
    } else {
        let _ = crate::libraries::process::private_write_file(&p, b"1");
        eprintln!("[dnd] flag file created: {:?}", p);
        true
    };
    state
}

pub fn held_notifs_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("moonbeauty-held-notifs.json")
}

pub fn load_held_notifs() -> Vec<NotificationToast> {
    // Finding 47: 16 MB cap (80 history × ~4 KB × 50 = 16 MB headroom;
    // a real DND storm can reach this). Prevents a pre-fix file written
    // at 400 MB from OOMing on read.
    crate::libraries::process::read_json_capped::<Vec<NotificationToast>>(
        &held_notifs_path(),
        16 * 1024 * 1024,
    )
    .unwrap_or_default()
}

pub fn save_held_notifs(toasts: &[NotificationToast]) {
    let p = held_notifs_path();
    if toasts.is_empty() {
        let _ = fs::remove_file(&p);
    } else if let Ok(json) = serde_json::to_string(toasts) {
        let tmp = p.with_extension("tmp");
        if crate::libraries::process::private_write_file(&tmp, json.as_bytes()).is_ok() {
            let _ = fs::rename(tmp, p);
        }
    }
    let mut g = held_cache().lock().unwrap();
    g.count = toasts.len();
    g.fetched_at = Instant::now();
}

struct HeldCache {
    count: usize,
    fetched_at: Instant,
}
static HELD_CACHE: std::sync::OnceLock<std::sync::Mutex<HeldCache>> = std::sync::OnceLock::new();

fn held_cache() -> &'static std::sync::Mutex<HeldCache> {
    HELD_CACHE.get_or_init(|| {
        std::sync::Mutex::new(HeldCache {
            count: 0,
            fetched_at: Instant::now() - std::time::Duration::from_secs(60),
        })
    })
}

pub fn held_notifs_count() -> usize {
    let mut g = held_cache().lock().unwrap();
    if g.fetched_at.elapsed() > std::time::Duration::from_secs(1) {
        g.count = load_held_notifs().len();
        g.fetched_at = Instant::now();
    }
    g.count
}

pub fn take_held_notifs() -> Vec<NotificationToast> {
    let list = load_held_notifs();
    let _ = fs::remove_file(held_notifs_path());
    let mut g = held_cache().lock().unwrap();
    g.count = 0;
    g.fetched_at = Instant::now();
    list
}

// ── Cross-process active count ─────────────────────────────────────
//
// The bar's notification capsule needs to know how many notifications
// are currently on screen or queued. Those live in the notif-surface
// process's memory (`active_toasts` + `pending_toasts`), not the bar.
// The notif-surface writes this count to a small file on every change
// (plus a heartbeat), and the bar reads it with a short cache.

pub fn active_count_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("moonbeauty-active-count")
}

/// Notif-surface calls this on any change to active+pending count.
pub fn write_active_count(count: usize) {
    let p = active_count_path();
    let tmp = p.with_extension("tmp");
    if crate::libraries::process::private_write_file(&tmp, count.to_string().as_bytes()).is_ok() {
        let _ = fs::rename(tmp, p);
    }
}

struct ActiveCountCache {
    count: usize,
    fetched_at: Instant,
}
static ACTIVE_COUNT_CACHE: std::sync::OnceLock<std::sync::Mutex<ActiveCountCache>> =
    std::sync::OnceLock::new();

fn active_count_cache() -> &'static std::sync::Mutex<ActiveCountCache> {
    ACTIVE_COUNT_CACHE.get_or_init(|| {
        std::sync::Mutex::new(ActiveCountCache {
            count: 0,
            fetched_at: Instant::now() - std::time::Duration::from_secs(60),
        })
    })
}

/// Bar-side read. Cached 250ms (fast enough to feel instant on DND
/// toggle, slow enough to avoid hammering the file during animation).
pub fn read_active_count() -> usize {
    let mut g = active_count_cache().lock().unwrap();
    if g.fetched_at.elapsed() > std::time::Duration::from_millis(250) {
        g.count = read_active_count_uncached();
        g.fetched_at = Instant::now();
    }
    g.count
}

fn read_active_count_uncached() -> usize {
    let p = active_count_path();
    let Ok(meta) = fs::metadata(&p) else {
        return 0;
    };
    // Stale detection: if the notif-surface has been dead for >10s,
    // treat as 0 so the capsule doesn't show a phantom count forever.
    if let Ok(modified) = meta.modified() {
        if let Ok(elapsed) = modified.elapsed() {
            if elapsed.as_secs() > 10 {
                return 0;
            }
        }
    }
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}
