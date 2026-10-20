use std::process::Command;

pub fn focused_output() -> Option<String> {
    let out = Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.as_array()?
        .iter()
        .find(|o| o.get("focused").and_then(|f| f.as_bool()) == Some(true))
        .and_then(|o| o.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from)
}


pub fn list_all_outputs() -> Vec<(String, i32, i32, u32, u32)> {
    let out = Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None });
    let Some(text) = out else { return Vec::new(); };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new(); };
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|o| {
                    let name = o.get("name")?.as_str()?.to_string();
                    let rect = o.get("rect")?;
                    let x = rect.get("x")?.as_i64()? as i32;
                    let y = rect.get("y")?.as_i64()? as i32;
                    let w = rect.get("width")?.as_i64()? as u32;
                    let h = rect.get("height")?.as_i64()? as u32;
                    Some((name, x, y, w, h))
                })
                .collect()
        })
        .unwrap_or_default()
}
