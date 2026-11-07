use moonbeauty::libraries::ipc::{IpcCommand, OverlayTarget};
use moonbeauty::libraries::process::resolve_exe;
use moonbeauty::overlays::screenshot;

use crate::send_ipc_sync;

pub fn run_fragment(args: &[String]) {
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
}

pub fn run_orchestrator(args: &[String]) {
    // Orchestrator: capture focused output → spawn child → report crop
    if args.iter().any(|a| a == "--screenshot") {
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

        let state_file = moonbeauty::libraries::process::mbs_unique_tmp("mbs-freeze-state", "txt");
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
            let write_ok = match moonbeauty::libraries::process::private_create_new(&local_png) {
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
        let out_path = moonbeauty::libraries::process::mbs_unique_tmp("mbs-freeze-out", "png");
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
        std::process::exit(0);
    }
}
