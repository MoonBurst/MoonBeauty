//! Freeze-frame capture via grim.

pub fn capture_fullscreen() -> Result<Vec<u8>, String> {
    capture_output(None)
}

pub fn capture_output(name: Option<&str>) -> Result<Vec<u8>, String> {
    let mut cmd = std::process::Command::new("grim");
    if let Some(n) = name {
        cmd.args(["-o", n]);
    }
    cmd.arg("-");
    let output = cmd.output().map_err(|e| format!("grim spawn: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("grim failed: {}", err.trim()));
    }
    Ok(output.stdout)
}
