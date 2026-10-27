//! Process utilities.

use std::path::PathBuf;

/// Resolve the current executable path, stripping the " (deleted)" suffix
/// that `/proc/PID/exe` appends after `cargo build` replaces a running binary.
pub fn resolve_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            if let Some(stripped) = s.strip_suffix(" (deleted)") {
                PathBuf::from(stripped)
            } else {
                p
            }
        })
        .unwrap_or_else(|| PathBuf::from("moon-beauty-shell"))
}
