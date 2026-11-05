use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct AudioStatus {
    pub volume_pct: u8,
    pub is_muted: bool,
}

pub struct AudioBackend;

fn cached(which: &'static str) -> Option<AudioStatus> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    static CACHE: OnceLock<Mutex<HashMap<&'static str, (Instant, AudioStatus)>>> = OnceLock::new();
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let g = c.lock().unwrap();
    g.get(which)
        .filter(|(t, _)| t.elapsed().as_secs() < 15)
        .map(|(_, s)| s.clone())
}

fn store(which: &'static str, status: AudioStatus) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    static CACHE: OnceLock<Mutex<HashMap<&'static str, (Instant, AudioStatus)>>> = OnceLock::new();
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    c.lock().unwrap().insert(which, (Instant::now(), status));
}

impl AudioBackend {
    pub async fn get_sink() -> AudioStatus {
        if let Some(s) = cached("sink") {
            return s;
        }
        let s = Self::get_sink_uncached().await;
        store("sink", s.clone());
        s
    }

    pub async fn get_source() -> AudioStatus {
        if let Some(s) = cached("source") {
            return s;
        }
        let s = Self::get_source_uncached().await;
        store("source", s.clone());
        s
    }

    async fn get_sink_uncached() -> AudioStatus {
        if let Ok(out) = Command::new("wpctl")
            .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&out.stdout);
            return Self::parse_wpctl(&text);
        }

        if let Ok(out) = Command::new("pactl")
            .args(["get-sink-volume", "@DEFAULT_SINK@"])
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&out.stdout);
            let mute_out = Command::new("pactl")
                .args(["get-sink-mute", "@DEFAULT_SINK@"])
                .output()
                .await
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("yes"))
                .unwrap_or(false);

            if let Some(pos) = text.find('%') {
                if let Some(start) = text[..pos].rfind(|c: char| !c.is_ascii_digit()) {
                    let vol = text[start + 1..pos].parse().unwrap_or(0);
                    return AudioStatus {
                        volume_pct: vol,
                        is_muted: mute_out,
                    };
                }
            }
        }

        AudioStatus::default()
    }

    async fn get_source_uncached() -> AudioStatus {
        if let Ok(out) = Command::new("wpctl")
            .args(["get-volume", "@DEFAULT_AUDIO_SOURCE@"])
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&out.stdout);
            return Self::parse_wpctl(&text);
        }
        AudioStatus::default()
    }
    pub async fn toggle_sink_mute() {
        let _ = Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
            .status()
            .await;
    }

    pub async fn toggle_source_mute() {
        let _ = Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"])
            .status()
            .await;
    }

    pub async fn cycle_sinks() {
        if let Ok(out) = Command::new("wpctl").arg("status").output().await {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut sinks = Vec::new();
            let mut in_sinks = false;
            let mut current_id = None;

            for line in text.lines() {
                if line.contains("Sinks:") {
                    in_sinks = true;
                } else if line.contains("Sources:") || line.contains("Filters:") {
                    in_sinks = false;
                } else if in_sinks {
                    if let Some(num_str) = line.split('.').next() {
                        let id_str = num_str.trim().trim_start_matches('*').trim();
                        if let Ok(id) = id_str.parse::<u32>() {
                            sinks.push(id);
                            if line.contains('*') {
                                current_id = Some(id);
                            }
                        }
                    }
                }
            }

            if sinks.len() > 1 {
                let next_id = if let Some(cur) = current_id {
                    let idx = sinks.iter().position(|&x| x == cur).unwrap_or(0);
                    sinks[(idx + 1) % sinks.len()]
                } else {
                    sinks[0]
                };

                let _ = Command::new("wpctl")
                    .args(["set-default", &next_id.to_string()])
                    .status()
                    .await;
            }
        }
    }

    fn parse_wpctl(text: &str) -> AudioStatus {
        let is_muted = text.contains("[MUTED]");
        let mut volume_pct = 0;
        if let Some(vol_str) = text.strip_prefix("Volume:") {
            let num_part = vol_str.split_whitespace().next().unwrap_or("0.0");
            if let Ok(f) = num_part.parse::<f32>() {
                volume_pct = (f * 100.0).round().clamp(0.0, 150.0) as u8;
            }
        }
        AudioStatus {
            volume_pct,
            is_muted,
        }
    }
}

/// Resolve a user-configured alarm sound. Tries in order:
///   1. Absolute path as-is
///   2. `~/.local/share/moonbeauty/sounds/<basename>` (stable, where imports go now)
///
/// cwd-relative and $CARGO_MANIFEST_DIR candidates were dropped
/// (finding 19): they only worked from the source tree and could be
/// shadowed by an attacker-controlled ./resources/<name>.
///
/// Logs the resolved path when found; if nothing is found, the caller
/// logs the failure (see `play_alarm_sound`).
pub fn resolve_alarm_sound_path(sound: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let raw = PathBuf::from(sound);
    if raw.is_absolute() && raw.exists() && raw.is_file() {
        return Some(raw);
    }

    // Filename-only fallback across the standard locations. Use just the
    // basename so a moved directory still resolves via XDG data.
    let filename = raw
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| raw.clone());

    let mut candidates: Vec<PathBuf> = Vec::new();

    // Stable per-user data dir — where ImportAlarmSound now copies files.
    if let Some(data) = dirs::data_dir() {
        candidates.push(
            data.join("moonbeauty")
                .join("sounds")
                .join(&filename),
        );
    }

    // Finding 19: cwd-relative and CARGO_MANIFEST_DIR candidates dropped.

    for c in &candidates {
        if c.exists() && c.is_file() {
            return Some(c.clone());
        }
    }
    None
}

/// Play an alarm sound. Tries pw-play, then paplay, then aplay. Logs the
/// resolved path and every failure mode so the user can see what's wrong
/// in `/tmp/mbs-test.log`. Sleeps `duration_secs` in a detached thread,
/// then kills the child — no `timeout` binary dependency.
pub fn play_alarm_sound(sound: &str, volume_pct: u32, duration_secs: u32) {
    let Some(path) = resolve_alarm_sound_path(sound) else {
        eprintln!(
            "[alarm-sound] NOT FOUND: {:?} — tried XDG data/sounds, cwd, cwd/resources, manifest/resources",
            sound
        );
        return;
    };
    let path_str = path.display().to_string();
    let vol_f = (volume_pct as f32) / 100.0;
    let vol_pa = (vol_f * 65536.0) as u32;
    let dur = std::time::Duration::from_secs(duration_secs.max(1) as u64);

    eprintln!(
        "[alarm-sound] playing {} (vol={}%, dur={}s)",
        path_str, volume_pct, duration_secs
    );

    std::thread::spawn(move || {
        let try_spawn = |bin: &str, args: Vec<String>| -> Option<std::process::Child> {
            std::process::Command::new(bin)
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()
        };

        let mut child = try_spawn(
            "pw-play",
            vec![format!("--volume={:.2}", vol_f), path_str.clone()],
        );
        let mut tool = "pw-play";

        if child.is_none() {
            child = try_spawn(
                "paplay",
                vec![format!("--volume={}", vol_pa), path_str.clone()],
            );
            tool = "paplay";
        }
        if child.is_none() {
            child = try_spawn("aplay", vec![path_str.clone()]);
            tool = "aplay";
        }

        let Some(mut child) = child else {
            eprintln!("[alarm-sound] no audio tool available (tried pw-play, paplay, aplay)");
            return;
        };

        std::thread::sleep(dur);
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("[alarm-sound] finished ({})", tool);
    });
}
