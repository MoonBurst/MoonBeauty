//! Bar layout geometry — capsule positions and widths.
//!
//! These functions translate `ShellSettings` into pixel coordinates for
//! capsule centers. They're used by the `Message::TopBar` handler to
//! anchor tooltips under the correct capsule.

use crate::libraries::config::ShellSettings;

pub fn capsule_min_width(id: &str) -> f32 {
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

pub fn capsule_actual_width(id: &str) -> f32 {
    crate::bar::capsules::lookup_width(id).unwrap_or_else(|| capsule_min_width(id))
}

pub fn row_width(items: &[&String], sp: f32, settings: &ShellSettings) -> f32 {
    let n = items.len();
    items
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let w = if id.as_str() == "tray" {
                settings.tray_width as f32
            } else {
                capsule_actual_width(id.as_str())
            };
            if i + 1 < n {
                w + sp
            } else {
                w
            }
        })
        .sum()
}

pub fn capsule_center_x(s: &ShellSettings, sw: u32, target: &str) -> i32 {
    let sw_f = sw as f32;
    let sp = s.capsule_spacing as f32;
    let pad = 16.0;

    let visible = |id: &String| s.visible_capsules.get(id).copied().unwrap_or(true);
    let left_items: Vec<&String> = s.bar_left.iter().filter(|id| visible(id)).collect();
    let center_items: Vec<&String> = s.bar_center.iter().filter(|id| visible(id)).collect();
    let right_items: Vec<&String> = s.bar_right.iter().filter(|id| visible(id)).collect();

    let w_of = |id: &str| -> f32 {
        if id == "tray" {
            s.tray_width as f32
        } else {
            capsule_actual_width(id)
        }
    };

    // 1. bar_left
    let mut x = pad;
    for (i, id) in left_items.iter().enumerate() {
        let w = w_of(id.as_str());
        if id.as_str() == target {
            return (x + w / 2.0) as i32;
        }
        x += w + if i + 1 < left_items.len() { sp } else { 0.0 };
    }

    // 2. bar_center
    let left_w = row_width(&left_items, sp, s);
    let right_w = row_width(&right_items, sp, s);
    let center_w = row_width(&center_items, sp, s);
    let avail = sw_f - 2.0 * pad;
    let fill_w = ((avail - left_w - center_w - right_w) / 2.0).max(0.0);
    let mut x = pad + left_w + fill_w;
    for (i, id) in center_items.iter().enumerate() {
        let w = w_of(id.as_str());
        if id.as_str() == target {
            return (x + w / 2.0) as i32;
        }
        x += w + if i + 1 < center_items.len() { sp } else { 0.0 };
    }

    // 3. bar_right
    let mut x = pad + avail - right_w;
    for (i, id) in right_items.iter().enumerate() {
        let w = w_of(id.as_str());
        if id.as_str() == target {
            return (x + w / 2.0) as i32;
        }
        x += w + if i + 1 < right_items.len() { sp } else { 0.0 };
    }

    200
}
