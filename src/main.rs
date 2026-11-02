use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use moonbeauty::libraries::config::ShellSettings;
use moonbeauty::libraries::ipc::{IpcCommand, OverlayTarget};
use moonbeauty::libraries::process::resolve_exe;
use moonbeauty::overlays::*;

use moonbeauty::overlays::screenshot;

mod app;

use app::{Message, MoonBeautyShellApp};

pub(crate) fn detect_notif_surface(args: &[String]) -> bool {
    args.iter().any(|a| a == "--notif-surface")
}

pub(crate) fn detect_standalone_overlay(args: &[String]) -> Option<OverlayTarget> {
    for (i, arg) in args.iter().enumerate() {
        if arg == "--lockscreen-instance" {
            return Some(OverlayTarget::Lockscreen);
        }
        if arg == "--annotator-session" {
            return Some(OverlayTarget::Annotator);
        }
        if arg == "--overlay" || arg == "--ipc" {
            if let Some(target_str) = args.get(i + 1) {
                return OverlayTarget::from_str(target_str);
            }
        }
    }
    None
}

// Sync IPC helper — send_client_command uses tokio::net::UnixStream,
// which requires a tokio reactor. main() has no runtime, so we use
// std::os::unix::net::UnixStream directly. Same wire format.
fn send_ipc_sync(cmd: &IpcCommand) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let path = std::env::var("XDG_RUNTIME_DIR")
        .map(|d| std::path::PathBuf::from(d).join("moonbeauty.sock"))
        .map_err(|e| format!("XDG_RUNTIME_DIR: {e}"))?;
    let mut stream = UnixStream::connect(&path)?;
    // Finding 36: read timeout. Without this, a hung bar (deadlock,
    // SIGSTOP, half-close race) blocks the CLI forever.
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let encoded = serde_json::to_vec(cmd)?;
    stream.write_all(&encoded)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp)?;
    Ok(resp)
}

fn notification_candidates(app_name: &str, desktop_entry: Option<&str>) -> Vec<String> {
    use std::collections::HashSet;
    let lower = app_name.to_lowercase();
    let mut cand_set: HashSet<String> = HashSet::new();
    cand_set.insert(lower.clone());
    cand_set.insert(lower.replace(' ', "-"));
    cand_set.insert(lower.replace(' ', "_"));
    cand_set.insert(lower.replace(' ', ""));
    if let Some(de) = desktop_entry {
        let de_clean = de.trim_end_matches(".desktop").to_lowercase();
        cand_set.insert(de_clean);
    }
    cand_set
        .into_iter()
        .filter(|s| !s.is_empty() && moonbeauty::libraries::process::is_safe_app_id(s))
        .collect()
}

fn is_app_focused(candidates: &[String]) -> bool {
    let Some(focused) = focused_app_id() else {
        return false;
    };
    candidates.iter().any(|c| focused.eq_ignore_ascii_case(c))
}

