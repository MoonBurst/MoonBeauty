use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct MprisTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub position_sec: f64,
    pub length_sec: f64,
    pub volume_pct: u8,
    pub url: Option<String>,
}

pub struct MprisBackend;

impl MprisBackend {
    pub async fn poll() -> Option<MprisTrack> {
        let output = Command::new("playerctl")
            .args([
                "metadata",
                "--format",
                "{{title}}|{{artist}}|{{album}}|{{status}}|{{position}}|{{mpris:length}}|{{volume}}|{{xesam:url}}",
            ])
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let line = raw.lines().next()?;
        let parts: Vec<&str> = line.split('|').collect();

        if parts.len() < 7 {
            return None;
        }

        let title = parts[0].trim().to_string();
        let artist = parts[1].trim().to_string();
        let album = parts[2].trim().to_string();
        let status = parts[3].trim();
        let pos_us = parts[4].trim().parse::<f64>().unwrap_or(0.0);
        let len_us = parts[5].trim().parse::<f64>().unwrap_or(0.0);
        let vol = parts[6].trim().parse::<f64>().unwrap_or(1.0);
        let url = parts.get(7).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        if title.is_empty() && artist.is_empty() {
            return None;
        }

        Some(MprisTrack {
            title,
            artist,
            album,
            is_playing: status.eq_ignore_ascii_case("Playing"),
            position_sec: pos_us / 1_000_000.0,
            length_sec: len_us / 1_000_000.0,
            volume_pct: (vol * 100.0).clamp(0.0, 100.0) as u8,
            url,
        })
    }

    pub async fn toggle_play_pause() {
        let _ = Command::new("playerctl").arg("play-pause").status().await;
    }

    pub async fn next() {
        let _ = Command::new("playerctl").arg("next").status().await;
    }

    pub async fn previous() {
        let _ = Command::new("playerctl").arg("previous").status().await;
    }

    pub async fn set_position(sec: f64) {
        let _ = Command::new("playerctl")
            .args(["position", &format!("{:.2}", sec)])
            .status()
            .await;
    }

    pub async fn set_volume(vol_pct: u8) {
        let frac = (vol_pct as f64) / 100.0;
        let _ = Command::new("playerctl")
            .args(["volume", &format!("{:.2}", frac)])
            .status()
            .await;
    }

    pub async fn open_track_folder(url_opt: Option<&str>) {
        if let Some(url) = url_opt {
            if let Some(clean_path) = url.strip_prefix("file://") {
                let path = PathBuf::from(clean_path);
                if let Some(parent) = path.parent() {
                    let _ = Command::new("xdg-open").arg(parent).spawn();
                }
            }
        }
    }

    pub async fn trash_current_track(url_opt: Option<&str>) -> bool {
        if let Some(url) = url_opt {
            if let Some(clean_path) = url.strip_prefix("file://") {
                let status = Command::new("gio")
                    .args(["trash", clean_path])
                    .status()
                    .await;
                if status.map(|s| s.success()).unwrap_or(false) {
                    Self::next().await;
                    return true;
                }
            }
        }
        false
    }
}
