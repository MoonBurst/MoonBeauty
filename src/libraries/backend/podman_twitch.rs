use bollard::Docker;
use bollard::container::ListContainersOptions;
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


/// Connect to ALL available container engine sockets. Both rootless and
/// rootful podman can hold different containers, and users often mix them.
fn connect_engines() -> Vec<Docker> {
    let mut engines: Vec<Docker> = Vec::new();
    let mut paths: Vec<String> = Vec::new();

    // 1. Rootless (most reliable)
    // Get UID without needing libc: metadata on /proc/self returns
    // the process owner's UID via std::os::unix::fs::MetadataExt.
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or_else(|_| {
            // Fallback: bash/zsh usually export UID
            std::env::var("UID")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1000)
        });
    paths.push(format!("/run/user/{}/podman/podman.sock", uid));
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        paths.push(format!("{}/podman/podman.sock", rt));
        paths.push(format!("{}/docker.sock", rt));
    }
    
    // 2. Proxy socket (if user set it up)
    paths.push("/run/podman/podman-user.sock".into());
    
    // 3. Rootful fallbacks
    paths.push("/run/podman/podman.sock".into());
    paths.push("/var/run/docker.sock".into());

    for path in &paths {
        if std::path::Path::new(path).exists() {
            match Docker::connect_with_unix(path, 120, bollard::API_DEFAULT_VERSION) {
                Ok(d) => {
                    crate::dbg_log!("[bollard] connected {}", path);
                    engines.push(d);
                }
                Err(e) => {
                    crate::dbg_log!("[bollard] {} exists but connect failed: {}", path, e);
                }
            }
        } else {
            crate::dbg_log!("[bollard] no socket at {}", path);
        }
    }
    
    if engines.is_empty() {
        crate::dbg_log!("[bollard] no accessible sockets found");
    }
    engines
}

pub struct PodmanBackend;

impl PodmanBackend {
    pub async fn poll() -> PodmanStatus {
        let engines = connect_engines();
        if engines.is_empty() {
            return PodmanStatus {
                summary_label: "Podman/Docker Offline".into(),
                ..Default::default()
            };
        }

        // Fetch from all engines, merge by name (later engines don't shadow earlier)
        let options = Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        });

        let mut containers_raw = Vec::new();
        let mut seen_names = std::collections::HashSet::new();
        let mut container_to_engine: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for (engine_idx, engine) in engines.iter().enumerate() {
            match engine.list_containers(options.clone()).await {
                Ok(list) => {
                    for c in list {
                        let name = c
                            .names
                            .as_ref()
                            .and_then(|n| n.first())
                            .map(|n| n.trim_start_matches('/').to_string())
                            .unwrap_or_default();
                        if name.is_empty() { continue; }
                        if seen_names.insert(name.clone()) {
                            container_to_engine.insert(name, engine_idx);
                            containers_raw.push(c);
                        }
                    }
                }
                Err(e) => crate::dbg_log!("[bollard] engine {} list failed: {}", engine_idx, e),
            }
        }
        eprintln!(
            "[bollard] merged {} containers from {} engines",
            containers_raw.len(),
            engines.len()
        );

        let mut containers = Vec::new();
        let mut watchers = Vec::new();
        let mut any_limited = false;

        for c in containers_raw {
            let name = c.names.as_ref()
                .and_then(|n| n.first())
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_default();
            let status_raw = c.status.unwrap_or_else(|| "Unknown".into());
            let state = c.state.unwrap_or_else(|| "".into()).to_lowercase();
            let is_running = state == "running" || status_raw.to_lowercase().starts_with("up");

            let is_watcher = name.contains("twitch") || name.contains("miner") || name.contains("berry");
            let mut watcher_details = WatcherDetails {
                name: name.clone(),
                is_running,
                ..Default::default()
            };

            if is_watcher && is_running {
                let engine_idx = container_to_engine.get(&name).copied().unwrap_or(0);
                let engine = &engines[engine_idx];
                let (streamer, drops, game, limited) = Self::parse_logs_async(engine, &name).await;
                watcher_details.streamer = streamer;
                watcher_details.drops = drops;
                watcher_details.game = game;
                watcher_details.is_limited = limited;
                if limited { any_limited = true; }
                watchers.push(watcher_details);
            }

            let purpose = Self::determine_purpose(&name, &c.image.unwrap_or_default());
            containers.push(ContainerInfo {
                name,
                status: status_raw,
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
            if active_count > 0 { format!("{} Active", active_count) } else { "Idle".into() }
        };

        PodmanStatus {
            is_ok: !containers.is_empty(),
            is_rate_limited: any_limited,
            watchers,
            containers,
            summary_label,
        }
    }

    async fn parse_logs_async(docker: &Docker, container: &str) -> (String, String, String, bool) {
        let options = Some(bollard::container::LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: "60".into(),
            ..Default::default()
        });

        let mut stream = docker.logs(container, options);
        let mut full_output = String::new();
        use futures_util::StreamExt;
        while let Some(Ok(chunk)) = stream.next().await {
            full_output.push_str(&String::from_utf8_lossy(&chunk.into_bytes()));
        }

        let logs = full_output;
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
            if !streamer.is_empty() && !drops.is_empty() { break; }
        }

        (streamer, drops, game, is_limited)
    }

    fn determine_purpose(name: &str, img: &str) -> String {
        let low = format!("{} {}", name, img).to_lowercase();
        if low.contains("flaresolverr") { "Cloudflare Proxy (:8191)".into() }
        else if low.contains("twitch") || low.contains("miner") { "Twitch Drops Watcher".into() }
        else if low.contains("jellyfin") { "Media Server (:8096)".into() }
        else if low.contains("vaultwarden") { "Bitwarden Vault".into() }
        else { "Background Service".into() }
    }

    pub async fn restart(container: &str) {
        for engine in connect_engines() {
            if engine.restart_container(container, None).await.is_ok() {
                return;
            }
        }
    }

    pub async fn restart_all() {
        let engines = connect_engines();
        for target in ["twitch-miner", "twitchminer-berrydrop", "flaresolverr"] {
            for engine in &engines {
                if engine.restart_container(target, None).await.is_ok() {
                    break;
                }
            }
        }
    }

    pub async fn open_web_portal(port: u16) {
        let url = format!("http://localhost:{}", port);
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}
