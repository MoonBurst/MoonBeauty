use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use moonbeauty::libraries::config::ShellSettings;
use moonbeauty::libraries::ipc::{IpcCommand, OverlayTarget};
use moonbeauty::libraries::process::resolve_exe;

mod app;
mod cli;

use app::MoonBeautyShellApp;

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
pub(crate) fn send_ipc_sync(cmd: &IpcCommand) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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

    cli::clipboard_ingest::run(&args);

    // Finding 07 defense-in-depth: restrictive ImageMagick policy that
    // every child process (annotator, watermark, reveal) inherits via
    // MAGICK_CONFIGURE_PATH.
    if let Some(dir) = moonbeauty::libraries::process::ensure_imagemagick_policy() {
        std::env::set_var("MAGICK_CONFIGURE_PATH", &dir);
    }

    cli::test_lock_auth::run(&args);

    cli::test_email::run(&args);

    cli::lockscreen_coordinator::run(&args);

    cli::notify_flags::run(&args);

    cli::music_flags::run(&args);

    // Restart bar (used by Settings when changing monitor)
    cli::restart::run(&args);
    cli::toggle::run(&args);

    // ── Screenshot CLI ────────────────────────────────────────────────
    // --screenshot [--annotate] [--watermark "name"]
    // MUST be above the notif-surface spawn block, or the flag falls
    // through and boots the full bar.
    cli::screenshot::run_fragment(&args);

    cli::screenshot::run_orchestrator(&args);


    cli::tray::run(&args);

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
    // Tighten user-content file permissions once per process. Fixes
    // pre-existing 0644 files written by earlier builds; new writes go
    // through private_write_file at 0600.
    moonbeauty::libraries::process::tighten_user_state_files();

    let settings = ShellSettings::load();
    // Eager spawn. D-Bus activation cold-start is 800-1500ms, which is
    // slower than the round-trip from the originating application in most
    // cases — notifications arrive AFTER the user has read the message.
    // 106 MB RSS is the cost of working notifications.
    //
    // Reverted from the lazy-activation experiment (2026-11-08). The
    // D-Bus service file is kept as a crash fallback: if notif-surface
    // crashes, D-Bus reactivates it on the next Notify() call.
    if !is_notif_surface && standalone_target.is_none() {
        let _ = std::process::Command::new("pkill")
            .args([
                "-f",
                "--",
                "(^|/)(moonbeauty|moon-beauty-shell) --notif-surface$",
            ])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let exe = resolve_exe();
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--notif-surface");
        let target = &settings.notif_screen_name;
        if !target.is_empty() && target != "Auto" && target != "Auto (Primary)" {
            cmd.env("MBS_OUTPUT", target);
        }
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
        // Each lock child is bound to a specific output. Only the primary
        // child gets KeyboardInteractivity::Exclusive — Wayland allows
        // only one Exclusive layer surface per seat, and three competing
        // Exclusive surfaces let sway rotate focus mid-typing, dropping
        // characters from the coordinator's buffer.
        let lock_output = std::env::var("MBS_LOCK_OUTPUT")
            .ok()
            .filter(|s| !s.is_empty())
            .or(binded_output_name.clone());
        let is_primary = std::env::var("MBS_LOCK_PRIMARY")
            .map(|s| s == "1")
            .unwrap_or(false);
        LayerShellSettings {
            size: None,
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: if is_primary {
                KeyboardInteractivity::Exclusive
            } else {
                KeyboardInteractivity::None
            },
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

    cli::tooltips::run(&args);

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

