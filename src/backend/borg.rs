use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct BorgStatus {
    pub state_label: String,
    pub is_active: bool,
    pub is_mounted: bool,
    pub is_failed: bool,
    pub percent: u8,
    pub speed: String,
    pub eta: String,
    pub details: Vec<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct RawBorgPayload {
    status: Option<String>,
    percent: Option<u8>,
    archive_name: Option<String>,
    speed: Option<String>,
    eta: Option<String>,
    processed_files: Option<u64>,
    total_files: Option<u64>,
}

pub struct BorgBackend;

impl BorgBackend {
    pub async fn poll() -> BorgStatus {
        let mut payload_raw = None;
        let candidates = [
            PathBuf::from("/dev/shm/borg-offsite-status.json"),
            dirs::runtime_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("borg-offsite-status.json"),
        ];

        for path in &candidates {
            if let Ok(content) = fs::read_to_string(path) {
                if !content.trim().is_empty() {
                    payload_raw = Some(content);
                    break;
                }
            }
        }

        let is_mounted = fs::read_to_string("/proc/mounts")
            .map(|s| s.contains("/tmp/borg-mount") || s.contains("/mnt/main_backup"))
            .unwrap_or(false);

        let Some(raw) = payload_raw else {
            return BorgStatus {
                state_label: if is_mounted { "Mounted".into() } else { "Idle".into() },
                is_active: false,
                is_mounted,
                is_failed: false,
                percent: 0,
                speed: "".into(),
                eta: "".into(),
                details: vec![if is_mounted {
                    "Archive Mounted at /tmp/borg-mount".into()
                } else {
                    "Standing by (All repositories synced)".into()
                }],
            };
        };

        let parsed: RawBorgPayload = serde_json::from_str(&raw).unwrap_or(RawBorgPayload {
            status: Some("idle".into()),
            percent: None,
            archive_name: None,
            speed: None,
            eta: None,
            processed_files: None,
            total_files: None,
        });

        let st = parsed.status.unwrap_or_else(|| "idle".into());
        let pct = parsed.percent.unwrap_or(0);
        let speed = parsed.speed.unwrap_or_default();
        let eta = parsed.eta.unwrap_or_default();

        let mut details = Vec::new();
        let is_active = st == "running" || st == "syncing" || st == "indexing";
        let is_error_state = st == "error" || st == "failed" || st == "fail";

        match st.as_str() {
            "indexing" => {
                details.push("Indexing repository files...".into());
            }
            "running" => {
                details.push(format!("Active Backup Progress: {}%", pct));
                if let (Some(done), Some(tot)) = (parsed.processed_files, parsed.total_files) {
                    details.push(format!("Files: {} / {}", done, tot));
                }
            }
            "syncing" => {
                details.push(format!("Syncing to WebDAV: {}%", pct));
            }
            "error" | "failed" | "fail" => {
                let msg = if speed.is_empty() { "Unknown error".to_string() } else { speed.clone() };
                details.push(format!("Error: {}", msg));
            }
            _ => {
                details.push("Repository Idle".into());
            }
        }

        if !speed.is_empty() && !is_error_state {
            details.push(format!("Transfer Speed: {}", speed));
        }
        if !eta.is_empty() && !is_error_state {
            details.push(format!("ETA: {}", eta));
        }

        // Failed if explicit error state OR any detail mentions error/fail
        let is_failed = is_error_state || details.iter().any(|d| {
            let low = d.to_lowercase();
            low.contains("error") || low.contains("failed") || low.contains("fail")
        });

        let state_label = if is_failed {
            "Borg: Failed".into()
        } else if is_active {
            if pct > 0 {
                format!("Borg: {}%", pct)
            } else {
                "Borg: Active".into()
            }
        } else if is_mounted {
            "Borg: Mounted".into()
        } else {
            "Borg: Idle".into()
        };

        BorgStatus {
            state_label,
            is_active,
            is_mounted,
            is_failed,
            percent: pct,
            speed,
            eta,
            details,
        }
    }

    pub async fn toggle_backup() {
        let _ = Command::new("sudo")
            .arg("/run/current-system/sw/bin/borg-backup-start")
            .spawn();
    }

    pub async fn toggle_mount() {
        let is_mounted = fs::read_to_string("/proc/mounts")
            .map(|s| s.contains("/tmp/borg-mount"))
            .unwrap_or(false);

        let script = if is_mounted {
            "/run/current-system/sw/bin/borg-umount-browser"
        } else {
            "/run/current-system/sw/bin/borg-mount-browser"
        };

        let _ = Command::new("sudo").arg(script).spawn();
    }
}
