use iced::widget::{container, Stack};
use iced::{Color, Element, Length, Padding, Task as Command, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;

use moon_beauty_shell::backend::*;
use moon_beauty_shell::bar::{CapsuleAction, TopBar, TopBarMessage};
use moon_beauty_shell::config::{ShellSettings, ShellTheme};
use moon_beauty_shell::ipc::{send_client_command, IpcCommand, OverlayTarget};
use moon_beauty_shell::overlays::*;
use moon_beauty_shell::overlays::annotator::Tool;

mod screenshot;

fn detect_notif_surface(args: &[String]) -> bool {
    args.iter().any(|a| a == "--notif-surface")
}

fn detect_standalone_overlay(args: &[String]) -> Option<OverlayTarget> {
    for (i, arg) in args.iter().enumerate() {
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

fn resolve_exe() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            // /proc/PID/exe on Linux returns "…/bin (deleted)" after `cargo build`
            // replaces the running binary. Strip the suffix so spawn still works.
            if let Some(stripped) = s.strip_suffix(" (deleted)") {
                std::path::PathBuf::from(stripped)
            } else {
                p
            }
        })
        .unwrap_or_else(|| std::path::PathBuf::from("moon-beauty-shell"))
}

pub fn main() -> Result<(), iced_layershell::Error> {
    let args: Vec<String> = std::env::args().collect();

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

    if args.iter().any(|a| a == "--freeze-selector") {
        let idx = args.iter().position(|a| a == "--freeze-selector").unwrap();
        let input = args.get(idx + 1).cloned().unwrap_or_default();
        let output = args.get(idx + 2).cloned().unwrap_or_default();
        let binded = args.get(idx + 3).cloned();
        if input.is_empty() || output.is_empty() {
            eprintln!("usage: --freeze-selector <input.png> <output.png> [output-name]");
            std::process::exit(2);
        }
        match screenshot::selector::run(
            std::path::PathBuf::from(input),
            std::path::PathBuf::from(output),
            binded,
        ) {
            Ok(_) => std::process::exit(0),
            Err(e) => { eprintln!("[freeze-selector] {e}"); std::process::exit(1); }
        }
    }

    // Orchestrator: capture focused output → spawn child → report crop
    if args.iter().any(|a| a == "--freeze-test") {
        let bytes = match screenshot::capture::capture_output(None) {
            Ok(b) => b,
            Err(e) => { eprintln!("[freeze] capture failed: {e}"); std::process::exit(1); }
        };
        let img = match image::load_from_memory(&bytes) {
            Ok(i) => i,
            Err(e) => { eprintln!("[freeze] decode failed: {e}"); std::process::exit(1); }
        };
        eprintln!("[freeze] captured {}x{} ({} bytes)", img.width(), img.height(), bytes.len());

        let tmp_in = "/tmp/mbs-freeze-in.png";
        let tmp_out = "/tmp/mbs-freeze-out.png";
        std::fs::write(tmp_in, &bytes).ok();
        let _ = std::fs::remove_file(tmp_out);

        let exe = resolve_exe();
        let status = std::process::Command::new(exe)
            .args(["--freeze-selector", tmp_in, tmp_out])
            .status();
        match status {
            Ok(s) if s.success() && std::path::Path::new(tmp_out).exists() => {
                let cropped = std::fs::read(tmp_out).unwrap_or_default();
                if let Ok(i) = image::load_from_memory(&cropped) {
                    eprintln!("[freeze] ✓ cropped {}x{} → {}", i.width(), i.height(), tmp_out);
                }
                std::process::exit(0);
            }
            Ok(_) => { eprintln!("[freeze] cancelled"); std::process::exit(1); }
            Err(e) => { eprintln!("[freeze] child spawn failed: {e}"); std::process::exit(1); }
        }
    }

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

    let is_notif_surface = detect_notif_surface(&args);
    let standalone_target = detect_standalone_overlay(&args);
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
            binded_output_name,
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
            binded_output_name,
            ..Default::default()
        }
    } else if standalone_target.is_some() {
        LayerShellSettings {
            size: Some((0, 0)),
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            binded_output_name,
            ..Default::default()
        }
    } else {
        LayerShellSettings {
            size: Some((0, settings.bar_height)),
            exclusive_zone: settings.bar_height as i32,
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            layer: Layer::Top,
            keyboard_interactivity: KeyboardInteractivity::None,
            binded_output_name,
            ..Default::default()
        }
    };

        if args.iter().any(|a| a == "--sys-tooltip") {
        match moon_beauty_shell::overlays::system_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => { eprintln!("[sys-tooltip] {e}"); std::process::exit(1); }
        }
    }

    if args.iter().any(|a| a == "--tray-popup") {
        match moon_beauty_shell::overlays::tray_popup::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => { eprintln!("[tray-popup] {e}"); std::process::exit(1); }
        }
    }

    if args.iter().any(|a| a == "--weather-tooltip") {
        match moon_beauty_shell::overlays::weather_tooltip::run() {
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

    if args.iter().any(|a| a == "--alarm-tooltip") {
        match moon_beauty_shell::overlays::alarm_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => { eprintln!("[alarm-tooltip] {e}"); std::process::exit(1); }
        }
    }

    MoonBeautyShellApp::run(Settings {
        layer_settings,
        ..Default::default()
    })
}

