//! Tooltip child process spawning.
//!
//! Height calculation, offset bias env helpers, and Wayland output lookup.

use crate::libraries::backend::UnifiedTelemetry;
use crate::libraries::config::ShellSettings;

/// Read per-capsule offset bias from env, added on top of global offsets.
pub fn read_offset_extra() -> (i32, i32) {
    let read = |key: &str| -> i32 {
        std::env::var(key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };
    (
        read("MBS_TOOLTIP_OFFSET_LEFT_EXTRA"),
        read("MBS_TOOLTIP_OFFSET_RIGHT_EXTRA"),
    )
}

/// Compute the popup height for a given tooltip id.
///
/// Priority:
///   1. If `settings.tooltip_sizes` contains an entry for this id, return
///      its height (produced by dragging the resize box in Settings → Bar Layout).
///   2. Otherwise, derive height from current telemetry content size
///      (weather forecast line count, CPU/GPU/RAM process count, etc.).
pub fn compute_height(id: &str, settings: &ShellSettings, telemetry: &UnifiedTelemetry) -> u32 {
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
            let lines = telemetry
                .weather
                .as_ref()
                .map(|w| w.tooltip_lines.len().min(15) as u32)
                .unwrap_or(1);
            lines * 24 + 40
        }
        "calendar" => {
            use chrono::Datelike;
            let now = chrono::Local::now();
            let first =
                chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap_or_default();
            let (ny, nm) = if now.month() == 12 {
                (now.year() + 1, 1)
            } else {
                (now.year(), now.month() + 1)
            };
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
        "music" => 520,
        "unified" => 400,
        "custom" => 300,
        "clock" => 320,
        _ => 400,
    };
    let total = content_h + bar_h + 20;
    eprintln!(
        "[compute-h] id={} DEFAULT content_h={} bar_h={} total={} (no override)",
        id, content_h, bar_h, total
    );
    total
}

// Wayland output lookup — resolve primary/focused output and screen width.

pub fn resolve_primary_output() -> Option<String> {
    let out = std::process::Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let arr = v.as_array()?;
    // Prefer "primary"
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

pub fn get_screen_width_for(output: Option<&str>) -> u32 {
    if let Ok(o) = std::process::Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"])
        .output()
    {
        if o.status.success() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                if let Some(arr) = v.as_array() {
                    if let Some(name) = output {
                        for out in arr {
                            if out.get("name").and_then(|n| n.as_str()) == Some(name) {
                                if let Some(w) = out
                                    .get("rect")
                                    .and_then(|r| r.get("width"))
                                    .and_then(|w| w.as_u64())
                                {
                                    return if w > 0 { w as u32 } else { 1920 };
                                }
                            }
                        }
                    }
                    for out in arr {
                        if out.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                            if let Some(w) = out
                                .get("rect")
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
    1920
}
