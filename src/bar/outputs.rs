//! Wayland output lookup — resolve primary/focused output and screen width.

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