#[derive(Debug, Clone)]
pub enum Message {
    TickHardware,
    TickClock,
    TickToasts,
    MetricsCollected(CpuMetrics, RamMetrics, GpuMetrics, AudioStatus, AudioStatus, Option<MprisTrack>),
    IpcReceived(IpcCommand),
    PollIpc,
    PollNotifs,
    ResizeNotifSurface(u32, u32),
    WindowResized(f32),
    TopBar(TopBarMessage),
    Launcher(LauncherMessage),
    Calc(CalcMessage),
    Clipboard(ClipboardMessage),
    Notes(NotesMessage),
    Pass(PassMessage),
    Power(PowerMessage),
    Todo(TodoMessage),
    Reminders(ReminderMessage),
    Dict(DictMessage),
    Unicode(UnicodeMessage),
    Web(WebSearchMessage),
    Amogus(AmogusMessage),
    Annotator(AnnotatorMessage),
    Rng(RngMessage),
    Email(EmailMessage),
    Notif(NotifMessage),
    Settings(SettingsMessage),
    Lock(LockMessage),
    DismissActiveOverlay,
    CloseShell,
}

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(
        self,
    ) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error>
    {
        match self {
            Message::ResizeNotifSurface(w, h) => Ok(
                iced_layershell::actions::LayershellCustomActionsWithInfo::SizeChange((w, h)),
            ),
            other => Err(other),
        }
    }
}

pub struct MoonBeautyShellApp {
    weather_tooltip_child: Option<std::process::Child>,
    settings: ShellSettings,
    theme: ShellTheme,
    telemetry: UnifiedTelemetry,
    active_overlay: Option<OverlayTarget>,
    is_standalone: bool,
    system_collector: SystemCollector,
    standalone_child: Option<(OverlayTarget, std::process::Child)>,
    ipc_rx: std::sync::mpsc::Receiver<IpcCommand>,
    is_notif_surface: bool,
    notif_rx: Option<std::sync::mpsc::Receiver<moon_beauty_shell::backend::notif_dbus::IncomingNotification>>,
    settings_json_mtime: Option<std::time::SystemTime>,
    window_w: f32,
    notif_screen_at_startup: String,
    notif_surface_size: (u32, u32),

    launcher: AppLauncher,
    calc: CalculatorOverlay,
    clipboard: ClipboardOverlay,
    notes: NotesOverlay,
    pass: PassOverlay,
    power: PowerOverlay,
    todo: TodoOverlay,
    reminders: RemindersOverlay,
    dict: DictionaryOverlay,
    unicode: UnicodeOverlay,
    web: WebSearchOverlay,
    amogus: AmogusOverlay,
    annotator: AnnotatorOverlay,
    rng: RngOverlay,
    email: EmailOverlay,
    notifs: NotificationOverlay,
    settings_ui: SettingsOverlay,
    lockscreen: LockscreenOverlay,
}

impl MoonBeautyShellApp {
    fn standalone_running(&mut self) -> Option<OverlayTarget> {
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

    fn kill_standalone(&mut self) {
        if let Some((_, mut child)) = self.standalone_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn spawn_standalone(&mut self, target: OverlayTarget) {
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
            Ok(c) => self.standalone_child = Some((target, c)),
            Err(e) => eprintln!("[ipc] spawn {:?} failed: {e}", target),
        }
    }

    fn toggle_standalone(&mut self, target: OverlayTarget) {
        match self.standalone_running() {
            Some(ref t) if t == &target => self.kill_standalone(),
            _ => self.spawn_standalone(target),
        }
    }
}

impl iced_layershell::Application for MoonBeautyShellApp {
    type Message = Message;
    type Flags = ();
    type Theme = Theme;
    type Executor = iced::executor::Default;

