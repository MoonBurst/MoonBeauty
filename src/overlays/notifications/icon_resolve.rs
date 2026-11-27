//! Desktop-entry, freedesktop theme icon, and friend avatar resolution.

use std::path::PathBuf;

/// Search the standard freedesktop icon theme dirs for a file named
/// `<name>.{png,svg,xpm}`.
///
/// Used for notification avatars (chat app icons, email client icons)
/// and anywhere else that has an icon name from a `.desktop` file.
/// Unlike `bar::tray::watcher::resolve_icon`, this returns the source
/// path directly without content hashing or re-encoding.
pub fn resolve_icon_name(name: &str) -> Option<String> {
    crate::libraries::process::resolve_icon_path(name).map(|p| p.to_string_lossy().into_owned())
}

/// Given a desktop-entry hint (e.g. `"firefox"`, `"org.mozilla.firefox"`),
/// find its `.desktop` file under the standard applications dirs, read
/// the `Icon=` field from `[Desktop Entry]`, then resolve that icon name
/// via `resolve_icon_name`.
pub fn resolve_desktop_icon(entry: &str) -> Option<String> {
    let clean = entry.trim_end_matches(".desktop").trim();
    if clean.is_empty() {
        return None;
    }
    // Reject traversal and absolute paths in desktop_entry hint.
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
    //   firefox             → firefox.desktop
    //   org.mozilla.firefox → org.mozilla.firefox.desktop
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

/// Read Icon= from a specific .desktop file's [Desktop Entry]
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

/// $XDG_DATA_HOME/moonbeauty/avatars (created if missing).
fn avatars_dir() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("moonbeauty")
        .join("avatars");
    let _ = std::fs::create_dir_all(&base);
    base
}

/// One-shot migration: copy resources/{name}/{name}.png into the XDG
/// avatars dir on first run. Tries cwd-relative first, then
/// $CARGO_MANIFEST_DIR/resources (dev builds). Non-destructive —
/// skips when XDG target already exists.
fn migrate_resources_to_xdg(name: &str) {
    let xdg = avatars_dir().join(name).join(format!("{}.png", name));
    if xdg.exists() {
        return;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.push(
        std::path::Path::new("resources")
            .join(name)
            .join(format!("{}.png", name)),
    );
    if let Some(m) = option_env!("CARGO_MANIFEST_DIR") {
        candidates.push(
            std::path::Path::new(m)
                .join("resources")
                .join(name)
                .join(format!("{}.png", name)),
        );
    }
    for legacy in candidates {
        if !legacy.exists() {
            continue;
        }
        if let Some(parent) = xdg.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::copy(&legacy, &xdg) {
            Ok(n) => eprintln!("[avatar-migrate] {:?} -> {:?} ({} bytes)", legacy, xdg, n),
            Err(e) => eprintln!("[avatar-migrate] {} failed: {}", name, e),
        }
        return;
    }
}

/// Resolve the friend avatar. Priority:
///   1. $XDG_DATA_HOME/moonbeauty/avatars/{name}/{name}.png
///   2. $XDG_DATA_HOME/moonbeauty/avatars/fallback.png
///   3. legacy resources/{name}/{name}.png
///   4. legacy resources/fallback.png
pub fn avatar_path_for(name: &str) -> Option<String> {
    migrate_resources_to_xdg(name);
    let xdg = avatars_dir();
    let specific = xdg.join(name).join(format!("{}.png", name));
    if specific.exists() {
        return Some(specific.to_string_lossy().into_owned());
    }
    let fallback = xdg.join("fallback.png");
    if fallback.exists() {
        return Some(fallback.to_string_lossy().into_owned());
    }
    let legacy_specific = std::path::Path::new("resources")
        .join(name)
        .join(format!("{}.png", name));
    if legacy_specific.exists() {
        return Some(legacy_specific.to_string_lossy().into_owned());
    }
    let legacy_fallback = std::path::Path::new("resources").join("fallback.png");
    if legacy_fallback.exists() {
        return Some(legacy_fallback.to_string_lossy().into_owned());
    }
    None
}
