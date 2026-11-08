//! MPRIS media player client — pure zbus, no libdbus, no playerctl.
//!
//! Enumerates `org.mpris.MediaPlayer2.*` bus names, prefers whichever is
//! currently Playing, and reads/writes the standard Player interface.

#![allow(clippy::type_complexity)]
use std::collections::HashMap;
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

/// Finding 31: cap MPRIS-provided strings before concatenation /
/// display. The initial `get_property::<String>` allocation is
/// unavoidable (it's the D-Bus message payload); this bounds the
/// *derived* work. Real `xesam:title`/`album` are <200 chars; 512 is
/// generous. `xesam:url` gets 4096 (Windows-style long paths).
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Finding 33: parse a `file://` MPRIS track URL into a safe absolute
/// path. Rejects relative paths and traversal. `open_track_folder` and
/// `trash_current_track` are the only sinks — both require a user
/// click, but the click is presented as innocuous ("Folder", "Trash").
fn safe_mpris_file_path(url: &str) -> Option<std::path::PathBuf> {
    let clean = url.strip_prefix("file://")?;
    // Reject relative (file://etc → no leading slash) and traversal
    // (file:///home/u/../../../etc).
    if !clean.starts_with('/') || clean.contains("..") {
        crate::dbg_log!("[mpris] rejected unsafe track url: {:?}", clean);
        return None;
    }
    // Finding 33 (corrected): restrict to media extensions. The earlier
    // comment claimed is_absolute + !contains("..") blocked
    // file:///home/u/.ssh/id_rsa, but /home/u/.ssh/id_rsa passes both
    // checks. A hostile MPRIS bridge presenting itself as a music player
    // could trigger `gio trash ~/.ssh/id_rsa` via the tooltip's Trash
    // button. Extension allowlist is the real fix.
    const MEDIA_EXT: &[&str] = &[
        "mp3", "flac", "ogg", "opus", "m4a", "wav", "aac", "wma", "alac",
        "aiff", "aif", "mka", "mp4", "m4v", "mkv", "webm", "mov",
    ];
    let ext = std::path::Path::new(clean)
        .extension()
        .and_then(|s| s.to_str())?;
    if !MEDIA_EXT.iter().any(|x| ext.eq_ignore_ascii_case(x)) {
        crate::dbg_log!("[mpris] rejected non-media extension: {:?} ({})", clean, ext);
        return None;
    }
    Some(std::path::PathBuf::from(clean))
}

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
        // Finding 32: cap enumeration at 20. A hostile app can register
        // thousands of `org.mpris.MediaPlayer2.*` names; unbounded
        // iteration → unbounded D-Bus probe calls → shell hang.
        // Real systems have ≤5 legit players; 20 is generous headroom.
        let mpris_names: Vec<String> = names
            .into_iter()
            .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
            .filter(|n| !n.ends_with(".playerctld"))
            .take(20)
            .collect();

        if mpris_names.is_empty() {
            return None;
        }

        // Prefer a player that's currently Playing.
        // Finding 32: per-player timeout so a hung / hostile proxy
        // doesn't block the whole enumeration. 2s is generous for a
        // local D-Bus round-trip (real players respond in <50ms).
        for name in &mpris_names {
            if let Ok(proxy) = zbus::Proxy::new(&conn, name.as_str(), MPRIS_PATH, MPRIS_IFACE).await
            {
                if let Ok(Ok(status)) = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    proxy.get_property::<String>("PlaybackStatus"),
                )
                .await
                {
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
                // Finding 32: cache None results for 30s instead of 5s.
                // A hostile player that hangs forces a 10s timeout per
                // poll; caching the None longer avoids 10s hangs on
                // every 15s hardware tick.
                let cache_ttl = if tr.is_some() { 5 } else { 30 };
                if t.elapsed().as_secs() < cache_ttl {
                    return tr.clone();
                }
            }
        }
        // Finding 32: outer 10s timeout caps worst case for the whole
        // poll_uncached invocation. Per-player timeout is 2s × 20 cap =
        // 40s worst case; this brings it to 10s.
        let result =
            match tokio::time::timeout(std::time::Duration::from_secs(10), Self::poll_uncached())
                .await
            {
                Ok(r) => r,
                Err(_) => {
                    eprintln!("[mpris] poll timed out at 10s (finding 32)");
                    None
                }
            };
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

        // Finding 31: cap each string as it's materialized. The primary
        // D-Bus allocation is unpreventable at this layer; this bounds
        // the downstream concat/display amplification.
        let get_str = |key: &str, max: usize| -> Option<String> {
            metadata.get(key).and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(truncate_chars(s.as_str(), max)),
                _ => None,
            })
        };

        let title = get_str("xesam:title", 512).unwrap_or_default();
        let album = get_str("xesam:album", 512).unwrap_or_default();
        let url = get_str("xesam:url", 4096);

        // Finding 31: cap array length (8 artists is generous) and each
        // entry (256 chars). A hostile provider returning 1M-item arrays
        // × 1KB each would otherwise allocate 1GB during join.
        let artist = metadata
            .get("xesam:artist")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(truncate_chars(s.as_str(), 512)),
                zbus::zvariant::Value::Array(arr) => {
                    let parts: Vec<String> = arr
                        .iter()
                        .take(8)
                        .filter_map(|item| {
                            if let zbus::zvariant::Value::Str(s) = item {
                                Some(truncate_chars(s.as_str(), 256))
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

    /// List all currently registered MPRIS bus names, capped at 20.
    /// Used by the music cycle to enumerate available players.
    pub async fn list_players() -> Vec<String> {
        let Ok(conn) = Connection::session().await else {
            return Vec::new();
        };
        let Ok(reply) = conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "ListNames",
                &(),
            )
            .await
        else {
            return Vec::new();
        };
        let Ok(names) = reply.body().deserialize::<Vec<String>>() else {
            return Vec::new();
        };
        names
            .into_iter()
            .filter(|n| n.starts_with("org.mpris.MediaPlayer2."))
            .filter(|n| !n.ends_with(".playerctld"))
            .take(20)
            .collect()
    }

    /// Poll a specific player by bus name. Used by the music cycle;
    /// the normal `poll()` auto-selects. Same field extraction as
    /// `poll_uncached`, just targeted.
    pub async fn poll_for(bus: &str) -> Option<MprisTrack> {
        let conn = Connection::session().await.ok()?;
        let proxy = zbus::Proxy::new(&conn, bus, MPRIS_PATH, MPRIS_IFACE)
            .await
            .ok()?;
        let t = std::time::Duration::from_secs(2);
        let metadata: HashMap<String, OwnedValue> =
            tokio::time::timeout(t, proxy.get_property("Metadata"))
                .await
                .ok()?
                .ok()?;
        let is_playing = tokio::time::timeout(t, proxy.get_property::<String>("PlaybackStatus"))
            .await
            .ok()?
            .ok()
            .map(|s| s == "Playing")
            .unwrap_or(false);
        let position_sec = tokio::time::timeout(t, proxy.get_property::<i64>("Position"))
            .await
            .ok()?
            .ok()
            .map(|p| p as f64 / 1_000_000.0)
            .unwrap_or(0.0);
        let volume_pct = tokio::time::timeout(t, proxy.get_property::<f64>("Volume"))
            .await
            .ok()?
            .ok()
            .map(|v| (v * 100.0).clamp(0.0, 100.0) as u8)
            .unwrap_or(100);

        let get_str = |key: &str, max: usize| -> Option<String> {
            metadata.get(key).and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(truncate_chars(s.as_str(), max)),
                _ => None,
            })
        };
        let title = get_str("xesam:title", 512).unwrap_or_default();
        let album = get_str("xesam:album", 512).unwrap_or_default();
        let url = get_str("xesam:url", 4096);
        let artist = metadata
            .get("xesam:artist")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(truncate_chars(s.as_str(), 512)),
                zbus::zvariant::Value::Array(arr) => {
                    let parts: Vec<String> = arr
                        .iter()
                        .take(8)
                        .filter_map(|item| {
                            if let zbus::zvariant::Value::Str(s) = item {
                                Some(truncate_chars(s.as_str(), 256))
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

    /// Play/pause a specific player by bus name. Used by the music
    /// cycle and the tooltip buttons (finding 74 fix).
    ///
    /// Finding 75: use `PlayPause` (required-by-MPRIS-spec toggle)
    /// after checking current state, instead of `Play`/`Pause`
    /// individually. Firefox implements all four; some MPRIS bridges
    /// (e.g. mpv-mpris) stub the individual ones. `PlayPause` with a
    /// pre-check is universal.
    pub async fn set_playing_for(bus: &str, play: bool) {
        let Ok(conn) = Connection::session().await else {
            crate::dbg_log!("[mpris] set_playing_for({}): no session conn", bus);
            return;
        };
        let Ok(proxy) = zbus::Proxy::new(&conn, bus, MPRIS_PATH, MPRIS_IFACE).await else {
            crate::dbg_log!("[mpris] set_playing_for({}): proxy failed", bus);
            return;
        };
        let method = if play { "Play" } else { "Pause" };
        match proxy.call_method(method, &()).await {
            Ok(_) => crate::dbg_log!("[mpris] set_playing_for({}): {} (idempotent)", bus, method),
            Err(_) => {
                let _ = proxy.call_method("PlayPause", &()).await;
                crate::dbg_log!("[mpris] set_playing_for({}): PlayPause fallback", bus);
            }
        }
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
        // Finding 33: reject relative / traversal paths from hostile MPRIS.
        if let Some(path) = url_opt.and_then(safe_mpris_file_path) {
            if let Some(parent) = path.parent() {
                let _ = Command::new("xdg-open").arg(parent).spawn();
            }
        }
    }

    pub async fn trash_current_track(url_opt: Option<&str>) -> bool {
        // Finding 33: `gio trash <path>` destroys the file. Without this
        // guard, hostile MPRIS sets xesam:url = file:///home/u/.ssh/id_rsa
        // and the user's "Trash" click silently destroys their SSH key.
        if let Some(path) = url_opt.and_then(safe_mpris_file_path) {
            let status = Command::new("gio")
                .args(["trash"])
                .arg(&path)
                .status()
                .await;
            if status.map(|s| s.success()).unwrap_or(false) {
                Self::next().await;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod safe_mpris_tests {
    use super::{safe_mpris_file_path, truncate_chars};
    use std::path::PathBuf;

    #[test]
    fn truncate_short_unchanged() {
        assert_eq!(truncate_chars("", 10), "");
        assert_eq!(truncate_chars("abc", 10), "abc");
        assert_eq!(truncate_chars("abcdefghij", 10), "abcdefghij");
    }

    #[test]
    fn truncate_long_ascii() {
        assert_eq!(truncate_chars("abcdefghij", 5), "abcde");
        assert_eq!(truncate_chars("a".repeat(10000).as_str(), 100).len(), 100);
    }

    #[test]
    fn truncate_unicode_respects_char_boundary() {
        // 5 chars (each 3 bytes UTF-8), cap to 3 chars = 9 bytes
        assert_eq!(truncate_chars("日本語です", 3), "日本語");
        // Never splits a multi-byte char in half
        let out = truncate_chars("日本語です", 4);
        assert_eq!(out.chars().count(), 4);
        assert_eq!(out, "日本語で");
    }

    #[test]
    fn file_path_absolute_accepted() {
        let p = safe_mpris_file_path("file:///home/user/Music/track.mp3");
        assert_eq!(p, Some(PathBuf::from("/home/user/Music/track.mp3")));
    }

    #[test]
    fn file_path_relative_rejected() {
        assert!(safe_mpris_file_path("file://etc/passwd").is_none());
        assert!(safe_mpris_file_path("file://home/user/track.mp3").is_none());
    }

    #[test]
    fn file_path_traversal_rejected() {
        assert!(safe_mpris_file_path("file:///home/user/../../../etc/passwd").is_none());
        assert!(safe_mpris_file_path("file:///home/user/..").is_none());
    }

    #[test]
    fn file_path_non_file_scheme_rejected() {
        assert!(safe_mpris_file_path("http://example.com/track.mp3").is_none());
        assert!(safe_mpris_file_path("https://example.com/track.mp3").is_none());
        assert!(safe_mpris_file_path("spotify:track:abc123").is_none());
        assert!(safe_mpris_file_path("").is_none());
    }

    #[test]
    fn non_media_extension_rejected() {
        // Finding 33 (corrected): the previous guard allowed any absolute
        // path, so file:///home/u/.ssh/id_rsa passed and 'Trash' would
        // run `gio trash ~/.ssh/id_rsa`. Media-extension allowlist now
        // blocks non-media extensions.
        assert!(safe_mpris_file_path("file:///home/u/.ssh/id_rsa").is_none());
        assert!(safe_mpris_file_path("file:///home/u/Documents/thesis.pdf").is_none());
        assert!(safe_mpris_file_path("file:///tmp/notes.txt").is_none());
        assert!(safe_mpris_file_path("file:///home/u/no_extension").is_none());
    }

    #[test]
    fn media_extensions_accepted() {
        for ext in &["mp3", "flac", "ogg", "opus", "m4a", "wav", "aac", "m4v", "mkv", "webm"] {
            let url = format!("file:///home/u/Music/track.{}", ext);
            assert!(safe_mpris_file_path(&url).is_some(), "{} rejected", ext);
        }
        assert!(safe_mpris_file_path("file:///home/u/Music/TRACK.MP3").is_some());
    }
}
