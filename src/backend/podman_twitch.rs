use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatcherDetails {
    pub name: String,
    pub is_running: bool,
    pub streamer: String,
    pub drops: String,
    pub game: String,
    pub is_limited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub name: String,
    pub status: String,
    pub is_running: bool,
    pub is_watcher: bool,
    pub purpose: String,
}

#[derive(Debug, Clone, Default)]
pub struct PodmanStatus {
    pub is_ok: bool,
    pub is_rate_limited: bool,
    pub watchers: Vec<WatcherDetails>,
    pub containers: Vec<ContainerInfo>,
    pub summary_label: String,
}

pub struct PodmanBackend;

impl PodmanBackend {
    pub async fn poll() -> PodmanStatus {
        let engine = if Command::new("podman").arg("version").output().await.is_ok() {
            "podman"
        } else {
            "docker"
        };

        let Ok(output) = Command::new(engine)
            .args(["ps", "-a", "--format", "{{.Names}}\t{{.Status}}\t{{.Image}}\t{{.State}}"])
            .output()
            .await
        else {
            return PodmanStatus {
                summary_label: "Podman Offline".into(),
                ..Default::default()
            };
        };

        let raw = String::from_utf8_lossy(&output.stdout);
        let mut containers = Vec::new();
        let mut watchers = Vec::new();
        let mut any_limited = false;

        for line in raw.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.is_empty() || parts[0].trim().is_empty() {
                continue;
            }

            let name = parts[0].trim().to_string();
            let status_raw = parts.get(1).copied().unwrap_or("Unknown");
            let image = parts.get(2).copied().unwrap_or("");
            let state = parts.get(3).copied().unwrap_or("").to_lowercase();
            let is_running = state == "running" || status_raw.to_lowercase().starts_with("up");

            let is_watcher = name.contains("twitch") || name.contains("miner") || name.contains("berry");
            let mut watcher_details = WatcherDetails {
                name: name.clone(),
                is_running,
                ..Default::default()
            };

            if is_watcher && is_running {
                let (streamer, drops, game, limited) = Self::parse_logs(engine, &name).await;
                watcher_details.streamer = streamer;
                watcher_details.drops = drops;
                watcher_details.game = game;
                watcher_details.is_limited = limited;
                if limited {
                    any_limited = true;
                }
                watchers.push(watcher_details);
            }

            let purpose = Self::determine_purpose(&name, image);
            containers.push(ContainerInfo {
                name,
                status: status_raw.to_string(),
                is_running,
                is_watcher,
                purpose,
            });
        }

        let summary_label = if any_limited {
            "Miner: Limited!".to_string()
        } else if let Some(w) = watchers.iter().find(|w| w.is_running && !w.streamer.is_empty()) {
            format!("📺 {}", w.streamer)
        } else {
            let active_count = containers.iter().filter(|c| c.is_running).count();
            if active_count > 0 {
                format!("{} Active", active_count)
            } else {
                "Idle".into()
            }
        };

        PodmanStatus {
            is_ok: !containers.is_empty(),
            is_rate_limited: any_limited,
            watchers,
            containers,
            summary_label,
        }
    }

    async fn parse_logs(engine: &str, container_name: &str) -> (String, String, String, bool) {
        let Ok(out) = Command::new(engine)
            .args(["logs", "--tail", "60", container_name])
            .output()
            .await
        else {
            return (String::new(), String::new(), String::new(), false);
        };

        let logs = String::from_utf8_lossy(&out.stdout);
        let mut streamer = String::new();
        let mut drops = String::new();
        let mut game = String::new();
        let is_limited = logs.contains("429") || logs.contains("rate limit") || logs.contains("too many requests");

        for line in logs.lines().rev() {
            if streamer.is_empty() {
                if let Some(pos) = line.find("Watching:") {
                    streamer = line[pos + 9..].split_whitespace().next().unwrap_or("").to_string();
                } else if let Some(pos) = line.find("Channel:") {
                    streamer = line[pos + 8..].split_whitespace().next().unwrap_or("").to_string();
                }
            }

            if drops.is_empty() && line.contains("drop") {
                if let Some(pos) = line.find("Drop progress:") {
                    drops = line[pos..].trim().to_string();
                }
            }

            if game.is_empty() && line.contains("game:") {
                if let Some(start) = line.find("game:") {
                    let rest = &line[start + 5..];
                    if let Some(end) = rest.find(')') {
                        game = rest[..end].trim().to_string();
                    }
                }
            }

            if !streamer.is_empty() && !drops.is_empty() {
                break;
            }
        }

        (streamer, drops, game, is_limited)
    }

    fn determine_purpose(name: &str, img: &str) -> String {
        let low = format!("{} {}", name, img).to_lowercase();
        if low.contains("flaresolverr") {
            "Cloudflare Proxy (:8191)".into()
        } else if low.contains("twitch") || low.contains("miner") {
            "Twitch Drops Watcher".into()
        } else if low.contains("jellyfin") {
            "Media Server (:8096)".into()
        } else if low.contains("vaultwarden") {
            "Bitwarden Vault".into()
        } else {
            "Background Service".into()
        }
    }

    pub async fn restart(container: &str) {
        let _ = Command::new("podman")
            .args(["restart", container])
            .status()
            .await;
    }

    pub async fn restart_all() {
        let _ = Command::new("podman")
            .args(["restart", "twitch-miner", "twitchminer-berrydrop", "flaresolverr"])
            .status()
            .await;
    }

    pub async fn open_web_portal(port: u16) {
        let url = format!("http://localhost:{}", port);
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}