    fn namespace(&self) -> String {
        "moon-beauty-shell".into()
    }

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        let args: Vec<String> = std::env::args().collect();
        let standalone_target = detect_standalone_overlay(&args);
        let is_notif_surface = detect_notif_surface(&args);
        let is_standalone = standalone_target.is_some();
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);

        let (ipc_tx, ipc_rx) = std::sync::mpsc::channel::<IpcCommand>();
        if standalone_target.is_none() && !is_notif_surface {
            std::thread::spawn(move || {
                use std::io::Read;
                use std::os::unix::net::UnixListener;
                let path = moon_beauty_shell::ipc::get_socket_path();
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
            moon_beauty_shell::backend::notif_dbus::spawn(tx);
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

            launcher: AppLauncher::new(),
            calc: CalculatorOverlay::new(),
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
        };

        let init_task = if matches!(app.active_overlay, Some(OverlayTarget::Launcher)) {
            iced::widget::text_input::focus("launcher_search")
                .map(Message::Launcher)
        } else if matches!(app.active_overlay, Some(OverlayTarget::Clipboard)) {
            Command::perform(
                ClipboardOverlay::reload(),
                |entries| Message::Clipboard(ClipboardMessage::Reloaded(entries)),
            )
        } else {
            Command::none()
        };
        (app, init_task)
    }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance {
            background_color: Color::TRANSPARENT,
            text_color: self.theme.text,
        }
    }

