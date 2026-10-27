use iced::widget::{container, Stack};
use iced::{Element, Length, Padding, Task as Command};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use moon_beauty_shell::libraries::backend::*;
use moon_beauty_shell::bar::{CapsuleAction, TopBar, TopBarMessage};
use moon_beauty_shell::libraries::config::{ShellSettings, ShellTheme};
use moon_beauty_shell::libraries::ipc::{send_client_command, IpcCommand, OverlayTarget};
use moon_beauty_shell::overlays::*;
use moon_beauty_shell::bar::geometry::{
    capsule_actual_width, capsule_center_x,
};
use moon_beauty_shell::bar::outputs::{get_screen_width_for, resolve_primary_output};
use moon_beauty_shell::libraries::process::resolve_exe;

use moon_beauty_shell::overlays::screenshot::annotator::Tool;


use moon_beauty_shell::overlays::screenshot;
use moon_beauty_shell::bar::unified::UnifiedTelemetry;

mod app;

use app::{Message, MoonBeautyShellApp};

fn detect_notif_surface(args: &[String]) -> bool {
    args.iter().any(|a| a == "--notif-surface")
}

fn detect_standalone_overlay(args: &[String]) -> Option<OverlayTarget> {
    for (i, arg) in args.iter().enumerate() {
        if arg == "--lockscreen-instance" { return Some(OverlayTarget::Lockscreen); }
        if arg == "--annotator-session" { return Some(OverlayTarget::Annotator); }
        if arg == "--overlay" || arg == "--ipc" {
            if let Some(target_str) = args.get(i + 1) {
                return OverlayTarget::from_str(target_str);
            }
        }
        if let Some(flag) = arg.strip_prefix("--") {
            if let Some(target) = OverlayTarget::from_str(flag) {
                return Some(target);
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
        .map(|d| std::path::PathBuf::from(d).join("moon-beauty-shell.sock"))
        .map_err(|e| format!("XDG_RUNTIME_DIR: {e}"))?;
    let mut stream = UnixStream::connect(&path)?;
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
    cand_set.into_iter().filter(|s| !s.is_empty()).collect()
}

fn is_app_focused(candidates: &[String]) -> bool {
    let Some(focused) = focused_app_id() else { return false; };
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
    let candidates: Vec<String> = cand_set.into_iter().filter(|s| !s.is_empty()).collect();
    eprintln!("[focus] app={:?} entry={:?} candidates={:?}", app_name, desktop_entry, candidates);

    if std::env::var("SWAYSOCK").is_ok() {
        for cand in &candidates {
            let _ = std::process::Command::new("swaymsg")
                .args(["-t", "command", &format!("[app_id=\"{}\"] focus", cand.trim())])
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
        let ok = std::process::Command::new("gtk-launch")
            .arg(de_clean)
            .status().map(|s| s.success()).unwrap_or(false);
        if ok { return true; }
    }

    let _ = std::process::Command::new("gtk-launch")
        .arg(&candidates[0]).status();
    false
}

fn focused_app_id() -> Option<String> {
    let out = std::process::Command::new("swaymsg")
        .args(["-t", "get_tree", "-r"])
        .stderr(std::process::Stdio::null())
        .output().ok()?;
    if !out.status.success() { return None; }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    fn walk(node: &serde_json::Value) -> Option<String> {
        if node.get("focused").and_then(|f| f.as_bool()) == Some(true) {
            if let Some(app_id) = node.get("app_id").and_then(|a| a.as_str()) {
                if !app_id.is_empty() { return Some(app_id.to_string()); }
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(nodes) = node.get(key).and_then(|n| n.as_array()) {
                for child in nodes {
                    if let Some(r) = walk(child) { return Some(r); }
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

    // ── Lock auth test mode ──
    if args.iter().any(|a| a == "--test-lock-auth") {
        moon_beauty_shell::overlays::lockscreen::LockCoordinator::run_auth_test();
    }

    // ── Multi-monitor lock coordinator (parent mode) ──
    // If called as `--lockscreen` (and NOT `--lockscreen-instance`),
    // act as the coordinator: spawn one child per output, broker auth,
    // watch hotplug. Blocks forever (exits via process::exit).
    if args.iter().any(|a| a == "--lockscreen")
        && !args.iter().any(|a| a == "--lockscreen-instance")
    {
        moon_beauty_shell::overlays::lockscreen::LockCoordinator::spawn();
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
        let runtime = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
        let cmd_path = std::path::PathBuf::from(runtime)
            .join("moon-beauty-shell-notif-cmd");
        match std::fs::write(&cmd_path, b"activate-latest") {
            Ok(_) => std::process::exit(0),
            Err(e) => { eprintln!("[activate-latest] write {:?}: {e}", cmd_path); std::process::exit(1); }
        }
    }
    if args.iter().any(|a| a == "--dismiss-notif") {
        let runtime = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
        let cmd_path = std::path::PathBuf::from(runtime)
            .join("moon-beauty-shell-notif-cmd");
        match std::fs::write(&cmd_path, b"dismiss") {
            Ok(_) => std::process::exit(0),
            Err(e) => { eprintln!("[dismiss-notif] write {:?}: {e}", cmd_path); std::process::exit(1); }
        }
    }
    if args.iter().any(|a| a == "--notif-history") {
        match send_ipc_sync(&IpcCommand::ToggleHistory) {
            Ok(_) => std::process::exit(0),
            Err(e) => { eprintln!("[notif-history] IPC failed: {e}"); std::process::exit(1); }
        }
    }

    // Restart bar (used by Settings when changing monitor)
    if args.iter().any(|a| a == "--restart-bar") {
        // Kill ONLY the bar (end of argv is exactly "moon-beauty-shell"),
        // and the orphaned tooltips that belong to it. NOT --settings,
        // NOT --notif-surface, NOT --restart-bar itself.
        let _ = std::process::Command::new("pkill")
            .args(["-f", "target/release/moon-beauty-shell$"])
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-f", "-sys-tooltip|-weather-tooltip|-alarm-tooltip|-tray-popup|-info-tooltip"])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let exe = resolve_exe();
        let _ = std::process::Command::new(exe).spawn();
        std::process::exit(0);
    }

    // Generic IPC toggle: moon-beauty-shell --toggle <overlay>
    // e.g. sway: bindsym $mod+Tab exec moon-beauty-shell --toggle history
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

    // ── Screenshot CLI ────────────────────────────────────────────────
    // --screenshot [--annotate] [--watermark "name"]
    // MUST be above the notif-surface spawn block, or the flag falls
    // through and boots the full bar.
    // Temporary: verify freeze-frame capture works before UI build
    if args.iter().any(|a| a == "--capture-test") {
        match screenshot::capture::capture_fullscreen() {
            Ok(bytes) => {
                let path = "/tmp/mbs-capture-test.png";
                let _ = std::fs::write(path, &bytes);
                match image::load_from_memory(&bytes) {
                    Ok(img) => println!(
                        "captured {}x{} → {} ({} bytes)",
                        img.width(), img.height(), path, bytes.len()
                    ),
                    Err(e) => eprintln!("decode: {e}"),
                }
            }
            Err(e) => eprintln!("capture failed: {e}"),
        }
        std::process::exit(0);
    }

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
            Err(e) => { eprintln!("[freeze-fragment] {e}"); std::process::exit(1); }
        }
    }

        // Orchestrator: capture focused output → spawn child → report crop
        if args.iter().any(|a| a == "--screenshot") {
        let quick = args.iter().any(|a| a == "--quick" || a == "--no-annotate");
        let annotate = !quick;
        let watermark_cli = args.iter()
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
            Err(e) => { eprintln!("[screenshot] capture failed: {e}"); std::process::exit(1); }
        };
        let full_img = match image::load_from_memory(&full_bytes) {
            Ok(i) => i.to_rgba8(),
            Err(e) => { eprintln!("[screenshot] decode failed: {e}"); std::process::exit(1); }
        };

        // The full image's origin corresponds to (min_x, min_y) in global space.
        let min_x = outputs.iter().map(|(_, x, _, _, _)| *x).min().unwrap_or(0);
        let min_y = outputs.iter().map(|(_, _, y, _, _)| *y).min().unwrap_or(0);

        let pid = std::process::id();
        let state_file = std::path::PathBuf::from(format!("/tmp/mbs-freeze-state-{}.txt", pid));
        screenshot::fragment::write_state_file(&state_file, "idle", 0, 0, 0, 0);

        let exe = resolve_exe();
        let mut children: Vec<(String, std::process::Child)> = Vec::new();

        for (name, x, y, w, h) in &outputs {
            let lx = (*x - min_x) as u32;
            let ly = (*y - min_y) as u32;
            if lx + *w > full_img.width() || ly + *h > full_img.height() {
                continue;
            }
            let crop = image::imageops::crop_imm(&full_img, lx, ly, *w, *h).to_image();
            let local_png = format!("/tmp/mbs-freeze-{}-{}.png", name, pid);
            if crop.save(&local_png).is_err() { continue; }

            match std::process::Command::new(&exe)
                .args([
                    "--freeze-fragment",
                    name,
                    &x.to_string(),
                    &y.to_string(),
                    &local_png,
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
            if state == "done" { rect = Some((sx, sy, cx, cy)); break; }
            if state == "cancelled" { break; }
            let mut any_alive = false;
            for (_, child) in children.iter_mut() {
                match child.try_wait() {
                    Ok(Some(_)) => {}
                    Ok(None) => any_alive = true,
                    Err(_) => {}
                }
            }
            if !any_alive { break; }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        for (_, mut child) in children.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clean up per-monitor crops
        for (name, _, _, _, _) in &outputs {
            let _ = std::fs::remove_file(format!("/tmp/mbs-freeze-{}-{}.png", name, pid));
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
        let out_path = format!("/tmp/mbs-freeze-out-{}.png", pid);
        if cropped.save(&out_path).is_err() {
            eprintln!("[screenshot] save crop failed");
            std::process::exit(1);
        }
        eprintln!("[screenshot] cropped → {}", out_path);

        if annotate {
            let cmd = IpcCommand::Open(OverlayTarget::Annotator, Some(out_path.clone()));
            match send_ipc_sync(&cmd) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[screenshot] annotator IPC failed: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            let home = std::env::var("HOME").unwrap_or_default();
            let dir = std::path::PathBuf::from(home).join("Pictures").join("Screenshots");
            let _ = std::fs::create_dir_all(&dir);
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            let dest = dir.join(format!("shot_{}.png", ts));
            let _ = std::fs::copy(&out_path, &dest);
            eprintln!("[screenshot] saved → {}", dest.display());
        }
        std::process::exit(0);
    }

    // CLI Notification Push: moon-beauty-shell --notify "LusterDawn" "Message"
    if args.len() >= 3 && (args[1] == "--notify" || (args.len() >= 4 && args[1] == "--ipc" && args[2] == "notify")) {
        let summary = if args[1] == "--notify" { &args[2] } else { &args[3] };
        let body = if args[1] == "--notify" { args.get(3).map(|s| s.as_str()).unwrap_or("") } else { args.get(4).map(|s| s.as_str()).unwrap_or("") };
        let _ = std::process::Command::new("notify-send")
            .args(["-a", "Desktop", summary, body])
            .spawn();
        std::process::exit(0);
    }

    if let Some(idx) = args.iter().position(|a| a == "--tray-context-menu") {
        let json_path = args.get(idx + 1).cloned().unwrap_or_default();
        let bus = args.get(idx + 2).cloned().unwrap_or_default();
        let menu_path = args.get(idx + 3).cloned().unwrap_or_default();
        let w = args.get(idx + 4).and_then(|s| s.parse().ok()).unwrap_or(250u32);
        let h = args.get(idx + 5).and_then(|s| s.parse().ok()).unwrap_or(400u32);
        if let Ok(json) = std::fs::read_to_string(&json_path) {
            if let Ok(items) = serde_json::from_str::<Vec<moon_beauty_shell::bar::tray::TrayMenuItem>>(&json) {
                let _ = std::fs::remove_file(&json_path);
                std::process::exit(match moon_beauty_shell::bar::tray::context::run(items, bus, menu_path, w, h) {
                    Ok(_) => 0,
                    Err(_) => 1,
                });
            }
        }
        std::process::exit(1);
    }

    let is_notif_surface = detect_notif_surface(&args);
    let standalone_target = detect_standalone_overlay(&args);
    eprintln!("[main-v225] standalone_target={:?} is_notif={}", standalone_target, is_notif_surface);
    let settings = ShellSettings::load();

    if !is_notif_surface && standalone_target.is_none() {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "--", "moon-beauty-shell --notif-surface"])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let exe = resolve_exe();
        let _ = std::process::Command::new(exe).arg("--notif-surface").spawn();
    }

    let resolved_output: Option<String> = if settings.notif_screen_name != "Auto"
        && settings.notif_screen_name != "Auto (Primary)"
        && !settings.notif_screen_name.is_empty()
    {
        Some(settings.notif_screen_name.clone())
    } else {
        // Resolve ONCE at startup. Prefer primary; fall back to focused.
        let primary = std::process::Command::new("swaymsg")
            .args(["-t", "get_outputs", "-r"]).output().ok()
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .and_then(|v| {
                let arr = v.as_array()?;
                for o in arr {
                    if o.get("primary").and_then(|p| p.as_bool()) == Some(true) {
                        return o.get("name").and_then(|n| n.as_str()).map(String::from);
                    }
                }
                None
            });
        let focused = std::process::Command::new("swaymsg")
            .args(["-t", "get_outputs", "-r"]).output().ok()
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .and_then(|v| {
                let arr = v.as_array()?;
                for o in arr {
                    if o.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                        return o.get("name").and_then(|n| n.as_str()).map(String::from);
                    }
                }
                None
            });
        primary.or(focused)
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
        eprintln!("[child] MBS_OUTPUT inherited → {:?}", std::env::var("MBS_OUTPUT"));
    }
    let binded_output_name = std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty());

    if is_notif_surface {
        eprintln!("[notif-surface] startup binded_output_name={:?}", binded_output_name);
    }
    let layer_settings = if is_notif_surface {
        let corner = &settings.notif_corner;
        let is_top = corner.contains("top");
        let is_right = corner.contains("right");
        let anchor = match (is_top, is_right) {
            (true, true)   => Anchor::Top    | Anchor::Right,
            (true, false)  => Anchor::Top    | Anchor::Left,
            (false, true)  => Anchor::Bottom | Anchor::Right,
            (false, false) => Anchor::Bottom | Anchor::Left,
        };
        let _strip_w = (settings.notif_card_width as i32
            + 2 * settings.notif_margin_x + 40).max(200) as u32;
        let _strip_h = (3 * settings.notif_card_height as i32
            + 2 * settings.notif_margin_x + 100).max(300) as u32;
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
        let lock_output = std::env::var("MBS_LOCK_OUTPUT").ok()
            .filter(|s| !s.is_empty())
            .or(binded_output_name.clone());
        LayerShellSettings {
            size: None,
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            events_transparent: false,
            start_mode: (lock_output).map(iced_layershell::settings::StartMode::TargetScreen).unwrap_or(iced_layershell::settings::StartMode::Active),
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
            start_mode: (None).map(iced_layershell::settings::StartMode::TargetScreen).unwrap_or(iced_layershell::settings::StartMode::Active),
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
        match moon_beauty_shell::overlays::system_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => { eprintln!("[sys-tooltip] {e}"); std::process::exit(1); }
        }
    }


    if args.iter().any(|a| a == "--weather-tooltip") {
        match moon_beauty_shell::bar::weather::tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => { eprintln!("[weather-tooltip] {e}"); std::process::exit(1); }
        }
    }

    if args.iter().any(|a| a == "--info-tooltip") {
        match moon_beauty_shell::overlays::info_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => { eprintln!("[info-tooltip] {e}"); std::process::exit(1); }
        }
    }


    iced_layershell::application(
        MoonBeautyShellApp::new_app,
        || "moon-beauty-shell".to_string(),
        MoonBeautyShellApp::update,
        MoonBeautyShellApp::view,
    )
    .subscription(MoonBeautyShellApp::subscription)
    .theme(app_theme)
    .style(app_style)
    .settings(Settings {
        layer_settings,
        ..Default::default()
    })
    .run()
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
    moon_beauty_shell::overlays::lockscreen::resolve_lock_key(event).map(|k| {
        if debug {
            eprintln!("[lock-key] resolved: {:?}", k);
        }
        Message::Lock(LockMessage::KeyPress(k))
    })
}

fn handle_email_keypress(event: iced::Event, status: iced::event::Status, _window: iced::window::Id) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event else { return None; };
    use iced::keyboard::{Key, key::Named};
    
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
            Key::Named(Named::ArrowUp) => return Some(Message::Email(EmailMessage::SelectFolderPrev)),
            Key::Named(Named::ArrowDown) => return Some(Message::Email(EmailMessage::SelectFolderNext)),
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

    if matches!(key, Key::Character(_)) && status == iced::event::Status::Captured && !modifiers.control() {
        return None;
    }

    match key {
        Key::Named(Named::ArrowUp) => Some(Message::Email(EmailMessage::SelectMailPrev)),
        Key::Named(Named::ArrowDown) => Some(Message::Email(EmailMessage::SelectMailNext)),
        Key::Named(Named::Enter) => Some(Message::Email(EmailMessage::OpenSelected)),
        Key::Named(Named::Delete) => Some(Message::Email(EmailMessage::DeleteSelected)),
        Key::Character(c) if c == "n" && !modifiers.control() => Some(Message::Email(EmailMessage::OpenComposer)),
        Key::Character(c) if c == "s" && !modifiers.control() => Some(Message::Email(EmailMessage::ToggleStar)),
        Key::Character(c) if c == "d" && !modifiers.control() => Some(Message::Email(EmailMessage::DeleteSelected)),
        Key::Character(c) if c == "c" && !modifiers.control() => Some(Message::Email(EmailMessage::SaveContact)),
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
        modified_key, text, modifiers, ..
    }) = event
    else {
        return None;
    };
    use iced::keyboard::{Key, key::Named};

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




impl MoonBeautyShellApp {
    /// Compute surface-local rectangles for each currently-active toast.
    /// Coordinates mirror `view_toasts`' resting pin positions (post-drop,
    /// post-expand) so the input region matches where toasts actually sit
    /// once their slide-in animation settles.
    pub fn compute_toast_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        let s = &self.settings;
        let is_right = s.notif_corner.contains("right");
        let is_top = s.notif_corner.contains("top");

        let card_w = s.notif_card_width as i32;
        let card_h = s.notif_card_height as i32;
        let margin_x = s.notif_margin_x.max(16);
        let baseline = s.notif_baseline_y;
        let surface_w =
            (s.notif_card_width as i32 + 2 * s.notif_margin_x + 100).max(200);

        let pad_top = if is_top { baseline } else { 0 };
        let pad_left = if is_right { 0 } else { margin_x };
        let pad_right = if is_right { margin_x } else { 0 };

        let peek = s.notif_stack_overlap.abs().max(4);
        let drop_dist = s.notif_drop_distance_px as i32;
        let fixed_base = drop_dist + 4 * peek;

        let x_surface = if is_right {
            surface_w - pad_right - card_w
        } else {
            pad_left
        };

        let n = self.notifs.active_toasts.len();
        let mut rects = Vec::with_capacity(n);
        for i in 0..n {
            let y_surface = pad_top + fixed_base - (i as i32) * peek;
            rects.push((x_surface, y_surface, card_w, card_h));
        }
        rects
    }

    pub fn close_tray(&mut self) {
        eprintln!(
            "[close-tray] invoked: expanded={} width={:.1} target_set={} (settings.tray_width)",
            self.tray_expanded, self.tray_anim_width, self.settings.tray_width
        );
        if self.tray_expanded {
            self.tray_expanded = false;
            self.tray_anim_start_width = self.tray_anim_width;
            self.tray_anim_started_at = std::time::Instant::now();
            self.tray_target_width = self.settings.tray_width as f32;
            self.tray_anim_duration_ms = 180;
        }
        // Invalidate any pending timeout timers
        self.tray_timeout_gen = self.tray_timeout_gen.wrapping_add(1);
    }

    pub fn standalone_running(&mut self) -> Option<OverlayTarget> {
        if let Some((t, child)) = self.standalone_child.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                return Some(t.clone());
            }
        }
        if self.standalone_child.is_some() {
            if let Some((_, mut child)) = self.standalone_child.take() {
                let _ = child.wait();
            }
        }
        None
    }

    pub fn kill_standalone(&mut self) {
        if let Some((target, mut child)) = self.standalone_child.take() {
            // For lockscreen, the coordinator normally kills its own
            // children on clean exit — but SIGKILL bypasses that path,
            // leaving orphan `--lockscreen-instance` children holding
            // dead sockets. Explicitly pkill them so the fresh coordinator
            // binds cleanly without competing for the same outputs.
            if matches!(target, OverlayTarget::Lockscreen) {
                let _ = std::process::Command::new("pkill")
                    .args(["-KILL", "-f", "--", "--lockscreen-instance"])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn spawn_standalone(&mut self, target: OverlayTarget) {
        self.kill_standalone();
        let exe = resolve_exe();
        let tag = target.as_arg();
        let log_path = format!("/tmp/mbs-standalone-{}.log", tag);
        let stderr_file = std::fs::OpenOptions::new()
            .create(true).truncate(true).write(true)
            .open(&log_path).ok();
        let stdout_file = stderr_file.as_ref().and_then(|f| f.try_clone().ok());

        let mut cmd = std::process::Command::new(exe);
        cmd.arg(format!("--{}", tag));
        if let Some(sf) = stderr_file { cmd.stderr(sf); }
        if let Some(of) = stdout_file { cmd.stdout(of); }

        match cmd.spawn() {
            Ok(c) => {
                eprintln!("[ipc] spawned {:?} pid={} (log: {})", target, c.id(), log_path);
                self.standalone_child = Some((target, c));
            }
            Err(e) => eprintln!("[ipc] spawn {:?} failed: {e}", target),
        }
    }

    pub fn toggle_standalone(&mut self, target: OverlayTarget) {
        match self.standalone_running() {
            Some(ref t) if t == &target => self.kill_standalone(),
            _ => self.spawn_standalone(target),
        }
    }
}

impl MoonBeautyShellApp {

    pub fn new_app() -> (Self, Command<Message>) {
        let args: Vec<String> = std::env::args().collect();
        let standalone_target = detect_standalone_overlay(&args);
        if let Some(idx) = args.iter().position(|a| a == "--tray-context-menu") {
        let json_path = args.get(idx + 1).cloned().unwrap_or_default();
        let bus = args.get(idx + 2).cloned().unwrap_or_default();
        let menu_path = args.get(idx + 3).cloned().unwrap_or_default();
        let w = args.get(idx + 4).and_then(|s| s.parse().ok()).unwrap_or(250u32);
        let h = args.get(idx + 5).and_then(|s| s.parse().ok()).unwrap_or(400u32);
        if let Ok(json) = std::fs::read_to_string(&json_path) {
            if let Ok(items) = serde_json::from_str::<Vec<moon_beauty_shell::bar::tray::TrayMenuItem>>(&json) {
                let _ = std::fs::remove_file(&json_path);
                std::process::exit(match moon_beauty_shell::bar::tray::context::run(items, bus, menu_path, w, h) {
                    Ok(_) => 0,
                    Err(_) => 1,
                });
            }
        }
        std::process::exit(1);
    }

    let is_notif_surface = detect_notif_surface(&args);
        let is_standalone = standalone_target.is_some();
        let settings = ShellSettings::load();
        let _tray_width_px = settings.tray_width as f32;
        eprintln!("[startup] tray_width={} tray_timeout={}s", settings.tray_width, settings.tray_timeout_sec);
        let theme = ShellTheme::from_settings(&settings);

        let (ipc_tx, ipc_rx) = std::sync::mpsc::channel::<IpcCommand>();
        if standalone_target.is_none() && !is_notif_surface {
            std::thread::spawn(move || {
                use std::io::Read;
                use std::os::unix::net::UnixListener;
                let path = moon_beauty_shell::libraries::ipc::get_socket_path();
                let _ = std::fs::remove_file(&path);
                let listener = match UnixListener::bind(&path) {
                    Ok(l) => { eprintln!("[ipc] listener bound (sync thread)"); l }
                    Err(e) => { eprintln!("[ipc] bind failed: {e}"); return; }
                };
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                            let mut buf = Vec::new();
                            if let Err(e) = stream.read_to_end(&mut buf) {
                                eprintln!("[ipc] read error: {e}");
                                continue;
                            }
                            match serde_json::from_slice::<IpcCommand>(&buf) {
                                Ok(cmd) => {
                                    eprintln!("[ipc] accepted: {:?}", cmd);
                                    if ipc_tx.send(cmd).is_err() {
                                        eprintln!("[ipc] channel closed");
                                        break;
                                    }
                                }
                                Err(e) => eprintln!("[ipc] deserialize error: {e}"),
                            }
                        }
                        Err(e) => eprintln!("[ipc] accept error: {e}"),
                    }
                }
            });
        }

        let notif_rx = if is_notif_surface {
            let (tx, rx) = std::sync::mpsc::channel();
            moon_beauty_shell::overlays::notifications::spawn(tx);
            moon_beauty_shell::bar::tray::watcher::spawn();
            Some(rx)
        } else {
            None
        };

        let notif_screen_at_startup = settings.notif_screen_name.clone();
        let app = Self {
            settings_ui: SettingsOverlay::new(&settings),
            settings,
            theme,
            telemetry: UnifiedTelemetry {
                cpu: Default::default(),
                gpu: Default::default(),
                gpu_selected_idx: 0,
                ram: Default::default(),
                net: Default::default(),
                battery: Default::default(),
                audio: Default::default(),
                mic: Default::default(),
                mpris: None,
                weather: None,
                podman: Default::default(),
                borg: Default::default(),
                bar_time: ClockBackend::get_bar_time(),
            },
            active_overlay: standalone_target,
            weather_tooltip_child: None,
            is_standalone,
            system_collector: SystemCollector::new(),
            standalone_child: None,
            ipc_rx,
            is_notif_surface,
            notif_rx,
            settings_json_mtime: std::fs::metadata(ShellSettings::config_path())
                .ok()
                .and_then(|m| m.modified().ok()),
            window_w: 1920.0,
            notif_screen_at_startup,
            notif_surface_size: if is_notif_surface { (1, 1) } else { (0, 0) },
            tray_expanded: false,
            tray_items: Vec::new(),
            tray_anim_width: _tray_width_px,
            tray_target_width: _tray_width_px,
            tray_anim_start_width: _tray_width_px,
            tray_anim_started_at: std::time::Instant::now(),
            tray_anim_duration_ms: 220,
            tray_timeout_gen: 0,
            tray_opened_at: std::time::Instant::now(),
            detected_gpus: if is_notif_surface || is_standalone { Vec::new() } else { SystemCollector::list_gpus_sync() },

            launcher: AppLauncher::new(),
            calc: {
                if let Ok(expr) = std::env::var("MBS_CALC_EXPR") {
                    CalculatorOverlay::new_with_input(&expr)
                } else {
                    CalculatorOverlay::new()
                }
            },
            clipboard: ClipboardOverlay::new(),
            notes: NotesOverlay::new(),
            pass: PassOverlay::new(),
            power: PowerOverlay::new(),
            todo: TodoOverlay::new(),
            reminders: RemindersOverlay::new(),
            dict: DictionaryOverlay::new(),
            unicode: UnicodeOverlay::new(),
            web: WebSearchOverlay::new(),
            amogus: AmogusOverlay::new(),
            annotator: {
                let mut a = AnnotatorOverlay::new();
                if let Ok(p) = std::env::var("MBS_ANNOTATOR_PATH") {
                    let pb = std::path::PathBuf::from(&p);
                    match std::fs::read(&pb) {
                        Ok(bytes) => {
                            a.load_image(pb, bytes);
                            eprintln!("[annotator] loaded capture from {p}");
                        }
                        Err(e) => eprintln!("[annotator] failed to read {p}: {e}"),
                    }
                }
                a
            },
            rng: RngOverlay::new(),
            email: EmailOverlay::new(),
            notifs: NotificationOverlay::new(),
            lockscreen: LockscreenOverlay::new(),
            weather_tick: false,
        
            last_cursor_x: 0.0,
            last_cursor_time: std::time::Instant::now() - std::time::Duration::from_secs(30),
            last_toast_count: 0,
            last_active_pending_written: None,
            last_lockscreen_check: std::time::Instant::now(),
        };

        let mut init_tasks = vec![
            Command::done(Message::FetchTrayItems),
            Command::done(Message::TickEmailSync),
            Command::perform(async {
                moon_beauty_shell::bar::weather::WeatherBackend::fetch().await.ok()
            }, Message::WeatherLoaded)
        ];

        if matches!(app.active_overlay, Some(OverlayTarget::Launcher)) {
            // Immediate focus attempt
            init_tasks.push(iced::widget::operation::focus("launcher_search").map(Message::Launcher));
            // Delayed re-focus — catches compositor focus arriving after map
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Launcher(LauncherMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Calc)) {
            init_tasks.push(
                iced::widget::operation::focus(moon_beauty_shell::overlays::calc::CALC_INPUT_ID)
                    .map(Message::Calc),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Calc(CalcMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Pass)) {
            init_tasks.push(
                iced::widget::operation::focus(moon_beauty_shell::overlays::pass::PASS_SEARCH_ID)
                    .map(Message::Pass),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Pass(PassMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Dictionary)) {
            init_tasks.push(
                iced::widget::operation::focus(moon_beauty_shell::overlays::dictionary::DICT_SEARCH_ID)
                    .map(Message::Dict),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Dict(DictMessage::Refocus),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Lockscreen)) {
            // No widget to focus — keyboard capture is via subscription
        } else if matches!(app.active_overlay, Some(OverlayTarget::Clipboard)) {
            init_tasks.push(Command::perform(
                ClipboardOverlay::reload(),
                |entries| Message::Clipboard(ClipboardMessage::Reloaded(entries)),
            ));
        } else if matches!(app.active_overlay, Some(OverlayTarget::Notes)) {
            init_tasks.push(
                iced::widget::operation::focus(moon_beauty_shell::overlays::notes::NOTES_INPUT_ID)
                    .map(Message::Notes),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Notes(NotesMessage::Refocus),
            ));
        }

        
        // Todo autofocus: same pattern as launcher/calc/pass — schedule
        // the input focus, then retry after 100ms in case the compositor
        // delivers window focus late (standalone layer surfaces sometimes
        // need this nudge).
        if matches!(app.active_overlay, Some(OverlayTarget::Todo)) {
            init_tasks.push(
                iced::widget::operation::focus(
                    moon_beauty_shell::overlays::todo::TODO_INPUT_ID,
                ).map(Message::Todo),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Todo(TodoMessage::Refocus),
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Unicode)) {
            init_tasks.push(
                iced::widget::operation::focus(moon_beauty_shell::overlays::unicode::UNICODE_INPUT_ID)
                    .map(Message::Unicode),
            );
            init_tasks.push(Command::perform(
                async {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                },
                |_| Message::Unicode(UnicodeMessage::Refocus),
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Email)) {
            init_tasks.push(Command::done(Message::Email(EmailMessage::SetLoading)));
            init_tasks.push(Command::perform(
                async { moon_beauty_shell::libraries::backend::email_client::fetch_envelopes("INBOX").await },
                |res| match res {
                        Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(envs)),
                        Err(e) => Message::Email(EmailMessage::Error(e)),
                    },
            ));
        }

        if matches!(app.active_overlay, Some(OverlayTarget::Reminders)) {
            init_tasks.push(
                iced::widget::operation::focus(moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID)
                    .map(Message::Reminders),
            );
            init_tasks.push(Command::perform(
                async { tokio::time::sleep(std::time::Duration::from_millis(100)).await; },
                |_| Message::Reminders(ReminderMessage::InitialFocus),
            ));
        }

        // History: on open, scroll to the auto-selected newest (bottom) row.
        // Immediate pass is usually enough; the delayed retry catches the
        // case where the scrollable hasn't been laid out yet when the
        // first scroll_to fires.
        if matches!(app.active_overlay, Some(OverlayTarget::NotificationsHistory)) {
            if let Some(y) = app.notifs.scroll_offset_for_selected(&app.theme, &app.settings) {
                init_tasks.push(iced::widget::operation::scroll_to(
                    iced::widget::Id::new("history_list"),
                    iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                ));
                init_tasks.push(Command::perform(
                    async move {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        y
                    },
                    |y| Message::Notif(NotifMessage::HistoryScrollTo(y)),
                ));
            }
        }

        (app, Command::batch(init_tasks))
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::WindowResized(w) => {
                self.window_w = w;
            }
            Message::ResizeNotifSurface(w, h) => {
                eprintln!("[resize] update caught ({}, {}) — try_into NOT called", w, h);
            }
            Message::PollNotifs => {
                let mtime = std::fs::metadata(ShellSettings::config_path())
                    .ok()
                    .and_then(|m| m.modified().ok());
                if mtime != self.settings_json_mtime {
                    self.settings_json_mtime = mtime;
                    self.settings = ShellSettings::load();
                    self.theme = ShellTheme::from_settings(&self.settings);
                }
                if !self.is_notif_surface
                    && !self.is_standalone
                    && self.settings.notif_screen_name != self.notif_screen_at_startup
                {
                    self.notif_screen_at_startup = self.settings.notif_screen_name.clone();
                    let _ = std::process::Command::new("pkill")
                        .args(["-f", "--", "moon-beauty-shell --notif-surface"])
                        .status();
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let exe = resolve_exe();
                    let _ = std::process::Command::new(exe).arg("--notif-surface").spawn();
                }
                if let Some(rx) = self.notif_rx.as_ref() {
                    while let Ok(n) = rx.try_recv() {
                        let is_test = n.app_name == "Diagnostics";
                        self.notifs.push_notification(
                            &n.app_name, &n.summary, &n.body, n.is_urgent, is_test,
                            n.desktop_entry.as_deref(),
                            n.actions.clone(),
                            &self.settings,
                        );
                    }
                }
                if self.is_notif_surface {
                    // CLI notification control (see --activate-latest-notif / --dismiss-notif)
                    let runtime = std::env::var("XDG_RUNTIME_DIR")
                        .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
                    let cmd_path = std::path::PathBuf::from(&runtime)
                        .join("moon-beauty-shell-notif-cmd");
                    if let Ok(cmd) = std::fs::read_to_string(&cmd_path) {
                        let _ = std::fs::remove_file(&cmd_path);
                        match cmd.trim() {
                            "dismiss" => {
                                // Target newest — after the view_toasts z-order
                                // fix, the newest toast is on top of z-order and
                                // is the one the user sees. Dismissing .first()
                                // (oldest) would hide a toast they weren't looking
                                // at, making the whole command feel like a no-op.
                                if let Some(last) = self.notifs.active_toasts.first().cloned() {
                                    self.notifs.update(NotifMessage::DismissToast(last.id));
                                }
                            }
                            "activate-latest" => {
                                if let Some(t) = self.notifs.active_toasts.first().cloned() {
                                    let app = t.app_name.clone();
                                    let entry = t.desktop_entry.clone();
                                    let nid = t.id;
                                    self.notifs.update(NotifMessage::DismissToast(t.id));
                                    std::thread::spawn(move || {
                                        moon_beauty_shell::overlays::notifications::emit_action_invoked(nid, "default");
                                        std::thread::sleep(std::time::Duration::from_millis(200));
                                        let matches = notification_candidates(&app, entry.as_deref());
                                        if !is_app_focused(&matches) {
                                            let ok = focus_notification_app(&app, entry.as_deref());
                                            eprintln!("[activate-latest] fallback app={:?} entry={:?} ok={}", app, entry, ok);
                                        } else {
                                            eprintln!("[activate-latest] client self-focused app={:?}", app);
                                        }
                                    });
                                } else {
                                    eprintln!("[activate-latest] no active toast");
                                }
                            }
                            _ => {}
                        }
                    }

                    if !moon_beauty_shell::overlays::notifications::dnd_active() {
                        let held = moon_beauty_shell::overlays::notifications::take_held_notifs();
                        if !held.is_empty() {
                            eprintln!("[dnd] unpaused! releasing {} held toast(s) to screen", held.len());
                            for mut item in held {
                                item.created_at = std::time::Instant::now();
                                item.hold_reset_at = Some(std::time::Instant::now());
                                item.exit_started_at = None;
                                item.slot_shift_start = None;
                                if self.notifs.active_toasts.len() < 4 {
                                    self.notifs.active_toasts.push(item);
                                } else {
                                    self.notifs.pending_toasts.push_back(item);
                                }
                            }
                        }
                    }
                    let big_w = (self.settings.notif_card_width as i32
                        + 2 * self.settings.notif_margin_x + 100)
                        .max(200) as u32;
                    let big_h = (3 * self.settings.notif_card_height as i32
                        + 2 * self.settings.notif_margin_x + 100)
                        .max(300) as u32;
                    let want = if self.notifs.active_toasts.is_empty() {
                        (1u32, 1u32)
                    } else {
                        (big_w, big_h)
                    };
                    let toast_count = self.notifs.active_toasts.len();
                    let count_changed = toast_count != self.last_toast_count;

                    // Publish active+pending to the bar. Runs on every tick,
                    // but only writes on change or once every 3s heartbeat.
                    let active_pending =
                        self.notifs.active_toasts.len() + self.notifs.pending_toasts.len();
                    let should_write = match self.last_active_pending_written {
                        None => true,
                        Some((prev, at)) => {
                            prev != active_pending || at.elapsed().as_secs() >= 3
                        }
                    };
                    if should_write {
                        moon_beauty_shell::overlays::notifications::write_active_count(active_pending);
                        self.last_active_pending_written =
                            Some((active_pending, std::time::Instant::now()));
                    }

                    if want != self.notif_surface_size || count_changed {
                        self.notif_surface_size = want;
                        self.last_toast_count = toast_count;
                        let rects = self.compute_toast_rects();
                        return Command::batch(vec![
                            Command::done(Message::ResizeNotifSurface(want.0, want.1)),
                            Command::done(Message::SetToastInputRegion(rects)),
                        ]);
                    }
                }
            }
            Message::TickClock => {
                self.telemetry.bar_time = ClockBackend::get_bar_time();
                // Only the primary bar process fires reminder notifications.
                // The standalone reminders overlay and the notification surface
                // also run TickClock every second; without this guard both see
                // alerted=false simultaneously and each fires a notify-send +
                // writes alerted=true, producing duplicate alerts.
                if !self.is_standalone && !self.is_notif_surface {
                    let now = chrono::Local::now().timestamp();
                    let mut entries = moon_beauty_shell::overlays::reminders::RemindersOverlay::load();
                    let mut changed = false;
                    for e in entries.iter_mut() {
                        if e.target_epoch <= now && !e.alerted {
                            e.alerted = true;
                            changed = true;
                            let text = if e.text.is_empty() { "Timer Finished!".into() } else { e.text.clone() };
                            let _ = std::process::Command::new("notify-send")
                                .args(["-a", "Alarm", "-u", "critical", "⏰ Timer Complete!", &text])
                                .spawn();

                            if e.play_sound {
                                if let Some(sound) = &self.settings.alarm_sound {
                                    play_alarm_sound(
                                        sound,
                                        self.settings.alarm_sound_volume,
                                        self.settings.alarm_sound_duration_sec,
                                    );
                                }
                            }
                        }
                    }
                    if changed {
                        moon_beauty_shell::overlays::reminders::save_reminders(&entries);
                    }
                }
            }
            Message::TickToasts => {
                self.notifs.tick_toasts(self.settings.notif_hold_duration_sec, self.settings.notif_volume);
            }
            Message::ComposeTabPressed(shift) => {
                if self.email.is_composing {
                    if !self.email.get_autocomplete_matches().is_empty() {
                        self.email.update(EmailMessage::ConfirmAutocomplete);
                        return Command::none();
                    }
                    // Iced 0.14 only has focus_next, so Shift+Tab will just go forward.
                    let _ = shift;
                    return iced::widget::operation::focus_next();
                }
            }
            Message::FocusComposeField => {
                self.email.active_compose_field = (self.email.active_compose_field + 1) % 3;
                let id = match self.email.active_compose_field {
                    0 => moon_beauty_shell::overlays::email::COMPOSE_TO_ID,
                    1 => moon_beauty_shell::overlays::email::COMPOSE_SUBJ_ID,
                    _ => moon_beauty_shell::overlays::email::COMPOSE_BODY_ID,
                };
                return iced::widget::operation::focus(id).map(Message::Email);
            }
            Message::TickEmailSync => {
                // Only the primary bar and the standalone email overlay
                // sync IMAP. The notif-surface doesn't need email data and
                // duplicating the sync doubles IMAP traffic + log noise.
                if self.is_notif_surface {
                    return Command::none();
                }
                if moon_beauty_shell::libraries::backend::email_client::config_exists() {
                    eprintln!("[sync] Boot/Timer triggered email sync");
                    let folder = "INBOX".to_string();
                    return Command::perform(
                        async move {
                            let old_envs = moon_beauty_shell::libraries::backend::email_client::load_index(&folder);
                            if let Ok(new_envs) = moon_beauty_shell::libraries::backend::email_client::fetch_from_network(&folder).await {
                                moon_beauty_shell::libraries::backend::email_client::save_index(&folder, &new_envs);
                                moon_beauty_shell::libraries::backend::email_client::sync_bodies_background(folder).await;
                                Ok::<_, String>((old_envs, new_envs))
                            } else {
                                Err("fetch failed".into())
                            }
                        },
                        |res| match res {
                            Ok((old, new)) => Message::EmailSyncedWithDiff(old, new),
                            Err(_) => Message::EmailSynced,
                        }
                    );
                }
            }
            Message::EmailSyncedWithDiff(old, new) => {
                eprintln!("[sync] EmailSyncedWithDiff: old={} new={}", old.len(), new.len());
                
                // Cross-process dedup file: prevent two processes from notifying the same email
                let dedup_path = dirs::runtime_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                    .join("mbs-email-notified.txt");
                let mut notified: std::collections::HashSet<String> = std::fs::read_to_string(&dedup_path)
                    .ok()
                    .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
                    .unwrap_or_default();
                
                let old_ids: std::collections::HashSet<String> = old.iter().map(|e| e.id.clone()).collect();
                for env in new {
                    if !old_ids.contains(&env.id) && !notified.contains(&env.id) {
                        eprintln!("[sync] New email from {}: {}", env.sender, env.subject);
                        
                        // Visual notification
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Email", &env.sender, &env.subject])
                            .spawn();
                        
                        // Spoken announcement
                        moon_beauty_shell::overlays::notifications::NotificationOverlay::speak_tts(
                            "Email",
                            &env.sender,
                            &env.subject,
                            Some("New email from {sender}: {body}"),
                            self.settings.tts_command.clone(),
                            self.settings.notif_volume,
                        );
                        
                        notified.insert(env.id.clone());
                    }
                }
                
                // Persist dedup list (cap at 200 most recent)
                let mut sorted: Vec<_> = notified.into_iter().collect();
                sorted.sort();
                sorted.truncate(200);
                let _ = std::fs::write(&dedup_path, sorted.join("\n"));
            }
            Message::EmailSynced => {}
            Message::TickWeather => {
                return Command::perform(async {
                    moon_beauty_shell::bar::weather::WeatherBackend::fetch().await.ok()
                }, Message::WeatherLoaded);
            }
            Message::WeatherLoaded(w) => {
                self.telemetry.weather = w;
            }
            Message::TickHardware => {
                self.telemetry.gpu_selected_idx = self.detected_gpus.iter().position(|g| g.card_id == self.settings.gpu_selected_id).unwrap_or(0);
                let bat = SystemCollector::collect_battery();
                self.telemetry.battery = bat;
                self.telemetry.net = self.system_collector.update_net();

                let gpu_id = self.settings.gpu_selected_id.clone();

                return Command::perform(
                    async move {
                        let cpu = SystemCollector::collect_cpu().await;
                        let ram = SystemCollector::collect_ram().await;
                        let gpu = SystemCollector::collect_gpu_by_card_id(&gpu_id).await;
                        let audio = AudioBackend::get_sink().await;
                        let mic = AudioBackend::get_source().await;
                        let mpris = MprisBackend::poll().await;
                        (cpu, ram, gpu, audio, mic, mpris)
                    },
                    |(cpu, ram, gpu, audio, mic, mpris)| {
                        Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris)
                    },
                );
            }
            Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris) => {
                self.telemetry.cpu = cpu;
                self.telemetry.ram = ram;
                self.telemetry.gpu = gpu;
                self.telemetry.audio = audio;
                self.telemetry.mic = mic;
                self.telemetry.mpris = mpris;
            }
            Message::IpcReceived(cmd) => match cmd {
                IpcCommand::Toggle(target) => {
                    // Every overlay runs as a standalone child so it gets
                    // keyboard=Exclusive from its layer surface. The bar's own
                    // layer is KeyboardInteractivity::None, so in-bar overlays
                    // cannot receive keystrokes.
                    eprintln!("[ipc] toggle {:?} (standalone)", target);

                    // SECURITY: the lockscreen must NEVER be unlockable via
                    // IPC. Before this guard, two rapid Toggle(Lockscreen)
                    // calls spawn-then-kill (200ms+ desktop-visible window),
                    // and any local script could dismiss the lock by sending
                    // two toggles. Lockscreen is now idempotent-spawn: IPC
                    // can spawn it, but only password auth removes it.
                    if matches!(target, OverlayTarget::Lockscreen) {
                        if let Some(OverlayTarget::Lockscreen) = self.standalone_running() {
                            eprintln!(
                                "[ipc] lockscreen already active; ignoring duplicate toggle"
                            );
                            return Command::none();
                        }
                        eprintln!("[ipc] lockscreen spawn (cannot be toggled off via IPC)");
                        self.spawn_standalone(OverlayTarget::Lockscreen);
                    } else {
                        self.toggle_standalone(target);
                    }
                }
                IpcCommand::Open(target, arg) => {
                    if matches!(target, OverlayTarget::Launcher | OverlayTarget::Clipboard | OverlayTarget::NotificationsHistory | OverlayTarget::Settings | OverlayTarget::Todo | OverlayTarget::Notes | OverlayTarget::Unicode) {
                        self.spawn_standalone(target);
                    } else if matches!(target, OverlayTarget::Lockscreen) {
                        // Lockscreen MUST be a standalone process so it can
                        // spawn the multi-monitor coordinator. Idempotent: if
                        // it's already active, ignore. Respawn = kill+spawn =
                        // unlocked window. Only auth success removes the lock.
                        if let Some(OverlayTarget::Lockscreen) = self.standalone_running() {
                            eprintln!(
                                "[ipc] lockscreen already active; ignoring duplicate open"
                            );
                            return Command::none();
                        }
                        self.spawn_standalone(target);
                    } else if matches!(target, OverlayTarget::Annotator) {
                        // Annotator needs fullscreen + exclusive keyboard, which only
                        // the standalone (--annotator-session) process gets via its
                        // LayerShellSettings. Spawn a child with the captured path.
                        let exe = resolve_exe();
                        let mut cmd = std::process::Command::new(exe);
                        cmd.arg("--annotator-session");
                        if let Some(ref p) = arg {
                            cmd.env("MBS_ANNOTATOR_PATH", p);
                        }
                        match cmd.spawn() {
                            Ok(child) => {
                                eprintln!("[annotator] spawned pid={}", child.id());
                                self.standalone_child = Some((OverlayTarget::Annotator, child));
                            }
                            Err(e) => eprintln!("[annotator] spawn failed: {e}"),
                        }
                    } else {
                        self.active_overlay = Some(target);
                    }
                }
                IpcCommand::Close(target) => {
                    if self.active_overlay == Some(target) {
                        self.active_overlay = None;
                    }
                }
                IpcCommand::CloseAll => {
                    self.active_overlay = None;
                }
                IpcCommand::DismissLatestNotif => {
                    // Bar's active_toasts is empty — the notif-surface
                    // process owns the actual toast stack. Route through the
                    // same file-based command channel that --dismiss-notif
                    // already uses. Without this routing, DismissLatestNotif
                    // was a guaranteed no-op (Vec::first() on an empty Vec).
                    let runtime = std::env::var("XDG_RUNTIME_DIR")
                        .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
                    let cmd_path = std::path::PathBuf::from(runtime)
                        .join("moon-beauty-shell-notif-cmd");
                    match std::fs::write(&cmd_path, b"dismiss") {
                        Ok(_) => eprintln!("[ipc] DismissLatestNotif -> notif-surface"),
                        Err(e) => eprintln!("[ipc] DismissLatestNotif write failed: {e}"),
                    }
                }
                IpcCommand::JumpToLatestNotif => {
                    self.spawn_standalone(OverlayTarget::NotificationsHistory);
                }
                IpcCommand::ToggleHistory => {
                    self.toggle_standalone(OverlayTarget::NotificationsHistory);
                }
                IpcCommand::ReloadSettings => {
                    self.settings = ShellSettings::load();
                    self.theme = ShellTheme::from_settings(&self.settings);
                }
                IpcCommand::TestNotification => {
                    for (title, body) in [
                        ("System Diagnostics #1 (first)", "Verification toast successfully routed."),
                        ("System Diagnostics #2 (second)", "Background worker service nominal."),
                        ("System Diagnostics #3 (third)", "Telemetry stream established."),
                    ] {
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Diagnostics", title, body])
                            .spawn();
                    }
                }
                IpcCommand::IncomingNotif { .. } => {}
                IpcCommand::Ping => {}
            },
            Message::PollIpc => {
                while let Ok(cmd) = self.ipc_rx.try_recv() {
                    let _ = self.update(Message::IpcReceived(cmd));
                }
            }
            Message::TopBar(msg) => {
                eprintln!("[topbar-msg] {:?}", msg);
                if let TopBarMessage::Action(act) = msg {
                    match act {
                        CapsuleAction::OpenOverlay(id) => {
                            eprintln!("[topbar-action] OpenOverlay: id = {:?}", id);

                            

                            // Recent click => use cursor position for dynamic placement.
                            // Fallback to capsule_center_x if no click in last 5s (e.g. IPC trigger).
                            let recent_click = self.last_cursor_time.elapsed() < std::time::Duration::from_secs(5);

                            // 1. ALARM — standalone reminders overlay.
                            // Handled BEFORE the tooltip-open/toggle block so:
                            //   (a) an open tooltip can't hijack the click,
                            //   (b) id="alarm" never enters the tooltip spawn
                            //       selection and falls through to --sys-tooltip
                            //       (which defaults unknown targets to RAM).
                            if id == "alarm" {
                                if let Some(mut c) = self.weather_tooltip_child.take() {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                }
                                self.toggle_standalone(OverlayTarget::Reminders);
                                return Command::none();
                            }

                            // 2. DROPDOWN TOOLTIPS (CPU, GPU, RAM, Tray)
                            if matches!(id.as_str(), "weather" | "cpu" | "gpu" | "ram" | "calendar" | "music" | "unified" | "net" | "custom" | "clock") {
                                // Toggle off if already open
                                let running = self.weather_tooltip_child.as_mut()
                                    .and_then(|c| c.try_wait().ok())
                                    .is_some_and(|status| status.is_none());
                                if running {
                                    eprintln!("[tooltip] Already open; toggling off");
                                    if let Some(mut c) = self.weather_tooltip_child.take() {
                                        let _ = c.kill();
                                        let _ = c.wait();
                                    }
                                    return Command::none();
                                }
                                self.weather_tooltip_child = None;

                                let target_output = if self.settings.notif_screen_name != "Auto"
                                    && self.settings.notif_screen_name != "Auto (Primary)"
                                    && !self.settings.notif_screen_name.is_empty() {
                                    Some(self.settings.notif_screen_name.as_str())
                                } else { None };
                                let sw = get_screen_width_for(target_output);
                                let _cx = if recent_click && self.last_cursor_x > 0.0 {
                                    self.last_cursor_x as i32
                                } else {
                                    capsule_center_x(&self.settings, sw, &id)
                                };
                                let target_output = if self.settings.notif_screen_name != "Auto"
                                    && self.settings.notif_screen_name != "Auto (Primary)"
                                    && !self.settings.notif_screen_name.is_empty() {
                                    Some(self.settings.notif_screen_name.as_str())
                                } else { None };
                                let sw = get_screen_width_for(target_output);
                                let cx = if id == "clock" {
                                    capsule_center_x(&self.settings, sw, &id)
                                } else if recent_click && self.last_cursor_x > 0.0 {
                                    self.last_cursor_x as i32
                                } else {
                                    capsule_center_x(&self.settings, sw, &id)
                                };
                                let in_left = self.settings.bar_left.contains(&id);
                                let in_center = self.settings.bar_center.contains(&id);
                                let in_right = self.settings.bar_right.contains(&id);
                                let (slant_l, slant_r): (&str, &str) = match self.settings.slant_mode.as_str() {
                                    "left" => ("L", "L"),
                                    "right" => ("R", "R"),
                                    _ => {
                                        if in_center {
                                            let vis: Vec<&String> = self.settings.bar_center.iter()
                                                .filter(|x| self.settings.visible_capsules.get(*x).copied().unwrap_or(true)).collect();
                                            let k = vis.len() / 2;
                                            match vis.iter().position(|x| **x == id) {
                                                Some(i) if i == k => ("L", "R"),
                                                Some(i) if i < k => ("L", "L"),
                                                Some(_) => ("R", "R"),
                                                None => ("L", "R"),
                                            }
                                        } else if in_right {
                                            ("R", "R")
                                        } else {
                                            ("L", "L")
                                        }
                                    }
                                };
                                let cap_w = capsule_actual_width(&id);
                                let grow_right = if in_left { true } else if in_right { false } else { cx < (sw as i32) / 2 };
                                let (tip_w, _tip_h) = self.settings.tooltip_size(&id);
                                let tip_output = if self.settings.bar_monitor != "All"
                                    && !self.settings.bar_monitor.is_empty()
                                {
                                    Some(self.settings.bar_monitor.clone())
                                } else {
                                    std::env::var("MBS_OUTPUT").ok().filter(|s| !s.is_empty())
                                        .or_else(resolve_primary_output)
                                };

                                let req = moon_beauty_shell::bar::tooltip_spawn::SpawnRequest {
                                    id: &id,
                                    exe: resolve_exe(),
                                    output: tip_output.as_deref(),
                                    tooltip_w: tip_w,
                                    tooltip_h: moon_beauty_shell::bar::tooltip_spawn::compute_height(
                                        &id, &self.settings, &self.telemetry),
                                    drop_ms: self.settings.tooltip_drop_ms,
                                    expand_ms: self.settings.tooltip_expand_ms,
                                    capsule_cx: cx,
                                    capsule_w: cap_w,
                                    screen_w: sw,
                                    slant_l,
                                    slant_r,
                                    grow_right,
                                };

                                match moon_beauty_shell::bar::tooltip_spawn::spawn(&req) {
                                    Ok(c) => {
                                        eprintln!("[ipc] spawned tooltip pid={}", c.id());
                                        self.weather_tooltip_child = Some(c);
                                    }
                                    Err(e) => eprintln!("[tooltip] Failed to spawn: {e}"),
                                }
                                return Command::none();
                            }

                            // 2. FULLSCREEN / STANDALONE OVERLAYS (Settings, History, Power)
                            let mapped: Option<OverlayTarget> = match id.as_str() {
                                "notify" => Some(OverlayTarget::NotificationsHistory),
                                "battery" => Some(OverlayTarget::Power),
                                _ => OverlayTarget::from_str(&id),
                            };
                            if let Some(target) = mapped {
                                if matches!(
                                    target,
                                    OverlayTarget::Launcher
                                        | OverlayTarget::Clipboard
                                        | OverlayTarget::NotificationsHistory
                                        | OverlayTarget::Settings
                                ) {
                                    self.spawn_standalone(target);
                                } else {
                                    self.active_overlay = Some(target);
                                }
                            }
                        }
                        CapsuleAction::TogglePlayPause => {
                            return Command::perform(MprisBackend::toggle_play_pause(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::NextTrack => {
                            return Command::perform(MprisBackend::next(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::PreviousTrack => {
                            return Command::perform(MprisBackend::previous(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::CycleAudioSink => {
                            return Command::perform(AudioBackend::cycle_sinks(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::ToggleSinkMute => {
                            return Command::perform(AudioBackend::toggle_sink_mute(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::ToggleNotificationDnd => {
                            let paused = moon_beauty_shell::overlays::notifications::toggle_dnd();
                            self.settings.notifications_enabled = !paused;
                            let _ = self.settings.save();
                            eprintln!("[dnd] paused={}", paused);

                            // The notif-surface owns org.freedesktop.Notifications
                            // and writes held-notifs.json. It must stay alive
                            // while paused to receive + hold. Only respawn if
                            // it died for some unrelated reason.
                            let running = std::process::Command::new("pgrep")
                                .args(["-f", "--", "moon-beauty-shell --notif-surface"])
                                .output()
                                .map(|o| !o.stdout.is_empty())
                                .unwrap_or(false);
                            if !running {
                                let exe = resolve_exe();
                                let _ = std::process::Command::new(exe).arg("--notif-surface").spawn();
                                eprintln!("[dnd] respawned notif-surface");
                            }
                        }
                        CapsuleAction::CancelTooltip(id) => {
                            eprintln!("[topbar] CancelTooltip: {}", id);
                            if let Some(mut c) = self.weather_tooltip_child.take() {
                                let _ = c.kill();
                                let _ = c.wait();
                            }
                            // Right-click on alarm capsule: wipe active timers
                            if id == "alarm" {
                                moon_beauty_shell::overlays::reminders::save_reminders(&[]);
                                eprintln!("[topbar] cleared active timers");
                            }
                        }
                        CapsuleAction::ToggleSourceMute => {
                            return Command::perform(AudioBackend::toggle_source_mute(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::ToggleTrayExpanded => {
                            if !self.tray_expanded {
                                // OPENING
                                self.tray_timeout_gen = self.tray_timeout_gen.wrapping_add(1);
                                let _my_gen = self.tray_timeout_gen;
                                
                                self.tray_opened_at = std::time::Instant::now();
                                self.tray_expanded = true;
                                self.tray_anim_start_width = self.tray_anim_width;
                                self.tray_anim_started_at = std::time::Instant::now();
                                let expanded_w = moon_beauty_shell::bar::capsules::compute_expanded_visible_width(
                                    &self.settings, self.tray_items.len()
                                );
                                self.tray_target_width = expanded_w;
                                self.tray_anim_duration_ms = 220;

                                // Auto-close is handled by the tray_watchdog_sub
                                // subscription (see subscription()), which polls
                                // tray_opened_at.elapsed() every 500ms while
                                // tray_expanded is true. No per-open timer needed.
                                return Command::perform(
                                    moon_beauty_shell::bar::tray::watcher::fetch_tray_items(),
                                    Message::TrayItemsLoaded,
                                );
                                return Command::perform(
                                    moon_beauty_shell::bar::tray::watcher::fetch_tray_items(),
                                    Message::TrayItemsLoaded,
                                );
                            } else {
                                // CLOSING — bump gen to invalidate any pending timer
                                self.close_tray();
                            }
                        }
                        CapsuleAction::TrayActivate(item) => {
                            self.close_tray();
                            return Command::perform(
                                async move {
                                    let _ = moon_beauty_shell::bar::tray::watcher::activate_tray_item(&item.bus, &item.path).await;
                                },
                                |_| Message::TopBar(TopBarMessage::Noop)
                            );
                        }
                        CapsuleAction::TrayContextMenu(item) => {
                            let item_clone = item.clone();
                            return Command::perform(async move {
                                if let Some((menu_path, items)) = moon_beauty_shell::bar::tray::watcher::get_menu_tree(&item_clone.bus, &item_clone.path).await {
                                    let json = serde_json::to_string(&items).unwrap();
                                    let tmp_path = format!("/tmp/mbs-tray-menu-{}.json", std::process::id());
                                    std::fs::write(&tmp_path, json).unwrap();
                                    let exe = resolve_exe();
                                    let h = (items.len() as u32 * 36).clamp(100, 600) + 40;
                                    let _ = std::process::Command::new(exe)
                                        .args(["--tray-context-menu", &tmp_path, &item_clone.bus, &menu_path, "250", &h.to_string()])
                                        .spawn();
                                }
                            }, |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::CycleGpuTarget => {
                            let gpus = &self.detected_gpus;
                            if !gpus.is_empty() {
                                let current = &self.settings.gpu_selected_id;
                                let next_idx = gpus
                                    .iter()
                                    .position(|g| &g.card_id == current)
                                    .map(|i| (i + 1) % gpus.len())
                                    .unwrap_or(0);
                                self.settings.gpu_selected_id = gpus[next_idx].card_id.clone();
                                let _ = self.settings.save();
                                eprintln!("[bar] right-click GPU: switched to {}", self.settings.gpu_selected_id);
                                return Command::done(Message::TickHardware);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Message::Launcher(msg) => match msg {
                LauncherMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                LauncherMessage::ExecuteApp(cmd) => {
                    // Tab pressed on math row — open calculator with same expression
                    if cmd == "__tab__" {
                        // Tab = Enter on currently selected item.
                        // For a math row, this copies the result.
                        // For an app row, this launches the app.
                        if let Some(selected) = self.launcher.filtered_apps.get(self.launcher.selected_index) {
                            let resolved = selected.exec.clone();
                            return self.update(Message::Launcher(LauncherMessage::ExecuteApp(resolved)));
                        }
                        return Command::none();
                    }
                    // Math result — copy to clipboard and close
                    if let Some(result) = cmd.strip_prefix("math:") {
                        let result = result.to_string();
                        eprintln!("[launcher] math result -> wl-copy: {}", result);
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Launcher", "Copied", &result])
                            .spawn();
                        if self.is_standalone {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            let r = result.clone();
                            rt.block_on(async move {
                                let _ = tokio::process::Command::new("wl-copy")
                                    .arg(&r)
                                    .status()
                                    .await;
                            });
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                        return Command::perform(async move {
                            let _ = tokio::process::Command::new("wl-copy")
                                .arg(&result)
                                .status()
                                .await;
                        }, |_| Message::DismissActiveOverlay);
                    }

                    if let Some(definition) = cmd.strip_prefix("dict:") {
                        // Ignore sentinel entries (pending/hint)
                        if definition == "__pending__" || definition == "__hint__" {
                            return Command::none();
                        }
                        let def = definition.to_string();
                        eprintln!("[launcher] dict copy: {}...", def.chars().take(40).collect::<String>());
                        let _ = std::process::Command::new("wl-copy").arg(&def).spawn();
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Dictionary", "Copied", &def])
                            .spawn();
                        if self.is_standalone {
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                        return Command::none();
                    }

                    if let Some(key) = cmd.strip_prefix("pass:") {
                        eprintln!("[launcher] pass copy: {}", key);
                        // Detached: `pass -c` may block on GPG pinentry, so we
                        // spawn it as its own process and exit immediately.
                        // The `sh -c '... "$1"' -- <key>` pattern prevents shell
                        // injection when the key has special chars.
                        let _ = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(r#"pass -c "$1" 2>&1 && notify-send -a 'Password Store' 'Copied' "$1""#)
                            .arg("--")
                            .arg(key)
                            .spawn();
                        if self.is_standalone {
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                        return Command::none();
                    }

                    if let Some(target_str) = cmd.strip_prefix("internal:") {
                        if let Some(target) = OverlayTarget::from_str(target_str) {
                            if self.is_standalone {
                                self.spawn_standalone(target);
                                std::process::exit(0);
                            } else {
                                self.active_overlay = Some(target);
                                return Command::none();
                            }
                        }
                    }
                    if self.is_standalone {
                        tokio::spawn(async move { AppLauncher::launch(&cmd).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { AppLauncher::launch(&cmd).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => return self.launcher.update(msg).map(Message::Launcher),
            },
            Message::Calc(msg) => match msg {
                CalcMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                CalcMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::calc::CALC_INPUT_ID,
                    ).map(Message::Calc);
                }
                CalcMessage::CopyResult(val) => {
                    if self.is_standalone {
                        // Block on the copy so it completes before exit.
                        let v = val.clone();
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            CalculatorOverlay::copy_to_clipboard(&v).await;
                        });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { CalculatorOverlay::copy_to_clipboard(&val).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => self.calc.update(msg),
            },
            Message::Clipboard(msg) => match msg {
                ClipboardMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                ClipboardMessage::CopyEntry(id) => {
                    if self.is_standalone {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move { ClipboardOverlay::copy(id).await; });
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { ClipboardOverlay::copy(id).await; }, |_| Message::DismissActiveOverlay);
                }
                ClipboardMessage::DeleteEntry(id) => {
                    return Command::perform(async move { ClipboardOverlay::delete(id).await; }, |_| Message::DismissActiveOverlay);
                }
                ClipboardMessage::WipeAll => {
                    return Command::perform(async move { ClipboardOverlay::wipe().await; }, |_| Message::DismissActiveOverlay);
                }
                ClipboardMessage::SearchChanged(q) => {
                    self.clipboard.search_query = q;
                }
                ClipboardMessage::Reloaded(entries) => {
                    self.clipboard.entries = entries;
                }
                ClipboardMessage::SelectNext => {
                let ids = self.clipboard.filtered_ids();
                if ids.is_empty() { return Command::none(); }
                let next_idx = self.clipboard.selected_id.and_then(|sel| ids.iter().position(|&id| id == sel)).map(|p| (p + 1) % ids.len()).unwrap_or(0);
                let next_id = ids[next_idx];
                eprintln!("[debug-nav] Down pressed. next_idx: {}, next_id: {}", next_idx, next_id);
                self.clipboard.selected_id = Some(next_id);
                return Command::batch([
                    Command::perform(ClipboardOverlay::load_preview(next_id), |(text, image_path)| Message::Clipboard(ClipboardMessage::PreviewLoaded { text, image_path })),
                    iced::widget::operation::scroll_to(
                        iced::widget::Id::new("clip_list"),
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: (next_idx as f32 * 64.0 - 200.0).max(0.0) }
                    )
                ]);
            }
            ClipboardMessage::SelectPrev => {
                let ids = self.clipboard.filtered_ids();
                if ids.is_empty() { return Command::none(); }
                let prev_idx = self.clipboard.selected_id.and_then(|sel| ids.iter().position(|&id| id == sel)).map(|p| (p + ids.len() - 1) % ids.len()).unwrap_or(0);
                let prev_id = ids[prev_idx];
                eprintln!("[debug-nav] Up pressed. prev_idx: {}, prev_id: {}", prev_idx, prev_id);
                self.clipboard.selected_id = Some(prev_id);
                return Command::batch([
                    Command::perform(ClipboardOverlay::load_preview(prev_id), |(text, image_path)| Message::Clipboard(ClipboardMessage::PreviewLoaded { text, image_path })),
                    iced::widget::operation::scroll_to(
                        iced::widget::Id::new("clip_list"),
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: (prev_idx as f32 * 64.0 - 200.0).max(0.0) }
                    )
                ]);
            }
                        ClipboardMessage::DeleteSelected => {
                eprintln!("[clip] Delete key pressed. Selected ID: {:?}", self.clipboard.selected_id);
                if let Some(id) = self.clipboard.selected_id {
                    let pos = self.clipboard.entries.iter().position(|e| e.id == id);

                    // Matches Lua logic: pipe ID + tab into cliphist delete
                    if let Ok(mut child) = std::process::Command::new("cliphist")
                        .arg("delete")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                    {
                        use std::io::Write;
                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = writeln!(stdin, "{}\t", id);
                        }
                        let _ = child.wait();
                        eprintln!("[clip] Deleted ID {}", id);
                    }

                    if let Some(idx) = pos {
                        self.clipboard.entries.remove(idx);
                        if !self.clipboard.entries.is_empty() {
                            let next_idx = if idx >= self.clipboard.entries.len() { self.clipboard.entries.len() - 1 } else { idx };
                            let next_id = self.clipboard.entries[next_idx].id;
                            self.clipboard.selected_id = Some(next_id);
                            return Command::batch([
                                Command::perform(
                                    ClipboardOverlay::load_preview(next_id),
                                    |(text, image_path)| Message::Clipboard(ClipboardMessage::PreviewLoaded { text, image_path })
                                ),
                                iced::widget::operation::scroll_to(
                                    iced::widget::Id::new("clip_list"),
                                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: (next_idx as f32 * 64.0 - 200.0).max(0.0) }
                                )
                            ]);
                        } else {
                            self.clipboard.selected_id = None;
                            self.clipboard.preview_text.clear();
                            self.clipboard.preview_image_path = None;
                        }
                    }
                }
                return Command::none();
            }
            ClipboardMessage::RunOcr(id) => {
                eprintln!("[ocr] RunOcr triggered for id {}", id);
                let is_img = self.clipboard.entries.iter().find(|e| e.id == id).map(|e| e.is_image).unwrap_or(false);
                eprintln!("[ocr] entry found, is_image={}", is_img);
                let tmp_path = std::env::temp_dir().join(format!("mbs-ocr-{}.png", id));
                
                // Do OCR synchronously — it's fast (~1s) and we're about to close anyway
                if let Ok(out) = std::process::Command::new("cliphist")
                    .args(["decode", &id.to_string()])
                    .output()
                {
                    if !out.stdout.is_empty() && std::fs::write(&tmp_path, &out.stdout).is_ok() {
                        // Clear LD_LIBRARY_PATH so tesseract uses its own
                        // nix-store glibc (2.43) instead of the system glibc (2.42).
                        // This prevents the 5.5.2-binary / 5.5.3-library mismatch.
                        let tess_out = std::process::Command::new("tesseract")
                            .args([tmp_path.to_str().unwrap(), "stdout"])
                            .env_remove("LD_LIBRARY_PATH")
                            .output();

                        if let Ok(tess_out) = tess_out {
                            let text = String::from_utf8_lossy(&tess_out.stdout).trim().to_string();
                            if !text.is_empty() {
                                eprintln!("[ocr] {} chars extracted", text.len());
                                if let Ok(mut child) = std::process::Command::new("wl-copy")
                                    .stdin(std::process::Stdio::piped())
                                    .spawn()
                                {
                                    use std::io::Write;
                                    if let Some(mut stdin) = child.stdin.take() {
                                        let _ = stdin.write_all(text.as_bytes());
                                    }
                                    let _ = child.wait();
                                }
                            } else {
                                eprintln!("[ocr] no text found — stderr: {}", String::from_utf8_lossy(&tess_out.stderr));
                            }
                        }
                    }
                }
                let _ = std::fs::remove_file(&tmp_path);
                
                if self.is_standalone { std::process::exit(0); }
                self.active_overlay = None;
                return Command::none();
            }
            ClipboardMessage::CopySelected => {
                if let Some(id) = self.clipboard.selected_id {
                    if let Ok(out) = std::process::Command::new("cliphist").args(["decode", &id.to_string()]).output() {
                        if let Ok(mut child) = std::process::Command::new("wl-copy").stdin(std::process::Stdio::piped()).spawn() {
                            use std::io::Write;
                            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(&out.stdout); }
                        }
                    }
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
            }
                        ClipboardMessage::SelectEntry(id) => {
                    self.clipboard.selected_id = Some(id);
                    return Command::perform(
                        ClipboardOverlay::load_preview(id),
                        |(text, image_path)| Message::Clipboard(ClipboardMessage::PreviewLoaded { text, image_path }),
                    );
                }
                ClipboardMessage::PreviewLoaded { text, image_path } => {
                    self.clipboard.preview_text = text;
                    self.clipboard.preview_image_path = image_path;
                }
            },
            Message::Notes(msg) => match msg {
                NotesMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                NotesMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::notes::NOTES_INPUT_ID,
                    ).map(Message::Notes);
                }
                _ => self.notes.update(msg),
            },
            Message::Pass(msg) => match msg {
                PassMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                PassMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::pass::PASS_SEARCH_ID,
                    ).map(Message::Pass);
                }
                PassMessage::CopyKey(k) => {
                    if self.is_standalone {
                        // Block on copy so it completes before exit (no orphan tokio task)
                        let k = k.clone();
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            PassOverlay::copy_password(&k).await;
                        });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { PassOverlay::copy_password(&k).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => self.pass.update(msg),
            },
            Message::Power(msg) => match msg {
                PowerMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => {
                    self.power.update(msg);
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
            },
            Message::Todo(msg) => match msg {
                TodoMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                TodoMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::todo::TODO_INPUT_ID,
                    ).map(Message::Todo);
                }
                TodoMessage::EditSelected | TodoMessage::EditTask(_) => {
                    self.todo.update(msg);
                    // Refocus the input so the user can type immediately
                    // after clicking ✎ or pressing F2.
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::todo::TODO_INPUT_ID,
                    ).map(Message::Todo);
                }
                TodoMessage::TaskSelectPrev | TodoMessage::TaskSelectNext => {
                    self.todo.update(msg);
                    if let Some(sel_id) = self.todo.selected_task_id {
                        // Scroll to selected task using row height of ~40px.
                        let visible: Vec<u64> = self.todo.db.todos.iter()
                            .filter(|t| t.category == self.todo.active_category)
                            .filter(|t| match self.todo.filter {
                                moon_beauty_shell::overlays::todo::TodoFilter::All => true,
                                moon_beauty_shell::overlays::todo::TodoFilter::Active => !t.completed,
                                moon_beauty_shell::overlays::todo::TodoFilter::Completed => t.completed,
                            })
                            .map(|t| t.id)
                            .collect();
                        let idx = visible.iter().position(|v| *v == sel_id).unwrap_or(0);
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("todo_list"),
                            iced::widget::operation::AbsoluteOffset {
                                x: 0.0,
                                y: (idx as f32 * 40.0 - 100.0).max(0.0),
                            },
                        );
                    }
                }
                _ => self.todo.update(msg),
            },
            Message::Reminders(msg) => match msg {
                ReminderMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                ReminderMessage::InitialFocus => {
                    // 100ms delayed retry — start on the task input
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID,
                    ).map(Message::Reminders);
                }
                ReminderMessage::Refocus => {
                    // Enter in task input → jump to when input
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::reminders::REMINDER_WHEN_ID,
                    ).map(Message::Reminders);
                }
                ReminderMessage::FocusNextField => {
                    // Tab: cycle Task → When → Task based on current field
                    self.reminders.cycle_focus_forward();
                    let target = if self.reminders.active_field == 0 {
                        moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID
                    } else {
                        moon_beauty_shell::overlays::reminders::REMINDER_WHEN_ID
                    };
                    return iced::widget::operation::focus(target).map(Message::Reminders);
                }
                ReminderMessage::FocusPrevField => {
                    self.reminders.cycle_focus_backward();
                    let target = if self.reminders.active_field == 0 {
                        moon_beauty_shell::overlays::reminders::REMINDER_TASK_ID
                    } else {
                        moon_beauty_shell::overlays::reminders::REMINDER_WHEN_ID
                    };
                    return iced::widget::operation::focus(target).map(Message::Reminders);
                }
                ReminderMessage::ScrollUp
                | ReminderMessage::ScrollDown
                | ReminderMessage::SelectFirst
                | ReminderMessage::SelectLast
                | ReminderMessage::DeleteSelected => {
                    self.reminders.update(msg);
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("reminders_list"),
                        iced::widget::operation::AbsoluteOffset { x: 0.0, y: self.reminders.scroll_y },
                    );
                }
                _ => self.reminders.update(msg),
            },
            Message::Dict(msg) => match msg {
                DictMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                DictMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::dictionary::DICT_SEARCH_ID,
                    ).map(Message::Dict);
                }
                DictMessage::SearchWord => {
                    let w = self.dict.word_query.clone();
                    // Check cache first
                    if let Some(cached) = self.dict.cache.get(&w.to_lowercase()).cloned() {
                        self.dict.update(DictMessage::DefinitionsLoaded(w, cached));
                        return Command::none();
                    }
                    return Command::perform(async move { (w.clone(), DictionaryOverlay::lookup_word(&w).await) }, |(w, defs)| {
                        Message::Dict(DictMessage::DefinitionsLoaded(w, defs))
                    });
                }
                DictMessage::CopyDefinition(def) => {
                    if self.is_standalone {
                        let def = def.clone();
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move {
                            let _ = tokio::process::Command::new("wl-copy").arg(&def).status().await;
                        });
                        std::process::exit(0);
                    }
                    return Command::perform(async move {
                        let _ = tokio::process::Command::new("wl-copy").arg(&def).status().await;
                    }, |_| Message::DismissActiveOverlay);
                }
                _ => self.dict.update(msg),
            },
            Message::Unicode(msg) => match msg {
                UnicodeMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                UnicodeMessage::Refocus => {
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::unicode::UNICODE_INPUT_ID,
                    ).map(Message::Unicode);
                }
                UnicodeMessage::CopySymbol(sym) => {
                    if self.is_standalone {
                        tokio::spawn(async move { UnicodeOverlay::copy(sym).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move { UnicodeOverlay::copy(sym).await; },
                        |_| Message::DismissActiveOverlay,
                    );
                }
                UnicodeMessage::SelectNext
                | UnicodeMessage::SelectPrev
                | UnicodeMessage::SelectNextRow
                | UnicodeMessage::SelectPrevRow => {
                    self.unicode.update(msg);
                    // Auto-scroll to selected item. Grid is 5 columns,
                    // item height = 66 + 8 gap = 74. Scroll so selected
                    // is roughly centered vertically.
                    if let Some(idx) = self.unicode.selected_idx {
                        let row = idx / 5;
                        let y = (row as f32) * 74.0 - 200.0;
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("unicode_grid"),
                            iced::widget::operation::AbsoluteOffset { x: 0.0, y: y.max(0.0) },
                        );
                    }
                }
                UnicodeMessage::CopySelected => {
                    self.unicode.update(msg);
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.unicode.update(msg),
            },
            Message::Web(msg) => match msg {
                WebSearchMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                WebSearchMessage::ExecuteSearch => {
                    if let Some(url) = WebSearchOverlay::build_url(&self.web.query) {
                        if self.is_standalone {
                            tokio::spawn(async move { WebSearchOverlay::launch_browser(&url).await; });
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                        return Command::perform(async move { WebSearchOverlay::launch_browser(&url).await; }, |_| Message::DismissActiveOverlay);
                    }
                }
                _ => self.web.update(msg),
            },
            Message::Annotator(msg) => {
                let is_text_press = matches!(msg, AnnotatorMessage::Pressed(_)) && self.annotator.tool == Tool::Text;
                match msg {
                AnnotatorMessage::ToggleReveal => {
                    self.annotator.reveal_mode = !self.annotator.reveal_mode;
                    if !self.annotator.reveal_mode {
                        self.annotator.status = "Draw · Enter saves & closes · ESC cancels".into();
                    } else if self.annotator.reveal_bytes.is_some() {
                        self.annotator.status = "Reveal: watermark amplified".into();
                    } else if let Some(source) = self.annotator.source_path.clone() {
                        match std::fs::read(&source)
                            .map_err(|e| e.to_string())
                            .and_then(|b| plate_watermark::reveal(&b))
                        {
                            Ok(bytes) => {
                                self.annotator.reveal_handle = Some(iced::widget::image::Handle::from_bytes(bytes.clone()));
                                self.annotator.reveal_bytes = Some(bytes);
                                self.annotator.status = "Reveal: watermark amplified".into();
                            }
                            Err(e) => {
                                self.annotator.status = format!("Reveal failed: {e}");
                                self.annotator.reveal_mode = false;
                            }
                        }
                    } else {
                        self.annotator.status = "Reveal: no source image".into();
                        self.annotator.reveal_mode = false;
                    }
                }
                AnnotatorMessage::Save => {
                if self.annotator.reveal_mode {
                    if let Some(bytes) = self.annotator.reveal_bytes.clone() {
                        let save_dir = dirs::picture_dir()
                            .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
                            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                            .join("Screenshots");
                        let _ = std::fs::create_dir_all(&save_dir);
                        let time_str = chrono::Local::now().format("%Y-%m-%d %H-%M").to_string();
                        let out = save_dir.join(format!("Revealed {}.png", time_str));
                        
                        match std::fs::write(&out, &bytes) {
                            Ok(_) => {
                                eprintln!("[annotator] saved revealed proof -> {:?}", out);

                                // Copy the revealed image to clipboard and map its name
                                if let Ok(mut child) = std::process::Command::new("wl-copy")
                                    .args(["-t", "image/png"])
                                    .stdin(std::process::Stdio::piped())
                                    .spawn()
                                {
                                    use std::io::Write;
                                    if let Some(mut stdin) = child.stdin.take() {
                                        let _ = stdin.write_all(&bytes);
                                    }
                                    let _ = child.wait();

                                    let cache_dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
                                    let map_path = cache_dir.join("mbs-clip-names.txt");
                                    let filename = out.file_name().unwrap_or_default().to_string_lossy().to_string();

                                    let mut existing_clip_ids = std::collections::HashSet::new();
                                    if let Ok(list_out) = std::process::Command::new("cliphist").args(["list"]).output() {
                                        for line in String::from_utf8_lossy(&list_out.stdout).lines() {
                                            if let Some((id_str, _)) = line.split_once('\t') {
                                                if let Ok(id) = id_str.parse::<u64>() { existing_clip_ids.insert(id); }
                                            }
                                        }
                                    }

                                    for _ in 0..20 {
                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                        if let Ok(list_out) = std::process::Command::new("cliphist").args(["list"]).output() {
                                            let text = String::from_utf8_lossy(&list_out.stdout);
                                            for line in text.lines() {
                                                if let Some((id_str, _)) = line.split_once('\t') {
                                                    if let Ok(id) = id_str.trim().parse::<u64>() {
                                                        if !existing_clip_ids.contains(&id) {
                                                            let entry = format!("{}\t{}\n", id, filename);
                                                            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&map_path) {
                                                                let _ = f.write_all(entry.as_bytes());
                                                            }
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if self.is_standalone { std::process::exit(0); }
                                self.active_overlay = None;
                            }
                            Err(e) => self.annotator.status = format!("Reveal save failed: {e}"),
                        }
                    } else {
                        self.annotator.status = "Reveal not ready".into();
                    }
                } else {
                    let source = self.annotator.source_path.clone();
                    let annotations = self.annotator.annotations.clone();
                    let watermark = if self.annotator.watermark_input.is_empty() {
                        None
                    } else {
                        Some(self.annotator.watermark_input.clone())
                    };
                    match source {
                        Some(p) => match save_annotated_image(p, annotations, watermark) {
                            Ok(paths) => {
                                eprintln!("[annotator] saved {} file(s)", paths.len());
                                if self.is_standalone { std::process::exit(0); }
                                self.active_overlay = None;
                            }
                            Err(e) => self.annotator.status = format!("Save failed: {e}"),
                        },
                        None => self.annotator.status = "Save failed: no source image".into(),
                    }
                }
                
            }
                AnnotatorMessage::Cancel => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                AnnotatorMessage::Saved(_) | AnnotatorMessage::RevealLoaded(_) => {}
                other => self.annotator.update(other),
                }
                if is_text_press {
                    return Command::none();
                }
            }
            Message::Amogus(msg) => match msg {
                AmogusMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                AmogusMessage::TriggerScan => {
                    self.amogus.update(AmogusMessage::TriggerScan);
                    return Command::perform(AmogusOverlay::run_scan(), |cols| Message::Amogus(AmogusMessage::ScanFinished(cols)));
                }
                _ => self.amogus.update(msg),
            },
            Message::Rng(msg) => match msg {
                RngMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.rng.update(msg),
            },
            Message::Email(msg) => match msg {
                EmailMessage::Dismiss => {
                    if self.email.is_composing {
                        let to = self.email.compose_to.clone();
                        let sub = self.email.compose_subject.clone();
                        let body = self.email.compose_body.text();
                        self.email.update(EmailMessage::CancelCompose);
                        return Command::perform(
                            async move { moon_beauty_shell::libraries::backend::email_client::save_draft(&to, &sub, &body).await },
                            |_| Message::Email(EmailMessage::DraftSaved)
                        );
                    } else {
                        if self.is_standalone { std::process::exit(0); }
                        self.active_overlay = None;
                    }
                }
                EmailMessage::SelectFolder(f) => {
                    self.email.update(EmailMessage::SelectFolder(f.clone()));
                    return Command::batch(vec![
                        Command::done(Message::Email(EmailMessage::SetLoading)),
                        Command::perform(
                            async move { moon_beauty_shell::libraries::backend::email_client::fetch_envelopes(&f).await },
                            |res| match res {
                                Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(envs)),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            }
                        )
                    ]);
                }
                EmailMessage::SelectMail(id) => {
                    let folder = self.email.active_folder.clone();
                    self.email.update(EmailMessage::SelectMail(id.clone()));
                    return Command::perform(
                        async move { moon_beauty_shell::libraries::backend::email_client::read_message(&id, &folder).await },
                        |body| Message::Email(EmailMessage::MessageLoaded(body)),
                    );
                }
                EmailMessage::SendMail => {
                    let to = self.email.compose_to.clone();
                    let sub = self.email.compose_subject.clone();
                    let body = self.email.compose_body.text().to_string();
                    
                    let mut attachments = Vec::new();
                    for path in &self.email.compose_attachments {
                        eprintln!("[attach] reading {:?}", path);
                        if let (Some(name), Ok(data)) = (path.file_name(), std::fs::read(path)) {
                            eprintln!("[attach] attached {} ({} bytes)", name.to_string_lossy(), data.len());
                            attachments.push((name.to_string_lossy().to_string(), data));
                        } else {
                            eprintln!("[attach] FAILED to read {:?}", path);
                        }
                    }
                    
                    eprintln!("[send] sending to={:?} subject={:?} attachments={}", to, sub, attachments.len());
                    self.email.update(EmailMessage::SendMail);
                    return Command::perform(
                        async move { moon_beauty_shell::libraries::backend::email_client::send_message(&to, &sub, &body, attachments).await },
                        |res| match res {
                            Ok(_) => Message::Email(EmailMessage::MessageLoaded(
                                moon_beauty_shell::libraries::backend::email_client::ParsedEmail {
                                    text: "✓ Message sent.".into(),
                                    images: Vec::new(),
                                    attachments: Vec::new(),
                                }
                            )),
                            Err(e) => Message::Email(EmailMessage::MessageLoaded(
                                moon_beauty_shell::libraries::backend::email_client::ParsedEmail {
                                    text: format!("✗ Send failed: {}", e),
                                    images: Vec::new(),
                                    attachments: Vec::new(),
                                }
                            )),
                        }
                    );
                }
                EmailMessage::EnvelopesLoaded(envs) => {
                    self.email.update(EmailMessage::EnvelopesLoaded(envs));
                    let folder = self.email.active_folder.clone();
                    return Command::perform(
                        async move { moon_beauty_shell::libraries::backend::email_client::fetch_from_network(&folder).await },
                        |res| match res {
                            Ok(envs) => Message::Email(EmailMessage::NetworkEnvelopesLoaded(envs)),
                            Err(_) => Message::Email(EmailMessage::SyncDone),
                        }
                    );
                }
                EmailMessage::NetworkEnvelopesLoaded(envs) => {
                    self.email.update(EmailMessage::EnvelopesLoaded(envs.clone()));
                    moon_beauty_shell::libraries::backend::email_client::save_index(&self.email.active_folder, &envs);
                    let folder = self.email.active_folder.clone();
                    return Command::perform(
                        async move { moon_beauty_shell::libraries::backend::email_client::sync_bodies_background(folder).await },
                        |_| Message::Email(EmailMessage::SyncDone)
                    );
                }
                EmailMessage::SyncDone => {}
                EmailMessage::SelectMailNext | EmailMessage::SelectMailPrev => {
                    if self.email.is_composing && !self.email.get_autocomplete_matches().is_empty() {
                        let target = match msg {
                            EmailMessage::SelectMailNext => EmailMessage::SelectAutocompleteNext,
                            _ => EmailMessage::SelectAutocompletePrev,
                        };
                        self.email.update(target);
                    } else if !self.email.is_composing && !self.email.is_logging_in {
                        self.email.update(msg);
                        return self.update(Message::Email(EmailMessage::OpenSelected));
                    }
                }
                EmailMessage::SelectFolderNext | EmailMessage::SelectFolderPrev => {
                    if !self.email.is_composing && !self.email.is_logging_in {
                        let old_folder = self.email.active_folder.clone();
                        self.email.update(msg);
                        if self.email.active_folder != old_folder {
                            let f = self.email.active_folder.clone();
                            return Command::batch(vec![
                                Command::done(Message::Email(EmailMessage::SetLoading)),
                                Command::perform(
                                    async move { moon_beauty_shell::libraries::backend::email_client::fetch_envelopes(&f).await },
                                    |res| match res {
                                        Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(envs)),
                                        Err(e) => Message::Email(EmailMessage::Error(e)),
                                    }
                                )
                            ]);
                        }
                    }
                }
                EmailMessage::OpenSelected => {
                    if !self.email.is_composing {
                        if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                            let id = mail.id.clone();
                            let folder = self.email.active_folder.clone();
                            self.email.update(EmailMessage::SelectMail(id.clone()));
                            return Command::perform(
                                async move { moon_beauty_shell::libraries::backend::email_client::read_message(&id, &folder).await },
                                |body| Message::Email(EmailMessage::MessageLoaded(body)),
                            );
                        }
                    }
                }
                EmailMessage::ToggleStar => {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        let id = mail.id.clone();
                        let folder = self.email.active_folder.clone();
                        let star = !mail.is_starred;
                        self.email.update(EmailMessage::ToggleStar);
                        return Command::perform(
                            async move { moon_beauty_shell::libraries::backend::email_client::toggle_star(&id, &folder, star).await },
                            |res| match res {
                                Ok(_) => Message::Email(EmailMessage::SyncDone),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            }
                        );
                    }
                }
                EmailMessage::DeleteSelected => {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        if mail.is_starred {
                            self.email.imap_error = Some("Cannot delete a starred email. Unstar it first.".into());
                        } else {
                            let id = mail.id.clone();
                            let mail_clone = mail.clone();
                            let folder = self.email.active_folder.clone();
                            
                            self.email.envelopes.remove(self.email.selected_index);
                            if self.email.selected_index >= self.email.envelopes.len() && !self.email.envelopes.is_empty() {
                                self.email.selected_index = self.email.envelopes.len() - 1;
                            }
                            self.email.parsed_email = None;
                            self.email.active_mail_id = None;

                            moon_beauty_shell::libraries::backend::email_client::save_index(&folder, &self.email.envelopes);

                            let is_trash = folder == "Trash";

                            if !is_trash {
                                let mut trash_envs = moon_beauty_shell::libraries::backend::email_client::load_index("Trash");
                                trash_envs.insert(0, mail_clone);
                                moon_beauty_shell::libraries::backend::email_client::save_index("Trash", &trash_envs);
                            }

                            return Command::perform(
                                async move { 
                                    if is_trash {
                                        moon_beauty_shell::libraries::backend::email_client::delete_message(&id, &folder).await 
                                    } else {
                                        moon_beauty_shell::libraries::backend::email_client::move_to_trash(&id, &folder).await 
                                    }
                                },
                                |res| match res {
                                    Ok(_) => Message::Email(EmailMessage::SyncDone),
                                    Err(e) => Message::Email(EmailMessage::Error(e)),
                                }
                            );
                        }
                    }
                }
                EmailMessage::SaveContact => {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        let email = mail.sender_email.clone();
                        if !email.is_empty() {
                            return Command::perform(
                                async move { moon_beauty_shell::libraries::backend::email_client::save_contact(&email).await },
                                |res| match res {
                                    Ok(_) => Message::Email(EmailMessage::DraftSaved),
                                    Err(e) => Message::Email(EmailMessage::Error(e)),
                                }
                            );
                        }
                    }
                }
                EmailMessage::OpenInBrowser | EmailMessage::GetAppPassword | EmailMessage::SelectContact(_) | EmailMessage::SelectAutocompleteNext | EmailMessage::SelectAutocompletePrev | EmailMessage::ConfirmAutocomplete | EmailMessage::StartAddFolder | EmailMessage::AddFolderChanged(_) | EmailMessage::SubmitAddFolder | EmailMessage::RemoveFolder(_) | EmailMessage::AttachmentsPicked(_) | EmailMessage::RemoveAttachment(_) | EmailMessage::ManualAttachChanged(_) | EmailMessage::SubmitManualAttach => {
                    self.email.update(msg);
                }
                EmailMessage::AddAttachment => {
                    return Command::perform(
                        async {
                            let files = rfd::AsyncFileDialog::new()
                                .add_filter("All Files", &["*"])
                                .pick_files()
                                .await;
                            let paths = files.map(|f| {
                                f.into_iter().map(|file| file.path().to_path_buf()).collect::<Vec<_>>()
                            }).unwrap_or_default();
                            eprintln!("[attach] rfd picked: {:?}", paths);
                            paths
                        },
                        |paths| Message::Email(EmailMessage::AttachmentsPicked(paths))
                    );
                }
                EmailMessage::OpenComposer => {
                    self.email.update(msg);
                    return iced::widget::operation::focus(
                        moon_beauty_shell::overlays::email::COMPOSE_TO_ID,
                    ).map(Message::Email);
                }
                EmailMessage::SaveLogin => {
                    let cfg = moon_beauty_shell::libraries::backend::email_client::EmailConfig {
                        email: self.email.login_email.clone(),
                        password: Some(self.email.login_password.clone()),
                        password_cmd: None,
                        imap_host: self.email.login_imap.clone(),
                        imap_port: 993,
                        smtp_host: self.email.login_smtp.clone(),
                        smtp_port: 465,
                        sops_key: if self.email.login_sops_key.trim().is_empty() { None } else { Some(self.email.login_sops_key.clone()) },
                    };
                    let use_sops = self.email.use_sops;
                    match moon_beauty_shell::libraries::backend::email_client::save_config(&cfg, use_sops) {
                        Ok(_) => {
                            self.email.is_logging_in = false;
                            self.email.login_error = None;
                            return Command::batch(vec![
                                Command::done(Message::Email(EmailMessage::SetLoading)),
                                Command::perform(
                                    async move { moon_beauty_shell::libraries::backend::email_client::fetch_envelopes("INBOX").await },
                                    |res| match res {
                                        Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(envs)),
                                        Err(e) => Message::Email(EmailMessage::Error(e)),
                                    }
                                )
                            ]);
                        }
                        Err(e) => {
                            self.email.login_error = Some(e);
                        }
                    }
                }
                _ => self.email.update(msg),
            },
            Message::Notif(msg) => match msg {
                NotifMessage::DismissDrawer => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                NotifMessage::HistoryScrollTo(y) => {
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("history_list"),
                        iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                    );
                }
                NotifMessage::HistorySelectNext
                | NotifMessage::HistorySelectPrev => {
                    self.notifs.update(msg);
                    // Use the accurate scroll offset (computed from actual
                    // row heights via history_row_height) rather than a
                    // fixed-pixel guess.
                    if let Some(y) = self.notifs.scroll_offset_for_selected(&self.theme, &self.settings) {
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("history_list"),
                            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                        );
                    }
                }
                NotifMessage::HistoryDeleteSelected => {
                    self.notifs.update(msg);
                    if let Some(y) = self.notifs.scroll_offset_for_selected(&self.theme, &self.settings) {
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("history_list"),
                            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
                        );
                    }
                }
                NotifMessage::ActivateToast(id) => {
                    if let Some(t) = self.notifs.active_toasts.iter().find(|t| t.id == id).cloned() {
                        let app = t.app_name.clone();
                        let entry = t.desktop_entry.clone();
                        let nid = t.id;
                        self.notifs.update(NotifMessage::DismissToast(id));
                        std::thread::spawn(move || {
                            moon_beauty_shell::overlays::notifications::emit_action_invoked(nid, "default");
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            let matches = notification_candidates(&app, entry.as_deref());
                            if !is_app_focused(&matches) {
                                let ok = focus_notification_app(&app, entry.as_deref());
                                eprintln!("[activate-toast] fallback app={:?} entry={:?} ok={}", app, entry, ok);
                            } else {
                                eprintln!("[activate-toast] client self-focused app={:?}", app);
                            }
                        });
                    }
                }
                _ => self.notifs.update(msg),
            },
            Message::Settings(msg) => {
                {
                    let s = format!("{:?}", msg);
                    let _truncated = if s.len() > 120 { format!("{}…", &s[..120]) } else { s };
                    let _ts = self.settings.tooltip_sizes.len();
                    let _bm = &self.settings.bar_monitor;
                }

                if let SettingsMessage::TestProfileSound(idx) = msg {
                    if let Some(p) = self.settings.friend_profiles.get(idx) {
                        if let Some(path) = &p.sound_path {
                            moon_beauty_shell::overlays::notifications::NotificationOverlay::play_sound(
                                path.clone(),
                                self.settings.notif_volume,
                            );
                        }
                    }
                    return Command::none();
                }

                if let SettingsMessage::TestProfileTts(idx) = msg {
                    if let Some(p) = self.settings.friend_profiles.get(idx) {
                        moon_beauty_shell::overlays::notifications::NotificationOverlay::speak_tts(
                            "Diagnostics",
                            "Test",
                            &p.summary,
                            p.default_readout.as_deref(),
                            self.settings.tts_command.clone(),
                            self.settings.notif_volume,
                        );
                    }
                    return Command::none();
                }

                let is_test = matches!(msg, SettingsMessage::TestNotification);

                if is_test {
                    for (title, body) in [
                        ("System Diagnostics #1 (first)", "Verification toast successfully routed."),
                        ("System Diagnostics #2 (second)", "Background worker service nominal."),
                        ("System Diagnostics #3 (third)", "Telemetry stream established."),
                    ] {
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Diagnostics", title, body])
                            .spawn();
                    }
                } else if matches!(msg, SettingsMessage::Dismiss) {
                    // Persist before closing / exiting the settings process.
                    let _ = self.settings.save();
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                } else {
                    self.settings_ui.update(msg, &mut self.settings);
                    self.theme = ShellTheme::from_settings(&self.settings);

                    if self.is_standalone {
                        tokio::spawn(async {
                            let _ = send_client_command(IpcCommand::ReloadSettings).await;
                        });
                    }
                }
            }
            Message::Lock(msg) => match msg {
                LockMessage::KeyPress(key) => {
                    self.lockscreen.forward_key(&key);
                    return Command::none();
                }
                LockMessage::PollIpc => {
                    if let Some(effect) = self.lockscreen.drain_ipc() {
                        if let LockMessage::AuthSuccess = effect {
                            eprintln!("[lock] coordinator signaled success — exiting");
                            std::process::exit(0);
                        }
                    }
                    return Command::none();
                }
                _ => {}
            },
            Message::CollapseTray => {
                self.close_tray();
            }
            Message::TrayWatchdogTick => {
                // 1) Timeout check
                if self.tray_expanded
                    && self.settings.tray_timeout_sec > 0
                    && self.tray_opened_at.elapsed().as_secs()
                        >= self.settings.tray_timeout_sec as u64
                {
                    eprintln!(
                        "[tray-watchdog] elapsed={}s >= timeout={}s, closing (settings.tray_width={})",
                        self.tray_opened_at.elapsed().as_secs(),
                        self.settings.tray_timeout_sec,
                        self.settings.tray_width
                    );
                    self.close_tray();
                }

                // 2) Animation step. Note: the completion check is OUTSIDE
                //    the `elapsed < duration` guard so the snap-to-target
                //    ALWAYS runs on the first tick past duration. Previous
                //    version froze at width=89.7 because once elapsed
                //    reached 180ms, the guard `elapsed < duration` became
                //    false and the inner `if t >= 1.0 { snap }` never ran.
                if (self.tray_anim_width - self.tray_target_width).abs() > 0.01 {
                    let elapsed_ms =
                        self.tray_anim_started_at.elapsed().as_millis() as u64;
                    let dur = self.tray_anim_duration_ms.max(1);
                    let t = (elapsed_ms as f32 / dur as f32).clamp(0.0, 1.0);
                    let expanding = self.tray_target_width > self.tray_anim_start_width;
                    let eased = if expanding {
                        1.0 - (1.0 - t).powi(3)
                    } else {
                        t * t * t
                    };
                    let delta = self.tray_target_width - self.tray_anim_start_width;
                    self.tray_anim_width = self.tray_anim_start_width + delta * eased;
                    // Force-snap when past duration OR t is at ceiling.
                    if elapsed_ms >= dur || t >= 1.0 {
                        self.tray_anim_width = self.tray_target_width;
                    }
                    eprintln!(
                        "[tray-anim] elapsed={}ms t={:.2} width={:.1} target={:.1}",
                        elapsed_ms, t, self.tray_anim_width, self.tray_target_width
                    );
                }
            }
            Message::TrayAnimTick => {
                let elapsed_ms = self.tray_anim_started_at.elapsed().as_millis() as f32;
                let t = (elapsed_ms / self.tray_anim_duration_ms.max(1) as f32).clamp(0.0, 1.0);

                // Expand: ease-out-cubic (fast start, smooth settle)
                // Collapse: ease-in-cubic (slow start, snap finish)
                let expanding = self.tray_target_width > self.tray_anim_start_width;
                let eased = if expanding {
                    1.0 - (1.0 - t).powi(3)
                } else {
                    t * t * t
                };

                let delta = self.tray_target_width - self.tray_anim_start_width;
                self.tray_anim_width = self.tray_anim_start_width + delta * eased;

                if t >= 1.0 {
                    self.tray_anim_width = self.tray_target_width;
                }
            }
            Message::FetchTrayItems => {
                // Background refresh so tray_items is populated before
                // the user ever clicks the tray capsule. Eliminates the
                // 0-item → N-item re-animation on first open.
                return Command::perform(
                    moon_beauty_shell::bar::tray::watcher::fetch_tray_items(),
                    Message::TrayItemsLoaded,
                );
            }
            Message::TrayItemsLoaded(items) => {
                self.tray_items = items;
                if self.tray_expanded {
                    let new_target = moon_beauty_shell::bar::capsules::compute_expanded_visible_width(
                        &self.settings, self.tray_items.len()
                    );
                    // Re-anchor if target changed mid-animation
                    if (new_target - self.tray_target_width).abs() > 1.0 {
                        self.tray_anim_start_width = self.tray_anim_width;
                        self.tray_anim_started_at = std::time::Instant::now();
                        self.tray_target_width = new_target;
                    }
                }
            }
            Message::DismissActiveOverlay => {
                // Todo edit mode: Escape cancels the edit rather than
                // closing the overlay.
                if matches!(self.active_overlay, Some(OverlayTarget::Todo))
                    && self.todo.editing_task_id.is_some()
                {
                    self.todo.update(moon_beauty_shell::overlays::todo::TodoMessage::CancelEdit);
                    return Command::none();
                }
                // Reminders: Escape clears the current input field, then the
                // list selection, then finally dismisses. Prevents a mistyped
                // reminder from wiping out the whole overlay on Esc.
                if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
                    if !self.reminders.task_text.is_empty() {
                        self.reminders.task_text.clear();
                        return Command::none();
                    }
                    if !self.reminders.when_text.is_empty() {
                        self.reminders.when_text.clear();
                        return Command::none();
                    }
                    if self.reminders.selected_idx.is_some() {
                        self.reminders.selected_idx = None;
                        return Command::none();
                    }
                }
                // Persist if we're closing the Settings overlay via Esc.
                if matches!(self.active_overlay, Some(OverlayTarget::Settings)) {
                    let _ = self.settings.save();
                }
                if self.is_standalone {
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    });
                    return Command::none();
                }
                self.active_overlay = None;
            }
            
            Message::CheckLockscreenAlive => {
                // Only the primary bar watches the lock.
                if self.is_notif_surface || self.is_standalone {
                    return Command::none();
                }

                // Pause detection: session was suspended (TTY switch, sleep).
                // Layer surfaces held by lock children are dropped by sway
                // on output detach WITHOUT sending Closed to the client —
                // from the child's perspective nothing happened, but its
                // wl_surface is no longer composited and keyboard grab is
                // gone. Detect via elapsed-time gap and force a full respawn.
                let now = std::time::Instant::now();
                let since_last = now.saturating_duration_since(self.last_lockscreen_check);
                self.last_lockscreen_check = now;

                let was_lockscreen = matches!(
                    self.standalone_child.as_ref().map(|(t, _)| t),
                    Some(OverlayTarget::Lockscreen)
                );
                if !was_lockscreen {
                    return Command::none();
                }

                if since_last > std::time::Duration::from_millis(2000) {
                    eprintln!(
                        "[lockscreen-watchdog] session was paused ({:?}); forcing lock respawn",
                        since_last
                    );
                    self.kill_standalone();
                    self.spawn_standalone(OverlayTarget::Lockscreen);
                    return Command::none();
                }

                // Exit status discrimination:
                //   success() == true  → coordinator exited cleanly. The only
                //     code path that does this is auth success: coordinator
                //     broadcasts Success, kills children, exits(0). Respawning
                //     here would immediately re-lock the screen — that's the
                //     "locks as soon as I unlock it" bug.
                //   success() == false → crash (nonzero exit or signal kill).
                //     Respawn to keep the screen locked.
                match self.standalone_child.as_mut() {
                    Some((_, child)) => match child.try_wait() {
                        Ok(None) => {
                            // Still running.
                            return Command::none();
                        }
                        Ok(Some(status)) => {
                            if status.success() {
                                eprintln!(
                                    "[lockscreen-watchdog] coordinator exited cleanly (auth success), not respawning"
                                );
                                self.standalone_child = None;
                                return Command::none();
                            }
                            eprintln!(
                                "[lockscreen-watchdog] coordinator crashed (code={:?}), respawning",
                                status.code()
                            );
                            self.standalone_child = None;
                            self.spawn_standalone(OverlayTarget::Lockscreen);
                            return Command::none();
                        }
                        Err(e) => {
                            eprintln!(
                                "[lockscreen-watchdog] try_wait error ({e}), respawning"
                            );
                            self.standalone_child = None;
                            self.spawn_standalone(OverlayTarget::Lockscreen);
                            return Command::none();
                        }
                    },
                    None => return Command::none(),
                }
            }
            Message::Noop => {}
            Message::SetToastInputRegion(_) => {
                // Intercepted by `try_into` — never reaches update.
            }
            Message::CloseShell => {
                std::process::exit(0);
            }
        }
        Command::none()
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let clock_sub = iced::time::every(std::time::Duration::from_millis(1000)).map(|_| Message::TickClock);
        let toast_sub = if self.is_notif_surface && !self.notifs.active_toasts.is_empty() {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::TickToasts)
        } else {
            iced::Subscription::none()
        };

        let email_sync_sub = if moon_beauty_shell::libraries::backend::email_client::config_exists() {
            iced::time::every(std::time::Duration::from_secs(60)).map(|_| Message::TickEmailSync)
        } else {
            iced::Subscription::none()
        };
                
        let email_sub = if matches!(self.active_overlay, Some(OverlayTarget::Email)) {
            iced::event::listen_with(handle_email_keypress)
        } else {
            iced::Subscription::none()
        };
        
        let hw_sub = iced::time::every(std::time::Duration::from_millis(self.settings.hardware_poll_interval_ms)).map(|_| Message::TickHardware);
        let weather_sub = iced::time::every(std::time::Duration::from_secs(900)).map(|_| Message::TickWeather);

        let clip_sub = if matches!(self.active_overlay, Some(OverlayTarget::Clipboard)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else { return None; };
                match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => Some(Message::Clipboard(ClipboardMessage::SelectPrev)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => Some(Message::Clipboard(ClipboardMessage::SelectNext)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => Some(Message::Clipboard(ClipboardMessage::CopySelected)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => Some(Message::Clipboard(ClipboardMessage::DeleteSelected)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => Some(Message::Clipboard(ClipboardMessage::DeleteSelected)),
                _ => None,
            }
            })
        } else {
            iced::Subscription::none()
        };

        let text_typing_sub = if self.annotator.text_editing.is_some() {
            iced::event::listen_with(handle_annotator_text_input)
        } else {
            iced::Subscription::none()
        };

        let _todo_sub = if matches!(self.active_overlay, Some(OverlayTarget::Todo)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
                    return None;
                };
                // Cycle categories with L/R, cycle filter with U/D.
                // (text_input also sees these but only cursor-moves on
                // non-empty input; empty input leaves them free.)
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft) => {
                        Some(Message::Todo(TodoMessage::CategoryPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight) => {
                        Some(Message::Todo(TodoMessage::CategoryNext))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(Message::Todo(TodoMessage::FilterPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(Message::Todo(TodoMessage::FilterNext))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let todo_sub = if matches!(self.active_overlay, Some(OverlayTarget::Todo)) {
            iced::event::listen_with(|event, status, _window| {
                // NOTE: no status gate. text_input returns Captured for
                // Delete/arrows even when empty (it "handles" a no-op).
                // If we gated on status, Delete would never reach us.
                // Instead, each message handler checks new_task_text.is_empty()
                // to decide whether it should act, so text editing still
                // works normally when there's text in the field.
                if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key: ref dbg_key, .. }) = event {
                    eprintln!("[todo-sub] key={:?} status={:?}", dbg_key, status);
                }
                let iced::Event::Keyboard(
                    iced::keyboard::Event::KeyPressed { key, .. }
                ) = event else {
                    return None;
                };
                use iced::keyboard::{Key, key::Named};
                match key {
                    Key::Named(Named::ArrowLeft)  => Some(Message::Todo(TodoMessage::CategoryPrev)),
                    Key::Named(Named::ArrowRight) => Some(Message::Todo(TodoMessage::CategoryNext)),
                    Key::Named(Named::ArrowUp)    => Some(Message::Todo(TodoMessage::TaskSelectPrev)),
                    Key::Named(Named::ArrowDown)  => Some(Message::Todo(TodoMessage::TaskSelectNext)),
                    Key::Named(Named::Delete)     => Some(Message::Todo(TodoMessage::DeleteSelected)),
                    // NOTE: Enter is intentionally absent here. text_input
                    // captures Enter and fires AddTask (via on_submit).
                    // AddTask dispatches 3-way: save-edit / add-new / toggle.
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let reminders_tick_sub = if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
            iced::time::every(std::time::Duration::from_millis(1000))
                .map(|_| Message::Reminders(ReminderMessage::Tick))
        } else {
            iced::Subscription::none()
        };

        let reminders_sub = if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
                    return None;
                };
                use iced::keyboard::{Key, key::Named};
                match key {
                    // Tab / Shift+Tab cycle between the two input fields
                    Key::Named(Named::Tab) if modifiers.shift() => {
                        Some(Message::Reminders(ReminderMessage::FocusPrevField))
                    }
                    Key::Named(Named::Tab) => {
                        Some(Message::Reminders(ReminderMessage::FocusNextField))
                    }
                    // ArrowUp/Down scroll the list regardless of input focus
                    Key::Named(Named::ArrowUp) => {
                        Some(Message::Reminders(ReminderMessage::ScrollUp))
                    }
                    Key::Named(Named::ArrowDown) => {
                        Some(Message::Reminders(ReminderMessage::ScrollDown))
                    }
                    Key::Named(Named::Delete) => {
                        Some(Message::Reminders(ReminderMessage::DeleteSelected))
                    }
                    Key::Named(Named::Home) => {
                        Some(Message::Reminders(ReminderMessage::SelectFirst))
                    }
                    Key::Named(Named::End) => {
                        Some(Message::Reminders(ReminderMessage::SelectLast))
                    }
                    Key::Character(c) if modifiers.control() && c.to_lowercase() == "s" => {
                        Some(Message::Reminders(ReminderMessage::ToggleSoundSelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        

        let rng_sub = if matches!(self.active_overlay, Some(OverlayTarget::Rng)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
                    return None;
                };
                use iced::keyboard::{Key, key::Named};
                match key {
                    Key::Named(Named::Enter) => Some(Message::Rng(RngMessage::ExecuteRoll)),
                    Key::Named(Named::ArrowLeft) | Key::Named(Named::Tab) if !iced::keyboard::Modifiers::default().shift() => Some(Message::Rng(RngMessage::CycleFieldPrev)),
                    Key::Named(Named::ArrowRight) | Key::Named(Named::Tab) => Some(Message::Rng(RngMessage::CycleFieldNext)),
                    Key::Named(Named::ArrowUp) => Some(Message::Rng(RngMessage::AdjustField(true))),
                    Key::Named(Named::ArrowDown) => Some(Message::Rng(RngMessage::AdjustField(false))),
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let notes_sub = if matches!(self.active_overlay, Some(OverlayTarget::Notes)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
                    return None;
                };
                eprintln!("[notes-sub] key={:?}", key);
                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(Message::Notes(NotesMessage::SelectNext))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(Message::Notes(NotesMessage::SelectPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => {
                        Some(Message::Notes(NotesMessage::DeleteSelected))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let unicode_sub = if matches!(self.active_overlay, Some(OverlayTarget::Unicode)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
                    return None;
                };
                use iced::keyboard::{Key, key::Named};
                match key {
                    Key::Named(Named::ArrowLeft)  => Some(Message::Unicode(UnicodeMessage::SelectPrev)),
                    Key::Named(Named::ArrowRight) => Some(Message::Unicode(UnicodeMessage::SelectNext)),
                    Key::Named(Named::ArrowUp)    => Some(Message::Unicode(UnicodeMessage::SelectPrevRow)),
                    Key::Named(Named::ArrowDown)  => Some(Message::Unicode(UnicodeMessage::SelectNextRow)),
                    Key::Named(Named::Enter)      => Some(Message::Unicode(UnicodeMessage::CopySelected)),
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let history_sub = if matches!(self.active_overlay, Some(OverlayTarget::NotificationsHistory)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else {
                    return None;
                };
                use iced::keyboard::{Key, key::Named};
                match key {
                    Key::Named(Named::ArrowUp) => Some(Message::Notif(NotifMessage::HistorySelectPrev)),
                    Key::Named(Named::ArrowDown) => Some(Message::Notif(NotifMessage::HistorySelectNext)),
                    Key::Named(Named::Delete) => Some(Message::Notif(NotifMessage::HistoryDeleteSelected)),
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let launcher_sub = if matches!(self.active_overlay, Some(OverlayTarget::Launcher)) {
            // Use listen_with (not on_key_press) so Left/Right events reach us
            // even though text_input consumes them for cursor movement.
            iced::event::listen_with(|event, status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
                    return None;
                };
                let shift = modifiers.shift();

                // Debug: log arrow keys with their capture status
                if matches!(
                    key,
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft)
                        | iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight)
                ) {
                    eprintln!("[launcher-key] arrow status={:?}", status);
                }

                match key {
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                        Some(Message::Launcher(LauncherMessage::SelectUp))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                        Some(Message::Launcher(LauncherMessage::SelectDown))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowRight) => {
                        Some(Message::Launcher(LauncherMessage::SelectNext))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowLeft) => {
                        Some(Message::Launcher(LauncherMessage::SelectPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) if shift => {
                        Some(Message::Launcher(LauncherMessage::SelectPrev))
                    }
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab) => {
                        Some(Message::Launcher(LauncherMessage::SelectNext))
                    }
                    _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let lock_jump_sub = if self.is_standalone && matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            iced::time::every(std::time::Duration::from_millis(500)).map(|_| Message::Lock(LockMessage::Tick))
        } else {
            iced::Subscription::none()
        };

        let lock_key_sub = if matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            iced::event::listen_with(handle_lock_keypress)
        } else {
            iced::Subscription::none()
        };

        let lock_ipc_sub = if matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::Lock(LockMessage::PollIpc))
        } else {
            iced::Subscription::none()
        };

        let esc_sub = if matches!(self.active_overlay, Some(OverlayTarget::Email)) {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else { return None; };
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    Some(Message::Email(EmailMessage::Dismiss))
                } else { None }
            })
        } else if self.annotator.text_editing.is_some() {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else { return None; };
                match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                    Some(Message::Annotator(AnnotatorMessage::CancelText))
                }
                _ => None,
            }
            })
        } else if matches!(self.active_overlay, Some(OverlayTarget::Lockscreen)) {
            // Lock screen cannot be dismissed with Escape
            iced::Subscription::none()
        } else {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else { return None; };
                {
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    Some(Message::DismissActiveOverlay)
                } else {
                    None
                }
            }
            })
        };

        // Enter → save when annotator overlay is active.
        let annotator_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator)) && self.annotator.text_editing.is_none() {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else { return None; };
                match key {
                iced::keyboard::Key::Character(c) => match c.as_str() {
                    "r" | "R" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Rect))),
                    "o" | "O" | "c" | "C" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Ellipse))),
                    "a" | "A" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Arrow))),
                    "p" | "P" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Pen))),
                    "t" | "T" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Text))),
                    "n" | "N" => Some(Message::Annotator(AnnotatorMessage::SetTool(Tool::Counter))),
                    _ => None,
                },
                _ => None,
            }
            })
        } else {
            iced::Subscription::none()
        };

        let enter_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator)) && self.annotator.text_editing.is_none() {
            iced::event::listen_with(|event, _status, _window| {
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, .. }) = event else { return None; };
                match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                    Some(Message::Annotator(AnnotatorMessage::Save))
                }
                _ => None,
            }
            })
        } else {
            iced::Subscription::none()
        };


                                let ipc_sub = if self.is_standalone || self.is_notif_surface {
            iced::Subscription::none()
        } else {
            iced::time::every(std::time::Duration::from_millis(50))
                .map(|_| Message::PollIpc)
        };

        // Lockscreen watchdog: 500ms cadence. Combined with the
        // coordinator's own 200ms child-watchdog, the two together
        // mean the *screen is locked* or *will be locked within 800ms*
        // under any crash scenario. Not bulletproof, but small enough
        // that a human attacker cannot reliably exploit the window.
        let lockscreen_watchdog_sub = if self.is_standalone || self.is_notif_surface {
            iced::Subscription::none()
        } else {
            iced::time::every(std::time::Duration::from_millis(500))
                .map(|_| Message::CheckLockscreenAlive)
        };

        let notif_sub = if self.is_notif_surface {
            iced::time::every(std::time::Duration::from_millis(50))
                .map(|_| Message::PollNotifs)
        } else if !self.is_standalone {
            iced::time::every(std::time::Duration::from_millis(500))
                .map(|_| Message::PollNotifs)
        } else {
            iced::Subscription::none()
        };
        let resize_sub = iced::window::resize_events()
            .map(|(_id, size)| Message::WindowResized(size.width));
        let elapsed = self.tray_anim_started_at.elapsed().as_millis() as u64;
        let anim_active = elapsed < self.tray_anim_duration_ms
            && (self.tray_anim_width - self.tray_target_width).abs() > 0.01;
        // Standalone window-close listener: when Sway detaches the surface
        // on a TTY switch, it sends a Closed event. By exiting, we let the
        // bar's lockscreen_watchdog see a dead child and respawn it.
        let close_sub = if self.is_standalone {
            iced::event::listen_with(|event, _, _| {
                if let iced::Event::Window(iced::window::Event::Closed) = event {
                    Some(Message::CloseShell)
                } else {
                    None
                }
            })
        } else {
            iced::Subscription::none()
        };

        let anim_sub = if anim_active {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::TrayAnimTick)
        } else {
            iced::Subscription::none()
        };
        // Tray watchdog: dedicated 500ms timer while expanded. Fixes the
        // "tray never closes" bug where the Command::perform-based timer
        // silently failed to fire. Polling is cheap — only active while
        // tray_expanded is true.
        // Tray watchdog sub drives both the timeout check AND the
        // collapse/expand animation. 16ms cadence while animating,
        // 500ms cadence while steady-expanded (timeout check only).
        // This replaces reliance on TrayAnimTick's sub, which has been
        // observed to silently not fire in this iced version (same
        // failure class as the earlier Command::perform timer).
        let tray_animating = (self.tray_anim_width - self.tray_target_width).abs() > 0.01;
        // Tray items pre-fetch: every 15s, keeps the item list warm so
        // first open animates straight to the correct width instead of
        // briefly showing 0 items (target=206) then re-animating to N
        // items (target=186). This is a low-cost D-Bus call per item.
        let tray_fetch_sub = if self.is_notif_surface || self.is_standalone {
            iced::Subscription::none()
        } else {
            iced::time::every(std::time::Duration::from_secs(15))
                .map(|_| Message::FetchTrayItems)
        };

        let tray_watchdog_sub = if tray_animating {
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::TrayWatchdogTick)
        } else if self.tray_expanded && self.settings.tray_timeout_sec > 0 {
            iced::time::every(std::time::Duration::from_millis(500))
                .map(|_| Message::TrayWatchdogTick)
        } else {
            iced::Subscription::none()
        };
        iced::Subscription::batch(vec![clock_sub, toast_sub, hw_sub, email_sync_sub, weather_sub, anim_sub, esc_sub,
            enter_sub, ipc_sub, notif_sub, resize_sub, clip_sub, annotator_sub, text_typing_sub, email_sub,
            launcher_sub, history_sub, todo_sub, notes_sub, rng_sub, reminders_sub, reminders_tick_sub, unicode_sub,
            lock_jump_sub, lock_key_sub, lock_ipc_sub, lockscreen_watchdog_sub, tray_watchdog_sub, tray_fetch_sub, close_sub])
    }

    pub fn view(&self) -> Element<'_, Message> {
        {
            static ONCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !ONCE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            }
        }
        if self.is_notif_surface {
            let is_right_corner = self.settings.notif_corner.contains("right");
            let is_top_corner = self.settings.notif_corner.contains("top");
            let pad_top = if is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
            let pad_bottom = if !is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
            let pad_right = if is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };
            let pad_left = if !is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };
            return container(self.notifs.view_toasts(&self.theme, &self.settings, self.window_w).map(Message::Notif))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(if is_right_corner {
                    iced::alignment::Horizontal::Right
                } else {
                    iced::alignment::Horizontal::Left
                })
                .align_y(if is_top_corner {
                    iced::alignment::Vertical::Top
                } else {
                    iced::alignment::Vertical::Bottom
                })
                .padding(Padding { top: pad_top, right: pad_right, bottom: pad_bottom, left: pad_left })
                .into();
        }
        let is_right_corner = self.settings.notif_corner.contains("right");
        let is_top_corner = self.settings.notif_corner.contains("top");

        let pad_top = if is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
        let pad_bottom = if !is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
        let pad_right = if is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };
        let pad_left = if !is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };

        let notifs_view = container(self.notifs.view_toasts(&self.theme, &self.settings, self.window_w).map(Message::Notif))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(if is_right_corner { iced::alignment::Horizontal::Right } else { iced::alignment::Horizontal::Left })
            .align_y(if is_top_corner { iced::alignment::Vertical::Top } else { iced::alignment::Vertical::Bottom })
            .padding(Padding {
                top: pad_top,
                right: pad_right,
                bottom: pad_bottom,
                left: pad_left,
            })
            .style(|_t| iced::widget::container::Style {
                background: None,
                ..Default::default()
            });

        let overlay_element: Option<Element<'_, Message>> = match &self.active_overlay {
            Some(OverlayTarget::Launcher) => Some(self.launcher.view(&self.theme, &self.settings).map(Message::Launcher)),
            Some(OverlayTarget::Calc) => Some(self.calc.view(&self.theme, &self.settings).map(Message::Calc)),
            Some(OverlayTarget::Clipboard) => Some(self.clipboard.view(&self.theme, &self.settings).map(Message::Clipboard)),
            Some(OverlayTarget::Notes) => Some(self.notes.view(&self.theme, &self.settings).map(Message::Notes)),
            Some(OverlayTarget::Pass) => Some(self.pass.view(&self.theme, &self.settings).map(Message::Pass)),
            Some(OverlayTarget::Power) => Some(self.power.view(&self.theme).map(Message::Power)),
            Some(OverlayTarget::Todo) => Some(self.todo.view(&self.theme, &self.settings).map(Message::Todo)),
            Some(OverlayTarget::Reminders) => Some(self.reminders.view(&self.theme, &self.settings).map(Message::Reminders)),
            Some(OverlayTarget::Dictionary) => Some(self.dict.view(&self.theme, &self.settings).map(Message::Dict)),
            Some(OverlayTarget::Unicode) => Some(self.unicode.view(&self.theme, &self.settings).map(Message::Unicode)),
            Some(OverlayTarget::Web) => Some(self.web.view(&self.theme).map(Message::Web)),
            Some(OverlayTarget::Amogus) => Some(self.amogus.view(&self.theme, &self.settings).map(Message::Amogus)),
            Some(OverlayTarget::Rng) => Some(self.rng.view(&self.theme, &self.settings).map(Message::Rng)),
            Some(OverlayTarget::Email) => Some(self.email.view(&self.theme, &self.settings).map(Message::Email)),
            Some(OverlayTarget::NotificationsHistory) => Some(self.notifs.view_history(&self.theme, &self.settings).map(Message::Notif)),
            Some(OverlayTarget::Settings) => Some(self.settings_ui.view(&self.settings, &self.theme).map(Message::Settings)),
            Some(OverlayTarget::Lockscreen) => Some(self.lockscreen.view(&self.theme).map(Message::Lock)),
            Some(OverlayTarget::Annotator) => Some(self.annotator.view().map(Message::Annotator)),
            None => None,
        };

        if self.is_standalone {
            if let Some(overlay) = overlay_element {
                return Stack::new().push(overlay).push(notifs_view).into();
            } else {
                eprintln!("[FATAL-EXIT] is_standalone=true but overlay_element=None");
                eprintln!("[FATAL-EXIT] active_overlay = {:?}", self.active_overlay);
                eprintln!("[FATAL-EXIT] is_notif_surface = {}", self.is_notif_surface);
                std::process::exit(0);
            }
        }

        let bar_view = TopBar::view(&self.settings, &self.theme, &self.telemetry, self.tray_expanded, &self.tray_items, self.tray_anim_width).map(Message::TopBar);

        let base_content: Element<'_, Message> = bar_view;



        if let Some(overlay) = overlay_element {
            Stack::new().push(base_content).push(overlay).into()
        } else {
            base_content
        }
    }
}
