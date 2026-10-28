//! MPRIS media player client — pure zbus, no libdbus, no playerctl.
//!
//! Enumerates `org.mpris.MediaPlayer2.*` bus names, prefers whichever is
//! currently Playing, and reads/writes the standard Player interface.

#![allow(clippy::type_complexity)]
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;
use zbus::zvariant::OwnedValue;
use zbus::Connection;

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

const MPRIS_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";

impl MprisBackend {
    async fn get_active_player() -> Option<(Connection, String)> {
        let conn = Connection::session().await.ok()?;

        let reply = conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "ListNames",
                &(),
            )
            .await
            .ok()?;

        let names: Vec<String> = reply.body().deserialize().ok()?;
        let mpris_names: Vec<String> = names
            .into_iter()
            .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
            .filter(|n| !n.ends_with(".playerctld"))
            .collect();

        if mpris_names.is_empty() {
            return None;
        }

        // Prefer a player that's currently Playing
        for name in &mpris_names {
            if let Ok(proxy) = zbus::Proxy::new(&conn, name.as_str(), MPRIS_PATH, MPRIS_IFACE).await
            {
                if let Ok(status) = proxy.get_property::<String>("PlaybackStatus").await {
                    if status == "Playing" {
                        return Some((conn, name.clone()));
                    }
                }
            }
        }

        Some((conn, mpris_names[0].clone()))
    }

    pub async fn poll() -> Option<MprisTrack> {
        // Cache for 5s. MPRIS changes are user-driven (a track change every
        // few minutes at most); a 3s poll cadence D-Bus round-trips 20x per
        // minute for no benefit.
        use std::sync::{Mutex, OnceLock};
        use std::time::Instant;
        static CACHE: OnceLock<Mutex<Option<(Instant, Option<MprisTrack>)>>> = OnceLock::new();
        let c = CACHE.get_or_init(|| Mutex::new(None));
        {
            let g = c.lock().unwrap();
            if let Some((t, tr)) = &*g {
                if t.elapsed().as_secs() < 5 {
                    return tr.clone();
                }
            }
        }
        let result = Self::poll_uncached().await;
        {
            let mut g = c.lock().unwrap();
            *g = Some((Instant::now(), result.clone()));
        }
        result
    }

    async fn poll_uncached() -> Option<MprisTrack> {
        let (conn, bus_name) = Self::get_active_player().await?;
        let proxy = zbus::Proxy::new(&conn, bus_name.as_str(), MPRIS_PATH, MPRIS_IFACE)
            .await
            .ok()?;

        let metadata: HashMap<String, OwnedValue> = proxy.get_property("Metadata").await.ok()?;

        let get_str = |key: &str| -> Option<String> {
            metadata.get(key).and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
        };

        let title = get_str("xesam:title").unwrap_or_default();
        let album = get_str("xesam:album").unwrap_or_default();
        let url = get_str("xesam:url");

        let artist = metadata
            .get("xesam:artist")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(s.to_string()),
                zbus::zvariant::Value::Array(arr) => {
                    let parts: Vec<String> = arr
                        .iter()
                        .filter_map(|item| {
                            if let zbus::zvariant::Value::Str(s) = item {
                                Some(s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if parts.is_empty() {
                        None
                    } else {
                        Some(parts.join(", "))
                    }
                }
                _ => None,
            })
            .unwrap_or_default();

        let length_sec = metadata
            .get("mpris:length")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::I64(n) => Some(*n as f64 / 1_000_000.0),
                zbus::zvariant::Value::U64(n) => Some(*n as f64 / 1_000_000.0),
                _ => None,
            })
            .unwrap_or(0.0);

        let is_playing = proxy
            .get_property::<String>("PlaybackStatus")
            .await
            .map(|s| s == "Playing")
            .unwrap_or(false);

        let position_sec = proxy
            .get_property::<i64>("Position")
            .await
            .map(|p| p as f64 / 1_000_000.0)
            .unwrap_or(0.0);

        let volume_pct = proxy
            .get_property::<f64>("Volume")
            .await
            .map(|v| (v * 100.0).clamp(0.0, 100.0) as u8)
            .unwrap_or(100);

        if title.is_empty() && artist.is_empty() {
            return None;
        }

        Some(MprisTrack {
            title,
            artist,
            album,
            is_playing,
            position_sec,
            length_sec,
            volume_pct,
            url,
        })
    }

    async fn call_method(method: &str) {
        let Some((conn, bus_name)) = Self::get_active_player().await else {
            return;
        };
        let Ok(proxy) = zbus::Proxy::new(&conn, bus_name.as_str(), MPRIS_PATH, MPRIS_IFACE).await
        else {
            return;
        };
        let _ = proxy.call_method(method, &()).await;
    }

    pub async fn toggle_play_pause() {
        Self::call_method("PlayPause").await;
    }

    pub async fn next() {
        Self::call_method("Next").await;
    }

    pub async fn previous() {
        Self::call_method("Previous").await;
    }

    pub async fn set_position(sec: f64) {
        let Some((conn, bus_name)) = Self::get_active_player().await else {
            return;
        };
        let Ok(proxy) = zbus::Proxy::new(&conn, bus_name.as_str(), MPRIS_PATH, MPRIS_IFACE).await
        else {
            return;
        };
        // Most players ignore the TrackId argument for SetPosition and just
        // seek within the current track. NoTrack is the safe sentinel.
        let Ok(track_id) =
            zbus::zvariant::ObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack")
        else {
            return;
        };
        let pos_us = (sec * 1_000_000.0) as i64;
        let _ = proxy.call_method("SetPosition", &(track_id, pos_us)).await;
    }

    pub async fn set_volume(vol_pct: u8) {
        let Some((conn, bus_name)) = Self::get_active_player().await else {
            return;
        };
        let Ok(proxy) = zbus::Proxy::new(&conn, bus_name.as_str(), MPRIS_PATH, MPRIS_IFACE).await
        else {
            return;
        };
        let _ = proxy.set_property("Volume", (vol_pct as f64) / 100.0).await;
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