    fn update(&mut self, message: Message) -> Command<Message> {
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
                                if let Some(first) = self.notifs.active_toasts.first().cloned() {
                                    self.notifs.update(NotifMessage::DismissToast(first.id));
                                }
                            }
                            "activate-latest" => {
                                if let Some(t) = self.notifs.active_toasts.last().cloned() {
                                    let app = t.app_name.clone();
                                    let entry = t.desktop_entry.clone();
                                    let nid = t.id;
                                    self.notifs.update(NotifMessage::DismissToast(t.id));
                                    std::thread::spawn(move || {
                                        moon_beauty_shell::backend::notif_dbus::emit_action_invoked(nid, "default");
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
                                if self.notifs.active_toasts.len() >= 4 {
                                    self.notifs.active_toasts.remove(0);
                                }
                                self.notifs.active_toasts.push(item);
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
                    if want != self.notif_surface_size {
                        self.notif_surface_size = want;
                        return Command::done(Message::ResizeNotifSurface(want.0, want.1));
                    }
                }
            }
            Message::TickClock => {
                self.telemetry.bar_time = ClockBackend::get_bar_time();
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
                    }
                }
                if changed {
                    moon_beauty_shell::overlays::reminders::save_reminders(&entries);
                }
            }
            Message::TickToasts => {
                self.notifs.tick_toasts(self.settings.notif_hold_duration_sec);
            }
            Message::TickHardware => {
                let bat = SystemCollector::collect_battery();
                self.telemetry.battery = bat;
                self.telemetry.net = self.system_collector.update_net();

                return Command::perform(
                    async move {
                        let cpu = SystemCollector::collect_cpu().await;
                        let ram = SystemCollector::collect_ram().await;
                        let gpu = SystemCollector::collect_gpu("card0", false, 0).await;
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
                    self.toggle_standalone(target);
                }
                IpcCommand::Open(target, arg) => {
                    if matches!(target, OverlayTarget::Launcher | OverlayTarget::Clipboard | OverlayTarget::NotificationsHistory | OverlayTarget::Settings) {
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
                    if let Some(first) = self.notifs.active_toasts.first().cloned() {
                        self.notifs.update(NotifMessage::DismissToast(first.id));
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
                if let TopBarMessage::Action(act) = msg {
                    match act {
                        CapsuleAction::OpenOverlay(id) => {
                            eprintln!("[topbar-action] OpenOverlay: id = {:?}", id);

                            // 1. DROPDOWN TOOLTIPS (Alarm, CPU, GPU, RAM, Weather, Tray)
                            if matches!(id.as_str(), "weather" | "cpu" | "gpu" | "ram" | "tray" | "alarm" | "calendar" | "music" | "unified" | "net" | "custom") {
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
                                let cx = capsule_center_x(&self.settings, sw, &id);
                                let exe = resolve_exe();
                                let mut cmd = std::process::Command::new(&exe);

                                if id == "alarm" {
                                    cmd.arg("--alarm-tooltip");
                                } else if id == "weather" {
                                    cmd.arg("--weather-tooltip");
                                } else if id == "tray" {
                                    cmd.arg("--tray-popup");
                                } else if matches!(id.as_str(), "calendar" | "music" | "unified" | "net" | "custom") {
                                    cmd.arg("--info-tooltip").env("MBS_INFO_MODE", &id);
                                } else {
                                    cmd.arg("--sys-tooltip").env("MBS_SYS_TARGET", &id);
                                }

                                let slant = if self.settings.bar_left.contains(&id) {
                                    "left"
                                } else if self.settings.bar_right.contains(&id) {
                                    "right"
                                } else {
                                    "center"
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
                                eprintln!(
                                    "[spawn] id={:?} cap_w={:.1} cx={} sw={} in_L={} in_R={} in_C={} slant_mode={:?}",
                                    id, cap_w, cx, sw,
                                    self.settings.bar_left.contains(&id),
                                    self.settings.bar_right.contains(&id),
                                    self.settings.bar_center.contains(&id),
                                    self.settings.slant_mode
                                );
                                let grow_right = if in_left { true } else if in_right { false } else { cx < (sw as i32) / 2 };
                                
                                cmd.env("MBS_TOOLTIP_W", cap_w.max(380.0).to_string())
                                   .env("MBS_CAPSULE_CX", cx.to_string())
                                   .env("MBS_CAPSULE_W", cap_w.to_string())
                                   .env("MBS_SCREEN_W", sw.to_string())
                                   .env("MBS_SLANT_L", slant_l)
                                   .env("MBS_SLANT_R", slant_r);

                                eprintln!("[tooltip] Spawning {:?} {:?} (cx={}, sw={}, sl={}, sr={})", exe, cmd.get_args().collect::<Vec<_>>(), cx, sw, slant_l, slant_r);
                                match cmd.spawn() {
                                    Ok(c) => {
                                        eprintln!("[tooltip] Successfully spawned pid={}", c.id());
                                        self.weather_tooltip_child = Some(c);
                                    }
                                    Err(e) => eprintln!("[tooltip] Failed to spawn: {e}"),
                                }
                                return Command::none();
                            }

                            // 2. FULLSCREEN / STANDALONE OVERLAYS (Settings, History, Power)
                            let mapped: Option<OverlayTarget> = match id.as_str() {
                                "calendar" | "clock" => Some(OverlayTarget::Settings),
                                "unified" | "net" => Some(OverlayTarget::Settings),
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
                        CapsuleAction::ToggleSourceMute => {
                            return Command::perform(AudioBackend::toggle_source_mute(), |_| Message::TopBar(TopBarMessage::Noop));
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
                CalcMessage::CopyResult(val) => {
                    if self.is_standalone {
                        tokio::spawn(async move { CalculatorOverlay::copy_to_clipboard(&val).await; });
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
                    iced::widget::scrollable::scroll_to(
                        iced::widget::scrollable::Id::new("clip_list"),
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
                    iced::widget::scrollable::scroll_to(
                        iced::widget::scrollable::Id::new("clip_list"),
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
                                iced::widget::scrollable::scroll_to(
                                    iced::widget::scrollable::Id::new("clip_list"),
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
                _ => self.notes.update(msg),
            },
            Message::Pass(msg) => match msg {
                PassMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                PassMessage::CopyKey(k) => {
                    if self.is_standalone {
                        tokio::spawn(async move { PassOverlay::copy_password(&k).await; });
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
                _ => self.todo.update(msg),
            },
            Message::Reminders(msg) => match msg {
                ReminderMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.reminders.update(msg),
            },
            Message::Dict(msg) => match msg {
                DictMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                DictMessage::SearchWord => {
                    let w = self.dict.word_query.clone();
                    return Command::perform(async move { (w.clone(), DictionaryOverlay::lookup_word(&w).await) }, |(w, defs)| {
                        Message::Dict(DictMessage::DefinitionsLoaded(w, defs))
                    });
                }
                _ => self.dict.update(msg),
            },
            Message::Unicode(msg) => match msg {
                UnicodeMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                UnicodeMessage::CopySymbol(s) => {
                    if self.is_standalone {
                        tokio::spawn(async move { UnicodeOverlay::copy(s).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { UnicodeOverlay::copy(s).await; }, |_| Message::DismissActiveOverlay);
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
                    return iced::window::get_latest().and_then(iced::window::gain_focus);
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
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.email.update(msg),
            },
            Message::Notif(msg) => match msg {
                NotifMessage::DismissDrawer => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                NotifMessage::ActivateToast(id) => {
                    if let Some(t) = self.notifs.active_toasts.iter().find(|t| t.id == id).cloned() {
                        let app = t.app_name.clone();
                        let entry = t.desktop_entry.clone();
                        let nid = t.id;
                        self.notifs.update(NotifMessage::DismissToast(id));
                        std::thread::spawn(move || {
                            moon_beauty_shell::backend::notif_dbus::emit_action_invoked(nid, "default");
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
                LockMessage::SubmitUnlock => {
                    let pass = self.lockscreen.password_buffer.clone();
                    return Command::perform(async move { LockscreenOverlay::authenticate_pam(&pass).await }, |ok| {
                        if ok { Message::Lock(LockMessage::UnlockSuccess) } else { Message::Lock(LockMessage::UnlockFailed) }
                    });
                }
                LockMessage::UnlockSuccess => {
                    self.lockscreen.update(msg);
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.lockscreen.update(msg),
            },
            Message::DismissActiveOverlay => {
                if self.is_standalone {
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    });
                    return Command::none();
                }
                self.active_overlay = None;
            }
            Message::CloseShell => {
                std::process::exit(0);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let clock_sub = iced::time::every(std::time::Duration::from_millis(1000)).map(|_| Message::TickClock);
        let toast_sub = if self.is_notif_surface && !self.notifs.active_toasts.is_empty() {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::TickToasts)
        } else {
            iced::Subscription::none()
        };
        let hw_sub = iced::time::every(std::time::Duration::from_millis(self.settings.hardware_poll_interval_ms)).map(|_| Message::TickHardware);

        let clip_sub = if matches!(self.active_overlay, Some(OverlayTarget::Clipboard)) {
            iced::keyboard::on_key_press(|key, _mods| match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => Some(Message::Clipboard(ClipboardMessage::SelectPrev)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => Some(Message::Clipboard(ClipboardMessage::SelectNext)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => Some(Message::Clipboard(ClipboardMessage::CopySelected)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete) => Some(Message::Clipboard(ClipboardMessage::DeleteSelected)),
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => Some(Message::Clipboard(ClipboardMessage::DeleteSelected)),
                _ => None,
            })
        } else {
            iced::Subscription::none()
        };

        let text_typing_sub = if self.annotator.text_editing.is_some() {
            iced::keyboard::on_key_press(|key, _mods| {
                match key {
                iced::keyboard::Key::Character(c) => {
                    match c.as_str() {
                        "[" => Some(Message::Annotator(AnnotatorMessage::FontSizeDown)),
                        "]" => Some(Message::Annotator(AnnotatorMessage::FontSizeUp)),
                        _ => Some(Message::Annotator(AnnotatorMessage::CharacterInput(c.to_string()))),
                    }
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Space) => {
                    Some(Message::Annotator(AnnotatorMessage::CharacterInput(" ".into())))
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Backspace) => {
                    Some(Message::Annotator(AnnotatorMessage::BackspacePressed))
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                    if _mods.control() {
                        Some(Message::Annotator(AnnotatorMessage::NewLinePressed))
                    } else {
                        Some(Message::Annotator(AnnotatorMessage::ConfirmText))
                    }
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::F2) => {
                    Some(Message::Annotator(AnnotatorMessage::ConfirmText))
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                    Some(Message::Annotator(AnnotatorMessage::CancelText))
                }
                _ => None,
                }
            })
        } else {
            iced::Subscription::none()
        };

        let esc_sub = if self.annotator.text_editing.is_some() {
            iced::keyboard::on_key_press(|key, _mods| match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                    Some(Message::Annotator(AnnotatorMessage::CancelText))
                }
                _ => None,
            })
        } else {
            iced::keyboard::on_key_press(|key, _modifiers| {
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    Some(Message::DismissActiveOverlay)
                } else {
                    None
                }
            })
        };

        // Enter → save when annotator overlay is active.
        let annotator_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator)) && self.annotator.text_editing.is_none() {
            iced::keyboard::on_key_press(|key, _mods| match key {
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
            })
        } else {
            iced::Subscription::none()
        };

        let enter_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator)) && self.annotator.text_editing.is_none() {
            iced::keyboard::on_key_press(|key, _mods| match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                    Some(Message::Annotator(AnnotatorMessage::Save))
                }
                _ => None,
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
        iced::Subscription::batch(vec![clock_sub, toast_sub, hw_sub, esc_sub,
            enter_sub, ipc_sub, notif_sub, resize_sub, clip_sub, annotator_sub, text_typing_sub])
    }

    fn view(&self) -> Element<'_, Message> {
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
            });

        let overlay_element: Option<Element<'_, Message>> = match &self.active_overlay {
            Some(OverlayTarget::Launcher) => Some(self.launcher.view(&self.theme, &self.settings).map(Message::Launcher)),
            Some(OverlayTarget::Calc) => Some(self.calc.view(&self.theme, &self.settings).map(Message::Calc)),
            Some(OverlayTarget::Clipboard) => Some(self.clipboard.view(&self.theme, &self.settings).map(Message::Clipboard)),
            Some(OverlayTarget::Notes) => Some(self.notes.view(&self.theme).map(Message::Notes)),
            Some(OverlayTarget::Pass) => Some(self.pass.view(&self.theme).map(Message::Pass)),
            Some(OverlayTarget::Power) => Some(self.power.view(&self.theme).map(Message::Power)),
            Some(OverlayTarget::Todo) => Some(self.todo.view(&self.theme).map(Message::Todo)),
            Some(OverlayTarget::Reminders) => Some(self.reminders.view(&self.theme).map(Message::Reminders)),
            Some(OverlayTarget::Dictionary) => Some(self.dict.view(&self.theme).map(Message::Dict)),
            Some(OverlayTarget::Unicode) => Some(self.unicode.view(&self.theme).map(Message::Unicode)),
            Some(OverlayTarget::Web) => Some(self.web.view(&self.theme).map(Message::Web)),
            Some(OverlayTarget::Amogus) => Some(self.amogus.view(&self.theme).map(Message::Amogus)),
            Some(OverlayTarget::Rng) => Some(self.rng.view(&self.theme).map(Message::Rng)),
            Some(OverlayTarget::Email) => Some(self.email.view(&self.theme).map(Message::Email)),
            Some(OverlayTarget::NotificationsHistory) => Some(self.notifs.view_history(&self.theme).map(Message::Notif)),
            Some(OverlayTarget::Settings) => Some(self.settings_ui.view(&self.settings, &self.theme).map(Message::Settings)),
            Some(OverlayTarget::Lockscreen) => Some(self.lockscreen.view(&self.theme).map(Message::Lock)),
            Some(OverlayTarget::Annotator) => Some(self.annotator.view().map(Message::Annotator)),
            None => None,
        };

        if self.is_standalone {
            if let Some(overlay) = overlay_element {
                return Stack::new().push(overlay).push(notifs_view).into();
            } else {
                std::process::exit(0);
            }
        }

        let bar_view = TopBar::view(&self.settings, &self.theme, &self.telemetry, false, &[]).map(Message::TopBar);
        let base_content = Stack::new().push(bar_view);

        if let Some(overlay) = overlay_element {
            Stack::new().push(base_content).push(overlay).into()
        } else {
            base_content.into()
        }
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}


fn capsule_min_width(id: &str) -> f32 {
    match id {
        "calendar" => 110.0,
        "music" => 140.0,
        "alarm" => 90.0,
        "weather" => 95.0,
        "unified" => 80.0,
        "notify" => 120.0,
        "clock" => 85.0,
        "audio" => 95.0,
        "mic" => 80.0,
        "net" => 145.0,
        "cpu" => 115.0,
        "gpu" => 150.0,
        "ram" => 105.0,
        "battery" => 85.0,
        _ => 80.0,
    }
}

fn get_capsule_widths(s: &ShellSettings, list: &[String]) -> f32 {
    list.iter()
        .filter(|id| s.visible_capsules.get(*id).copied().unwrap_or(true))
        .map(|id| capsule_min_width(id) + s.capsule_spacing as f32)
        .sum()
}

fn capsule_actual_width(id: &str) -> f32 {
    moon_beauty_shell::bar::capsules::lookup_width(id).unwrap_or_else(|| capsule_min_width(id))
}

fn row_width(items: &[&String], sp: f32) -> f32 {
    let n = items.len();
    items.iter().enumerate().map(|(i, id)| {
        let w = capsule_actual_width(id.as_str());
        if i + 1 < n { w + sp } else { w }
    }).sum()
}

fn resolve_primary_output() -> Option<String> {
    let out = std::process::Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"]).output().ok()?;
    if !out.status.success() { return None; }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let arr = v.as_array()?;
    // Prefer the output marked "primary"
    for o in arr {
        if o.get("primary").and_then(|p| p.as_bool()) == Some(true) {
            if let Some(name) = o.get("name").and_then(|n| n.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    // Fallback: focused
    for o in arr {
        if o.get("focused").and_then(|f| f.as_bool()) == Some(true) {
            if let Some(name) = o.get("name").and_then(|n| n.as_str()) {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn capsule_center_x(s: &ShellSettings, sw: u32, target: &str) -> i32 {
    let sw_f = sw as f32;
    let sp = s.capsule_spacing as f32;
    let pad = 16.0;

    let visible = |id: &String| s.visible_capsules.get(id).copied().unwrap_or(true);
    let left_items: Vec<&String> = s.bar_left.iter().filter(|id| visible(id)).collect();
    let center_items: Vec<&String> = s.bar_center.iter().filter(|id| visible(id)).collect();
    let right_items: Vec<&String> = s.bar_right.iter().filter(|id| visible(id)).collect();

    // 1. bar_left
    let mut x = pad;
    for (i, id) in left_items.iter().enumerate() {
        let w = capsule_actual_width(id.as_str());
        if id.as_str() == target { return (x + w / 2.0) as i32; }
        x += w + if i + 1 < left_items.len() { sp } else { 0.0 };
    }

    // 2. bar_center — account for asymmetric left/right widths
    let left_w = row_width(&left_items, sp);
    let right_w = row_width(&right_items, sp);
    let center_w = row_width(&center_items, sp);
    let avail = sw_f - 2.0 * pad;
    let fill_w = ((avail - left_w - center_w - right_w) / 2.0).max(0.0);
    let mut x = pad + left_w + fill_w;
    for (i, id) in center_items.iter().enumerate() {
        let w = capsule_actual_width(id.as_str());
        if id.as_str() == target { return (x + w / 2.0) as i32; }
        x += w + if i + 1 < center_items.len() { sp } else { 0.0 };
    }

    // 3. bar_right
    let mut x = pad + avail - right_w;
    for (i, id) in right_items.iter().enumerate() {
        let w = capsule_actual_width(id.as_str());
        if id.as_str() == target { return (x + w / 2.0) as i32; }
        x += w + if i + 1 < right_items.len() { sp } else { 0.0 };
    }

    200
}

fn get_screen_width_for(output: Option<&str>) -> u32 {
    if let Ok(o) = std::process::Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"]).output()
    {
        if o.status.success() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                if let Some(arr) = v.as_array() {
                    if let Some(name) = output {
                        for out in arr {
                            if out.get("name").and_then(|n| n.as_str()) == Some(name) {
                                if let Some(w) = out.get("rect").and_then(|r| r.get("width")).and_then(|w| w.as_u64()) {
                                    return if w > 0 { w as u32 } else { 1920 };
                                }
                            }
                        }
                    }
                    for out in arr {
                        if out.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                            if let Some(w) = out.get("rect").and_then(|r| r.get("width")).and_then(|w| w.as_u64()) {
                                return if w > 0 { w as u32 } else { 1920 };
                            }
                        }
                    }
                }
            }
        }
    }
    1920
}

fn get_screen_width() -> u32 {
    if let Ok(o) = std::process::Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"]).output()
    {
        if o.status.success() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                if let Some(arr) = v.as_array() {
                    for output in arr {
                        if output.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                            if let Some(w) = output.get("rect")
                                .and_then(|r| r.get("width"))
                                .and_then(|w| w.as_u64())
                            {
                                return if w > 0 { w as u32 } else { 1920 };
                            }
                        }
                    }
                }
            }
        }
    }
    if let Ok(o) = std::process::Command::new("hyprctl")
        .args(["monitors", "-j"]).output()
    {
        if o.status.success() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                if let Some(arr) = v.as_array() {
                    for mon in arr {
                        if mon.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                            if let Some(w) = mon.get("width").and_then(|w| w.as_u64()) {
                                return if w > 0 { w as u32 } else { 1920 };
                            }
                        }
                    }
                }
            }
        }
    }
    1920
}