fn focus_notification_app(app_name: &str, desktop_entry: Option<&str>) -> bool {
    use std::collections::HashSet;

    let lower = app_name.to_lowercase();
    let mut cand_set: HashSet<String> = HashSet::new();
    cand_set.insert(lower.clone());
    cand_set.insert(lower.replace(' ', "-"));
    cand_set.insert(lower.replace(' ', "_"));
    cand_set.insert(lower.replace(' ', ""));
    if let Some(de) = desktop_entry {
        let de_clean = de.trim_end_matches(".desktop").to_string();
        cand_set.insert(de_clean.clone());
        cand_set.insert(de_clean.to_lowercase());
    }
    let candidates: Vec<String> = cand_set
        .into_iter()
        .filter(|s| !s.is_empty() && moonbeauty::libraries::process::is_safe_app_id(s))
        .collect();
    moonbeauty::dbg_log!(
        "[focus] app={:?} entry={:?} candidates={:?}",
        app_name,
        desktop_entry,
        candidates
    );

    if std::env::var("SWAYSOCK").is_ok() {
        for cand in &candidates {
            let _ = std::process::Command::new("swaymsg")
                .args(["-t", "command", &format!("[app_id=\"{}\"] focus", cand)])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        if let Some(focused) = focused_app_id() {
            for cand in &candidates {
                if focused.eq_ignore_ascii_case(cand) {
                    return true;
                }
            }
        }
    }

    if let Some(de) = desktop_entry {
        let de_clean = de.trim_end_matches(".desktop");
        if moonbeauty::libraries::process::is_safe_app_id(de_clean) {
            let ok = std::process::Command::new("gtk-launch")
                .arg(de_clean)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
    }

    if let Some(first) = candidates.first() {
        let _ = std::process::Command::new("gtk-launch").arg(first).status();
    }
    false
}

fn focused_app_id() -> Option<String> {
    let out = std::process::Command::new("swaymsg")
        .args(["-t", "get_tree", "-r"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    fn walk(node: &serde_json::Value) -> Option<String> {
        if node.get("focused").and_then(|f| f.as_bool()) == Some(true) {
            if let Some(app_id) = node.get("app_id").and_then(|a| a.as_str()) {
                if !app_id.is_empty() {
                    return Some(app_id.to_string());
                }
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(nodes) = node.get(key).and_then(|n| n.as_array()) {
                for child in nodes {
                    if let Some(r) = walk(child) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }
    walk(&v)
}

fn app_theme(_state: &MoonBeautyShellApp) -> iced::Theme {
    iced::Theme::Dark
}

fn app_style(_state: &MoonBeautyShellApp, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: iced::Color::TRANSPARENT,
        text_color: iced::Color::WHITE,
    }
}

pub fn main() -> Result<(), iced_layershell::Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--clipboard-ingest") {
        use std::io::Read;
        let mut bytes = Vec::new();
        if std::io::stdin().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
            let dir = dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("moonbeauty")
                .join("clipboard");
            let _ = std::fs::create_dir_all(&dir);
            let index_path = dir.join("index.json");

            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // Detect images by Magic Bytes (PNG, JPEG, GIF, WebP)
            // JPEG XL: either the 12-byte container signature or a bare
            // codestream (starts with 0xFF 0x0A).
            let is_jxl = bytes.starts_with(&[0xFF, 0x0A])
                || bytes.starts_with(&[0x00, 0x00, 0x00, 0x0C, 0x4A, 0x58, 0x4C, 0x20, 0x0D, 0x0A, 0x87, 0x0A]);
            let is_image = is_jxl
                || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47])
                || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
                || bytes.starts_with(b"GIF89a")
                || bytes.starts_with(b"GIF87a")
                || (bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP");

            let ext = if is_jxl {
                "jxl"
            } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                "png"
            } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                "jpg"
            } else if bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a") {
                "gif"
            } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
                "webp"
            } else {
                "txt"
            };
            let path = dir.join(format!("{}.{}", id, ext));
            let _ = std::fs::write(&path, &bytes);

            // URI-list ingest (annotator saves, file-manager copies).
            if bytes.starts_with(b"file:") {
                let text = String::from_utf8_lossy(&bytes);
                let mut ingested = 0usize;
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') { continue; }
                    let path_str = line.strip_prefix("file://").unwrap_or(line);
                    let path = std::path::PathBuf::from(path_str);
                    if !path.is_file() { continue; }
                    if moonbeauty::overlays::clipboard::ClipboardOverlay::add_entry(&path, None).is_some() {
                        ingested += 1;
                        eprintln!("[clipboard-ingest] URI: {}", path.display());
                    }
                }
                eprintln!("[clipboard-ingest] URI batch: {} new", ingested);
                std::process::exit(0);
            }

            let mut items: Vec<moonbeauty::overlays::clipboard::ClipItem> = Vec::new();
            if let Ok(content) = std::fs::read_to_string(&index_path) {
                if let Ok(parsed) = serde_json::from_str(&content) {
                    items = parsed;
                }
            }
            let mut text_preview = if is_image {
                String::new()
            } else {
                String::from_utf8_lossy(&bytes[..bytes.len().min(100)]).to_string()
            };

            if is_image {
                let name_file = moonbeauty::libraries::process::mbs_runtime_dir()
                    .join("mbs-last-clip-name.txt");
                if let Ok(txt) = std::fs::read_to_string(&name_file) {
                    text_preview = txt.trim().to_string();
                    let _ = std::fs::remove_file(&name_file);
                }
            }

            // Dedupe by content hash. `wl-paste --watch` can fire more
            // than once per wl-copy (fork-mode ownership change), which
            // previously produced N identical rows for one clipboard
            // write. Compare against the TOP entry only — inserting a
            // genuinely-new occurrence of the same content later still
            // works because it lands at items[0] and pushes the old one
            // down.
            let incoming_hash: String = {
                use sha2::Digest as _;
                format!("{:x}", sha2::Sha256::digest(&bytes))
            };

            // Backfill top entry's hash if missing (one-shot migration
            // for rows written before this patch).
            if let Some(top) = items.first_mut() {
                if top.content_hash.is_none() {
                    if let Some(p) = moonbeauty::overlays::clipboard::ClipboardOverlay::get_file_path(&dir, top.id) {
                        if let Ok(b) = std::fs::read(&p) {
                            use sha2::Digest as _;
                            top.content_hash = Some(format!("{:x}", sha2::Sha256::digest(&b)));
                        }
                    }
                }
            }

            if items.first().and_then(|t| t.content_hash.as_deref())
                == Some(incoming_hash.as_str())
            {
                eprintln!("[clipboard-ingest] duplicate content, skipping insert");
                // The mbs-last-clip-name.txt we may have read from the
                // first fire is already consumed; nothing left to do.
                std::process::exit(0);
            }

            items.insert(
                0,
                moonbeauty::overlays::clipboard::ClipItem {
                    id,
                    text: text_preview,
                    is_image,
                    thumb_path: None,
                    content_hash: Some(incoming_hash),
                    source_path: None,
                },
            );
            items.truncate(100); // Keep last 100

            if let Ok(json) = serde_json::to_string(&items) {
                let tmp = dir.join("index.json.tmp");
                if std::fs::write(&tmp, &json).is_ok() {
                    let _ = std::fs::rename(&tmp, &index_path);
                    eprintln!(
                        "[clipboard-ingest] Saved {} items, is_image={}",
                        items.len(),
                        is_image
                    );
                }
            }
        }
        std::process::exit(0);
    }

    // Finding 07 defense-in-depth: restrictive ImageMagick policy that
    // every child process (annotator, watermark, reveal) inherits via
    // MAGICK_CONFIGURE_PATH.
    if let Some(dir) = moonbeauty::libraries::process::ensure_imagemagick_policy() {
        std::env::set_var("MAGICK_CONFIGURE_PATH", &dir);
    }

    // ── Lock auth test mode ──
    if args.iter().any(|a| a == "--test-lock-auth") {
        moonbeauty::overlays::lockscreen::LockCoordinator::run_auth_test();
    }

    // ── Email connection test (one-shot diagnostic) ──
    if args.iter().any(|a| a == "--test-email") {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        rt.block_on(async {
            match moonbeauty::libraries::backend::email::client::test_connection().await {
                Ok((imap, smtp, report)) => {
                    println!("{}", report);
                    if imap && smtp {
                        println!("\n✅ both imap and smtp OK");
                        std::process::exit(0);
                    } else {
                        println!("\n❌ imap={} smtp={}", imap, smtp);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("test failed: {}", e);
                    std::process::exit(2);
                }
            }
        });
    }

    // ── Multi-monitor lock coordinator (parent mode) ──
    // If called as `--lockscreen` (and NOT `--lockscreen-instance`),
    // act as the coordinator: spawn one child per output, broker auth,
    // watch hotplug. Blocks forever (exits via process::exit).
    if args.iter().any(|a| a == "--lockscreen")
        && !args.iter().any(|a| a == "--lockscreen-instance")
    {
        moonbeauty::overlays::lockscreen::LockCoordinator::spawn();
    }

    // --quickshot was replaced by --screenshot --annotate.
    if args.iter().any(|a| a == "--quickshot" || a == "quickshot") {
        eprintln!("[error] --quickshot is gone. Screenshot workflows now:");
        eprintln!("        --screenshot              capture region and save");
        eprintln!("        --screenshot --annotate   capture and open annotator");
        eprintln!("        --watermark \"name\"        embed invisible watermark");
        eprintln!();
        eprintln!("        Update your keybinds accordingly.");
        std::process::exit(2);
    }

    // Notification CLI actions
    if args.iter().any(|a| a == "--activate-latest-notif") {
        let cmd_path = moonbeauty::libraries::process::mbs_runtime_dir()
            .join("moonbeauty-notif-cmd");
        match moonbeauty::libraries::process::private_write_file(
            &cmd_path,
            b"activate-latest",
        ) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[activate-latest] write {:?}: {e}", cmd_path);
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|a| a == "--dismiss-notif") {
        let cmd_path = moonbeauty::libraries::process::mbs_runtime_dir()
            .join("moonbeauty-notif-cmd");
        match moonbeauty::libraries::process::private_write_file(&cmd_path, b"dismiss") {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[dismiss-notif] write {:?}: {e}", cmd_path);
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|a| a == "--notif-history") {
        match send_ipc_sync(&IpcCommand::ToggleHistory) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[notif-history] IPC failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // Music CLI flags — fired from the tooltip's Cycle/Prev/Play/Next
    // buttons and bindable to sway keybinds.
    for (flag, cmd) in [
        ("--music-cycle", IpcCommand::MusicCycle),
        ("--music-next", IpcCommand::MusicNext),
        ("--music-previous", IpcCommand::MusicPrevious),
        ("--music-play-pause", IpcCommand::MusicPlayPause),
    ] {
        if args.iter().any(|a| a == flag) {
            match send_ipc_sync(&cmd) {
                Ok(_) => std::process::exit(0),
                Err(e) => {
                    eprintln!("[{flag}] IPC failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    }

    // Restart bar (used by Settings when changing monitor)
    if args.iter().any(|a| a == "--restart-bar") {
        // Kill orphaned clipboard watchers FIRST. Their cmdline ends with
        // "--clipboard-ingest" (not "moonbeauty$"), so the
        // anchored shell pkill below never matches them. Without this,
        // every restart spawns a new watcher and the clipboard UI ends
        // up with N copies of each item after N restarts.
        let _ = std::process::Command::new("pkill")
            .args(["-f", "wl-paste --watch.*moonbeauty"])
            .status();
        // Finding 22: anchored patterns — require path-prefix before the
        // binary name and end-of-argv after the flag. Prevents matching
        // scripts / editors that merely mention the strings.
        let _ = std::process::Command::new("pkill")
            .args(["-f", "(^|/)(moonbeauty|moonbeauty)$"])
            .status();
        let _ = std::process::Command::new("pkill")
            .args([
                "-f",
                "(^|/)(moonbeauty|moonbeauty) (--(sys|weather|alarm|info)-tooltip|--tray-popup)$",
            ])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let exe = resolve_exe();
        let mut cmd = std::process::Command::new(exe);
        let _ = moonbeauty::libraries::process::spawn_and_reap(&mut cmd);
        std::process::exit(0);
    }

    // Generic IPC toggle: moonbeauty --toggle <overlay>
    // e.g. sway: bindsym $mod+Tab exec moonbeauty --toggle history
    if let Some(idx) = args.iter().position(|a| a == "--toggle") {
        let target_str = args.get(idx + 1).cloned().unwrap_or_default();
        match OverlayTarget::from_str(&target_str) {
            Some(target) => match send_ipc_sync(&IpcCommand::Toggle(target)) {
                Ok(_) => std::process::exit(0),
                Err(e) => {
                    eprintln!("[toggle] IPC failed: {e}");
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("[toggle] unknown target: {}", target_str);
                std::process::exit(2);
            }
        }
    }

    // Bare `--<overlay>` (e.g. `moonbeauty --history`) = implicit IPC
    // toggle, equivalent to `--toggle <overlay>`.  Explicit standalone
    // spawn is `--overlay <target>` — used internally by the bar to
    // spawn overlay children; not meant for user invocation.
    if let Some(arg) = args.get(1) {
        if let Some(name) = arg.strip_prefix("--") {
            if let Some(target) = OverlayTarget::from_str(name) {
                match send_ipc_sync(&IpcCommand::Toggle(target)) {
                    Ok(_) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("[toggle] IPC failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    // ── Screenshot CLI ────────────────────────────────────────────────
    // --screenshot [--annotate] [--watermark "name"]
    // MUST be above the notif-surface spawn block, or the flag falls
    // through and boots the full bar.
    // Temporary: verify freeze-frame capture works before UI build
    // Child: render freeze-frame selector, write crop to disk
    if args.iter().any(|a| a == "--freeze-fragment") {
        let idx = args.iter().position(|a| a == "--freeze-fragment").unwrap();
        let monitor = args.get(idx + 1).cloned().unwrap_or_default();
        let gx: i32 = args.get(idx + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let gy: i32 = args.get(idx + 3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let local_png = args.get(idx + 4).cloned().unwrap_or_default();
        let state_file = args.get(idx + 5).cloned().unwrap_or_default();
        if monitor.is_empty() || local_png.is_empty() || state_file.is_empty() {
            eprintln!("usage: --freeze-fragment <name> <gx> <gy> <local.png> <state_file>");
            std::process::exit(2);
        }
        match screenshot::fragment::run(
            monitor,
            gx,
            gy,
            std::path::PathBuf::from(local_png),
            std::path::PathBuf::from(state_file),
        ) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[freeze-fragment] {e}");
                std::process::exit(1);
            }
        }
    }

    // Orchestrator: capture focused output → spawn child → report crop
    if args.iter().any(|a| a == "--screenshot") {
        let quick = args.iter().any(|a| a == "--quick" || a == "--no-annotate");
        let annotate = !quick;
        let watermark_cli = args
            .iter()
            .position(|a| a == "--watermark")
            .and_then(|i| args.get(i + 1).cloned());
        let _ = watermark_cli;

        // Multi-monitor: capture the entire desktop once, then spawn one fragment
        // per monitor with a local crop. Fragments coordinate drags via a shared state file.
        let outputs = screenshot::output::list_all_outputs();
        if outputs.is_empty() {
            eprintln!("[screenshot] no outputs found");
            std::process::exit(1);
        }

        let full_bytes = match screenshot::capture::capture_output(None) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[screenshot] capture failed: {e}");
                std::process::exit(1);
            }
        };
        let full_img = match image::load_from_memory(&full_bytes) {
            Ok(i) => i.to_rgba8(),
            Err(e) => {
                eprintln!("[screenshot] decode failed: {e}");
                std::process::exit(1);
            }
        };

        // The full image's origin corresponds to (min_x, min_y) in global space.
        let min_x = outputs.iter().map(|(_, x, _, _, _)| *x).min().unwrap_or(0);
        let min_y = outputs.iter().map(|(_, _, y, _, _)| *y).min().unwrap_or(0);

        let state_file =
            moonbeauty::libraries::process::mbs_unique_tmp("mbs-freeze-state", "txt");
        screenshot::fragment::write_state_file(&state_file, "idle", 0, 0, 0, 0);

        let exe = resolve_exe();
        let mut children: Vec<(String, std::process::Child)> = Vec::new();
        let mut local_png_paths: Vec<std::path::PathBuf> = Vec::new();

        for (name, x, y, w, h) in &outputs {
            let lx = (*x - min_x) as u32;
            let ly = (*y - min_y) as u32;
            if lx + *w > full_img.width() || ly + *h > full_img.height() {
                continue;
            }
            let crop = image::imageops::crop_imm(&full_img, lx, ly, *w, *h).to_image();
            let local_png = moonbeauty::libraries::process::mbs_unique_tmp(
                &format!("mbs-freeze-{}", name),
                "png",
            );
            let write_ok =
                match moonbeauty::libraries::process::private_create_new(&local_png) {
                    Ok(mut f) => {
                        let ok = image::DynamicImage::ImageRgba8(crop)
                            .write_to(&mut f, image::ImageFormat::Png)
                            .is_ok();
                        if !ok {
                            let _ = std::fs::remove_file(&local_png);
                        }
                        ok
                    }
                    Err(_) => false,
                };
            if !write_ok {
                eprintln!("[screenshot] crop write failed: {}", local_png.display());
                continue;
            }
            local_png_paths.push(local_png.clone());

            match std::process::Command::new(&exe)
                .args([
                    "--freeze-fragment",
                    name,
                    &x.to_string(),
                    &y.to_string(),
                    local_png.to_str().unwrap_or(""),
                    state_file.to_str().unwrap_or(""),
                ])
                .spawn()
            {
                Ok(child) => children.push((name.clone(), child)),
                Err(e) => eprintln!("[screenshot] spawn {} failed: {e}", name),
            }
        }

        if children.is_empty() {
            eprintln!("[screenshot] no fragments spawned");
            std::process::exit(1);
        }

        // Poll state file
        let mut rect: Option<(i32, i32, i32, i32)> = None;
        loop {
            let (state, sx, sy, cx, cy) = screenshot::fragment::read_state_file(&state_file);
            if state == "done" {
                rect = Some((sx, sy, cx, cy));
                break;
            }
            if state == "cancelled" {
                break;
            }
            let mut any_alive = false;
            for (_, child) in children.iter_mut() {
                match child.try_wait() {
                    Ok(Some(_)) => {}
                    Ok(None) => any_alive = true,
                    Err(_) => {}
                }
            }
            if !any_alive {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        for (_, mut child) in children.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clean up per-monitor crops
        for p in &local_png_paths {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_file(&state_file);

        let Some((sx, sy, cx, cy)) = rect else {
            eprintln!("[screenshot] cancelled");
            std::process::exit(130);
        };

        let rx = (sx.min(cx) - min_x).max(0) as u32;
        let ry = (sy.min(cy) - min_y).max(0) as u32;
        let rw = (sx - cx).unsigned_abs();
        let rh = (sy - cy).unsigned_abs();

        if rw < 4 || rh < 4 || rx + rw > full_img.width() || ry + rh > full_img.height() {
            eprintln!("[screenshot] invalid selection");
            std::process::exit(130);
        }

        let cropped = image::imageops::crop_imm(&full_img, rx, ry, rw, rh).to_image();
        let out_path =
            moonbeauty::libraries::process::mbs_unique_tmp("mbs-freeze-out", "png");
        let save_ok = match moonbeauty::libraries::process::private_create_new(&out_path) {
            Ok(mut f) => {
                let ok = image::DynamicImage::ImageRgba8(cropped)
                    .write_to(&mut f, image::ImageFormat::Png)
                    .is_ok();
                if !ok {
                    let _ = std::fs::remove_file(&out_path);
                }
                ok
            }
            Err(_) => false,
        };
        if !save_ok {
            eprintln!("[screenshot] save crop failed");
            std::process::exit(1);
        }
        eprintln!("[screenshot] cropped → {}", out_path.display());

        if annotate {
            let cmd = IpcCommand::Open(
                OverlayTarget::Annotator,
                Some(out_path.to_string_lossy().into_owned()),
            );
            match send_ipc_sync(&cmd) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[screenshot] annotator IPC failed: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            let home = std::env::var("HOME").unwrap_or_default();
            let dir = std::path::PathBuf::from(home)
                .join("Pictures")
                .join("Screenshots");
            let _ = std::fs::create_dir_all(&dir);
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let dest = dir.join(format!("shot_{}.jxl", ts));
            match screenshot::annotator::png_to_jxl(&out_path, &dest) {
                Ok(()) => eprintln!("[screenshot] saved → {}", dest.display()),
                Err(e) => {
                    eprintln!("[screenshot] JXL encode failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        std::process::exit(0);
    }

    // CLI Notification Push: moonbeauty --notify "LusterDawn" "Message"
    if args.len() >= 3
        && (args[1] == "--notify" || (args.len() >= 4 && args[1] == "--ipc" && args[2] == "notify"))
    {
        let summary = if args[1] == "--notify" {
            &args[2]
        } else {
            &args[3]
        };
        let body = if args[1] == "--notify" {
            args.get(3).map(|s| s.as_str()).unwrap_or("")
        } else {
            args.get(4).map(|s| s.as_str()).unwrap_or("")
        };
        let _ = std::process::Command::new("notify-send")
            .args(["-a", "Desktop", "--", summary, body])
            .spawn();
        std::process::exit(0);
    }

    if let Some(idx) = args.iter().position(|a| a == "--tray-context-menu") {
        let json_path = args.get(idx + 1).cloned().unwrap_or_default();
        let bus = args.get(idx + 2).cloned().unwrap_or_default();
        let menu_path = args.get(idx + 3).cloned().unwrap_or_default();
        let w = args
            .get(idx + 4)
            .and_then(|s| s.parse().ok())
            .unwrap_or(250u32);
        let h = args
            .get(idx + 5)
            .and_then(|s| s.parse().ok())
            .unwrap_or(400u32);
        if let Ok(json) = std::fs::read_to_string(&json_path) {
            if let Ok(items) =
                serde_json::from_str::<Vec<moonbeauty::bar::tray::TrayMenuItem>>(&json)
            {
                let _ = std::fs::remove_file(&json_path);
                std::process::exit(
                    match moonbeauty::bar::tray::context::run(items, bus, menu_path, w, h) {
                        Ok(_) => 0,
                        Err(_) => 1,
                    },
                );
            }
        }
        std::process::exit(1);
    }

    let is_notif_surface = detect_notif_surface(&args);

    // Finding 68: notif-surface reads its target monitor from
    // settings.json directly, ignoring inherited MBS_OUTPUT. This
    // eliminates the dual-spawn-path race between bar-spawn and
    // D-Bus/systemd activation — whichever parent launches us, the
    // child resolves the same value that's on disk right now.
    if is_notif_surface {
        let s = ShellSettings::load();
        let target = &s.notif_screen_name;
        if !target.is_empty() && target != "Auto" && target != "Auto (Primary)" {
            std::env::set_var("MBS_OUTPUT", target);
            eprintln!(
                "[notif-surface] MBS_OUTPUT={:?} (from settings.json, overrides env)",
                target
            );
        } else {
            std::env::remove_var("MBS_OUTPUT");
            eprintln!("[notif-surface] MBS_OUTPUT unset (settings=Auto; using sway primary)");
        }
    }

    let standalone_target = detect_standalone_overlay(&args);
    eprintln!(
        "[main-v225] standalone_target={:?} is_notif={}",
        standalone_target, is_notif_surface
    );
    let settings = ShellSettings::load();

    if !is_notif_surface && standalone_target.is_none() {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "--", "(^|/)(moonbeauty|moonbeauty) --notif-surface$"])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let exe = resolve_exe();
        // Finding 65: pass MBS_OUTPUT explicitly from settings. The parent
        // env's MBS_OUTPUT is resolved once at bar startup and becomes stale
        // when notif_screen_name changes at runtime; external MBS_OUTPUT
        // (systemd Environment) silently overrides the setting entirely.
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--notif-surface");
        let target = &settings.notif_screen_name;
        if !target.is_empty() && target != "Auto" && target != "Auto (Primary)" {
            cmd.env("MBS_OUTPUT", target);
        }
        // else: inherit parent MBS_OUTPUT (respects external override)
        let _ = moonbeauty::libraries::process::spawn_and_reap(&mut cmd);
    }

    // Fast path: bar already set MBS_OUTPUT — child inherits it.
    let resolved_output: Option<String> = if std::env::var("MBS_OUTPUT").is_ok() {
        None
    } else if settings.notif_screen_name != "Auto"
        && settings.notif_screen_name != "Auto (Primary)"
        && !settings.notif_screen_name.is_empty()
    {
        Some(settings.notif_screen_name.clone())
    } else {
        // Single swaymsg call; prefer primary, then focused, in one pass.
        std::process::Command::new("swaymsg")
            .args(["-t", "get_outputs", "-r"])
            .output()
            .ok()
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .and_then(|v| {
                let arr = v.as_array()?;
                for o in arr {
                    if o.get("primary").and_then(|p| p.as_bool()) == Some(true) {
                        return o.get("name").and_then(|n| n.as_str()).map(String::from);
                    }
                }
                for o in arr {
                    if o.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                        return o.get("name").and_then(|n| n.as_str()).map(String::from);
                    }
                }
                None
            })
    };
    // Only the top-level bar process resolves MBS_OUTPUT.
    // Child processes (tooltips, notif-surface, standalone overlays) inherit
    // it via std::process env, guaranteeing they all agree on the target output.
    if std::env::var("MBS_OUTPUT").is_err() {
        if let Some(ref name) = resolved_output {
            std::env::set_var("MBS_OUTPUT", name);
            eprintln!("[bar] resolved binded_output_name → {} (parent)", name);
        }
    } else {
        eprintln!(
            "[child] MBS_OUTPUT inherited → {:?}",
            std::env::var("MBS_OUTPUT")
        );
    }
    let binded_output_name = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());

    if is_notif_surface {
        eprintln!(
            "[notif-surface] startup binded_output_name={:?}",
            binded_output_name
        );
    }
    let layer_settings = if is_notif_surface {
        let corner = &settings.notif_corner;
        let is_top = corner.contains("top");
        let is_right = corner.contains("right");
        let anchor = match (is_top, is_right) {
            (true, true) => Anchor::Top | Anchor::Right,
            (true, false) => Anchor::Top | Anchor::Left,
            (false, true) => Anchor::Bottom | Anchor::Right,
            (false, false) => Anchor::Bottom | Anchor::Left,
        };
        let _strip_w =
            (settings.notif_card_width as i32 + 2 * settings.notif_margin_x + 40).max(200) as u32;
        let _strip_h = (3 * settings.notif_card_height as i32 + 2 * settings.notif_margin_x + 100)
            .max(300) as u32;
        LayerShellSettings {
            size: Some((1, 1)),
            exclusive_zone: -1,
            anchor,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::None,
            events_transparent: false,
            start_mode: binded_output_name
                .map(iced_layershell::settings::StartMode::TargetScreen)
                .unwrap_or(iced_layershell::settings::StartMode::Active),
            ..Default::default()
        }
    } else if standalone_target.as_ref() == Some(&OverlayTarget::Lockscreen) {
        // Each lock child is bound to a specific output (or the primary if
        // no arg was provided — single-monitor fallback).
        let lock_output = std::env::var("MBS_LOCK_OUTPUT")
            .ok()
            .filter(|s| !s.is_empty())
            .or(binded_output_name.clone());
        LayerShellSettings {
            size: None,
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            events_transparent: false,
            start_mode: (lock_output)
                .map(iced_layershell::settings::StartMode::TargetScreen)
                .unwrap_or(iced_layershell::settings::StartMode::Active),
            ..Default::default()
        }
    } else if standalone_target.as_ref() == Some(&OverlayTarget::Annotator) {
        // Floating annotator window. wlr-layer-shell centers when all four
        // sides are anchored with a fixed size. Overlay layer keeps it above
        // the bar. Tune (W, H) to taste — this reads well on 1440p.
        LayerShellSettings {
            size: Some((1600, 1000)),
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            events_transparent: false,
            start_mode: binded_output_name
                .map(iced_layershell::settings::StartMode::TargetScreen)
                .unwrap_or(iced_layershell::settings::StartMode::Active),
            ..Default::default()
        }
    } else if standalone_target.is_some() {
        LayerShellSettings {
            size: Some((1600, 1000)),
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            events_transparent: false,
            start_mode: (None)
                .map(iced_layershell::settings::StartMode::TargetScreen)
                .unwrap_or(iced_layershell::settings::StartMode::Active),
            ..Default::default()
        }
    } else {
        LayerShellSettings {
            size: Some((0, settings.bar_height)),
            exclusive_zone: settings.bar_height as i32,
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            layer: Layer::Top,
            keyboard_interactivity: KeyboardInteractivity::None,
            events_transparent: false,
            start_mode: match settings.bar_monitor.as_str() {
                "All" | "" => iced_layershell::settings::StartMode::Active,
                n => iced_layershell::settings::StartMode::TargetScreen(n.to_string()),
            },
            ..Default::default()
        }
    };

    if args.iter().any(|a| a == "--sys-tooltip") {
        match moonbeauty::overlays::system_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[sys-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--weather-tooltip") {
        match moonbeauty::bar::weather::tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[weather-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--info-tooltip") {
        match moonbeauty::overlays::info_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[info-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    let _app = iced_layershell::application(
        MoonBeautyShellApp::new_app,
        || "moonbeauty".to_string(),
        MoonBeautyShellApp::update,
        MoonBeautyShellApp::view,
    )
    .subscription(MoonBeautyShellApp::subscription)
    .theme(app_theme)
    .style(app_style)
    .settings(Settings {
        layer_settings,
        ..Default::default()
    });
    _app.run()
}

fn handle_lock_keypress(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    if status == iced::event::Status::Captured {
        return None;
    }
    let debug = std::env::var_os("MBS_LOCK_DEBUG").is_some();
    moonbeauty::overlays::lockscreen::resolve_lock_key(event).map(|k| {
        if debug {
            eprintln!("[lock-key] resolved: {:?}", k);
        }
        Message::Lock(LockMessage::KeyPress(k))
    })
}

fn handle_email_keypress(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event
    else {
        return None;
    };
    use iced::keyboard::{key::Named, Key};

    // ---- SEND: three shortcuts to maximize compatibility ----
    if key == Key::Named(Named::Enter) && (modifiers.control() || modifiers.alt()) {
        eprintln!("[key] Send triggered via Enter+mod");
        return Some(Message::Email(EmailMessage::SendMail));
    }
    if let Key::Character(c) = &key {
        if modifiers.control() && c.to_lowercase() == "s" {
            eprintln!("[key] Send triggered via Ctrl+S");
            return Some(Message::Email(EmailMessage::SendMail));
        }
        if modifiers.control() && c.to_lowercase() == "o" {
            eprintln!("[key] Attach triggered via Ctrl+O");
            return Some(Message::Email(EmailMessage::AddAttachment));
        }
    }

    if key == Key::Named(Named::Tab) {
        return Some(Message::ComposeTabPressed(modifiers.shift()));
    }

    if modifiers.alt() {
        match key {
            Key::Named(Named::ArrowUp) => {
                return Some(Message::Email(EmailMessage::SelectFolderPrev))
            }
            Key::Named(Named::ArrowDown) => {
                return Some(Message::Email(EmailMessage::SelectFolderNext))
            }
            _ => {}
        }
    }

    if modifiers.control() {
        if let Key::Character(c) = &key {
            match c.to_lowercase().as_str() {
                "n" | "j" => return Some(Message::Email(EmailMessage::SelectMailNext)),
                "p" | "k" => return Some(Message::Email(EmailMessage::SelectMailPrev)),
                _ => {}
            }
        }
    }

    if matches!(key, Key::Character(_))
        && status == iced::event::Status::Captured
        && !modifiers.control()
    {
        return None;
    }

    match key {
        Key::Named(Named::ArrowUp) => Some(Message::Email(EmailMessage::SelectMailPrev)),
        Key::Named(Named::ArrowDown) => Some(Message::Email(EmailMessage::SelectMailNext)),
        Key::Named(Named::Enter) => Some(Message::Email(EmailMessage::OpenSelected)),
        Key::Named(Named::Delete) => Some(Message::Email(EmailMessage::DeleteSelected)),
        Key::Character(c) if c == "n" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::OpenComposer))
        }
        Key::Character(c) if c == "s" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::ToggleStar))
        }
        Key::Character(c) if c == "d" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::DeleteSelected))
        }
        Key::Character(c) if c == "c" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::SaveContact))
        }
        _ => None,
    }
}

/// Keyboard handler for annotator text-annotation mode. Forwards printable
/// characters, Enter, and Backspace to the annotator's text buffer. Modifier
/// combos fall through (they're reserved for tool shortcuts / global binds);
/// events already captured by a focused widget (e.g. the watermark
/// text_input) are skipped so we don't double-handle them.
fn handle_annotator_text_input(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        modified_key,
        text,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    use iced::keyboard::{key::Named, Key};

    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    if status == iced::event::Status::Captured {
        return None;
    }

    // Named keys first (Enter's `text` is Some("\r"), a control char —
    // must be intercepted here before the printable filter below).
    if let Key::Named(named) = &modified_key {
        match named {
            Named::Enter => {
                return Some(Message::Annotator(AnnotatorMessage::NewLinePressed));
            }
            Named::Backspace => {
                return Some(Message::Annotator(AnnotatorMessage::BackspacePressed));
            }
            _ => {}
        }
    }

    // Printable: prefer `text` (respects IME, layout, shift/caps). Convert
    // SmolStr → String because AnnotatorMessage::CharacterInput takes String.
    if let Some(t) = text {
        if !t.is_empty() && !t.chars().any(|c| c.is_control()) {
            return Some(Message::Annotator(AnnotatorMessage::CharacterInput(
                t.to_string(),
            )));
        }
    }

    // Fallback: modified_key.
    if let Key::Character(c) = modified_key {
        if !c.is_empty() {
            return Some(Message::Annotator(AnnotatorMessage::CharacterInput(
                c.to_string(),
            )));
        }
    }

    None
}


