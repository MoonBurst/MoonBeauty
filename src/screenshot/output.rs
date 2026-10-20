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
