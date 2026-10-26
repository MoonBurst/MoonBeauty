use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct AudioStatus {
    pub volume_pct: u8,
    pub is_muted: bool,
}

pub struct AudioBackend;

fn cached(which: &'static str) -> Option<AudioStatus> {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    use std::collections::HashMap;
    static CACHE: OnceLock<Mutex<HashMap<&'static str, (Instant, AudioStatus)>>> = OnceLock::new();
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let g = c.lock().unwrap();
    g.get(which)
        .filter(|(t, _)| t.elapsed().as_secs() < 5)
        .map(|(_, s)| s.clone())
}

fn store(which: &'static str, status: AudioStatus) {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    use std::collections::HashMap;
    static CACHE: OnceLock<Mutex<HashMap<&'static str, (Instant, AudioStatus)>>> = OnceLock::new();
    let c = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    c.lock().unwrap().insert(which, (Instant::now(), status));
}

impl AudioBackend {
    pub async fn get_sink() -> AudioStatus {
        if let Some(s) = cached("sink") { return s; }
        let s = Self::get_sink_uncached().await;
        store("sink", s.clone());
        s
    }

    pub async fn get_source() -> AudioStatus {
        if let Some(s) = cached("source") { return s; }
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

    pub async fn adjust_sink_volume(step_pct: i8) {
        let sign = if step_pct >= 0 { "+" } else { "-" };
        let frac = format!("{:.2}{}", (step_pct.abs() as f32) / 100.0, sign);
        let _ = Command::new("wpctl")
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &frac])
            .status()
            .await;
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
