//! Tooltip child process spawning.
//!
//! `Message::TopBar` gathers the params for a tooltip (position, size,
//! direction, output) and hands them to `spawn()`, which builds the
//! `Command` with all the env vars and spawns the popup process.
//!
//! Extracted from `main.rs` so the spawn logic isn't tangled up with
//! the bar's event dispatch.

use std::io;
use std::path::PathBuf;
use std::process::Child;

use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::ShellSettings;

/// All data needed to spawn a tooltip popup.
pub struct SpawnRequest<'a> {
    pub id: &'a str,
    pub exe: PathBuf,
    pub output: Option<&'a str>,
    pub tooltip_w: u32,
    pub tooltip_h: u32,
    pub drop_ms: u32,
    pub expand_ms: u32,
    pub capsule_cx: i32,
    pub capsule_w: f32,
    pub screen_w: u32,
    pub slant_l: &'a str,
    pub slant_r: &'a str,
    pub grow_right: bool,
}

pub fn spawn(req: &SpawnRequest) -> io::Result<Child> {
    let mut cmd = std::process::Command::new(&req.exe);

    if req.id == "weather" {
        cmd.arg("--weather-tooltip");
    } else if matches!(req.id, "calendar" | "music" | "unified" | "net" | "custom" | "clock") {
        cmd.arg("--info-tooltip").env("MBS_INFO_MODE", req.id);
    } else {
        cmd.arg("--sys-tooltip").env("MBS_SYS_TARGET", req.id);
    }

    if let Some(out) = req.output {
        cmd.env("MBS_OUTPUT", out);
    }

    cmd.env("MBS_TOOLTIP_W", req.tooltip_w.max(req.capsule_w as u32).to_string())
       .env("MBS_TOOLTIP_H", req.tooltip_h.to_string())
       .env("MBS_TOOLTIP_DROP_MS", req.drop_ms.to_string())
       .env("MBS_TOOLTIP_EXPAND_MS", req.expand_ms.to_string())
       .env("MBS_SCREEN_W", req.screen_w.to_string())
       .env("MBS_SLANT_L", req.slant_l)
       .env("MBS_SLANT_R", req.slant_r)
       .env("MBS_GROW_RIGHT", if req.grow_right { "1" } else { "0" })
       .env("MBS_CAPSULE_CX", req.capsule_cx.to_string())
       .env("MBS_CAPSULE_W", req.capsule_w.to_string());

    eprintln!(
        "[spawn] id={:?} w={} h={} cx={} cap_w={:.1} sw={} L={} R={} grow_right={} out={:?}",
        req.id, req.tooltip_w, req.tooltip_h, req.capsule_cx, req.capsule_w,
        req.screen_w, req.slant_l, req.slant_r, req.grow_right, req.output
    );

    cmd.spawn()
}

/// Compute the popup height for a given tooltip id.
///
/// Priority:
///   1. If `settings.tooltip_sizes` contains an entry for this id, return
///      its height. This is the value produced by dragging the resize box
///      in Settings → Bar Layout. Consulted here at spawn time because
///      the popup process reads only MBS_TOOLTIP_H from env and does not
///      reload settings itself.
///   2. Otherwise, derive height from current telemetry content size
///      (weather forecast line count, CPU/GPU/RAM process count, etc.).
pub fn compute_height(
    id: &str,
    settings: &ShellSettings,
    telemetry: &UnifiedTelemetry,
) -> u32 {
    // User-dragged override wins.
    if let Some((w, h)) = settings.tooltip_sizes.get(id) {
        eprintln!(
            "[compute-h] id={} EXPLICIT OVERRIDE {}x{} (from tooltip_sizes)",
            id, w, h
        );
        return *h;
    }

    // No override: derive from content.
    let bar_h = settings.bar_height;
    let content_h: u32 = match id {
        "weather" => {
            let lines = telemetry.weather.as_ref()
                .map(|w| w.tooltip_lines.len().min(15) as u32)
                .unwrap_or(1);
            lines * 24 + 40
        }
        "calendar" => {
            use chrono::Datelike;
            let now = chrono::Local::now();
            let first = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
                .unwrap_or_default();
            let (ny, nm) = if now.month() == 12 { (now.year() + 1, 1) } else { (now.year(), now.month() + 1) };
            let next = chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap_or_default();
            let days = next.signed_duration_since(first).num_days() as u32;
            let offset = first.weekday().num_days_from_sunday();
            let weeks = (offset + days).div_ceil(7);
            40 + 36 + 24 + weeks * 50 + 20
        }
        "cpu" => telemetry.cpu.top_procs.len().min(10) as u32 * 24 + 100,
        "gpu" => telemetry.gpu.top_procs.len().min(10) as u32 * 24 + 100,
        "ram" => telemetry.ram.top_procs.len().min(10) as u32 * 24 + 100,
        "net" => 450,
        "music" => 400,
        "unified" => 400,
        "custom" => 300,
        "clock" => 320,
        _ => 400,
    };
    let total = content_h + bar_h + 20;
    eprintln!("[compute-h] id={} DEFAULT content_h={} bar_h={} total={} (no override)", id, content_h, bar_h, total);
    total
}
