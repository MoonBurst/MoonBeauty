//! MPD backend — native client, no MPRIS bridge needed.
//!
//! MPD speaks its own protocol on TCP:6600 or a Unix socket. Unlike
//! Firefox/Spotify, it does NOT register org.mpris.MediaPlayer2.* on
//! D-Bus unless a separate `mpd-mpris` daemon is running. This module
//! removes that dependency — the shell talks MPD protocol directly.
//!
//! API facts driving this code (mpd_client 1.4.1):
//!   - `Client::connect_to` / `connect_unix` return `(Client, StateChanges)`.
//!   - `Status.current_song` carries only `(SongPosition, SongId)` — no
//!     tags. Metadata requires a second `CurrentSong` command, batched
//!     with `command_list` to keep it one round-trip.
//!   - `Song` exposes `duration: Option<Duration>` and
//!     `tags: HashMap<Tag, Vec<String>>` as *fields*, not accessors.
//!   - There is no `TogglePause` — read state, then `SetPause(!paused)`.
//!
//! Naming quirk: returns `MprisTrack` even though the source is MPD.
//! The struct fields are identical, and `UnifiedTelemetry` already
//! holds `Option<MprisTrack>`. A neutral `Track` type would ripple
//! through 6 files — deferred as a naming refactor.

use std::path::PathBuf;
use std::time::Duration;

use mpd_client::{
    commands::{
        definitions::{CurrentSong, Next, Play, Previous, SetPause},
        Status,
    },
    responses::{PlayState, Song},
    tag::Tag,
    Client,
};

use super::mpris::MprisTrack;

pub struct MpdBackend;

impl MpdBackend {
    /// Resolve the connection target. Priority:
    ///   1. `MPD_HOST` env (if starts with `/` → Unix socket; else
    ///      `host:MPD_PORT` or `host:6600`)
    ///   2. `$XDG_RUNTIME_DIR/mpd/socket` if it exists
    ///   3. `127.0.0.1:6600` (MPD default)
    ///
    /// Pure function so it's unit-testable without env-var races.
    fn resolve_target(
        host_env: Option<&str>,
        port_env: Option<&str>,
        xdg_runtime: Option<&std::path::Path>,
        xdg_socket_exists: bool,
    ) -> String {
        if let Some(host) = host_env {
            if host.starts_with('/') {
                return host.to_string();
            }
            return format!("{}:{}", host, port_env.unwrap_or("6600"));
        }
        if let Some(runtime) = xdg_runtime {
            if xdg_socket_exists {
                return runtime
                    .join("mpd")
                    .join("socket")
                    .to_string_lossy()
                    .into_owned();
            }
        }
        "127.0.0.1:6600".into()
    }

    fn connect_target() -> String {
        let host = std::env::var("MPD_HOST").ok();
        let port = std::env::var("MPD_PORT").ok();
        let xdg: Option<PathBuf> =
            std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let xdg_sock_exists = xdg
            .as_ref()
            .map(|d| d.join("mpd").join("socket").exists())
            .unwrap_or(false);
        Self::resolve_target(
            host.as_deref(),
            port.as_deref(),
            xdg.as_deref(),
            xdg_sock_exists,
        )
    }

    async fn connect() -> Option<Client> {
        let target = Self::connect_target();
        // mpd_client 1.x removed `connect_to` / `connect_unix`. The only
        // entry point is the generic `Client::connect(connection)` which
        // takes an already-open stream. We open it ourselves:
        //   - Unix socket path → tokio::net::UnixStream
        //   - host:port       → tokio::net::TcpStream
        // Errors from the stream side and the protocol side are
        // stringified to unify the type (io::Error vs MpdProtocolError).
        let result: Result<Client, String> = if target.starts_with('/') {
            match tokio::net::UnixStream::connect(target.as_str()).await {
                Ok(stream) => Client::connect(stream)
                    .await
                    .map(|(c, _)| c)
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        } else {
            match tokio::net::TcpStream::connect(target.as_str()).await {
                Ok(stream) => Client::connect(stream)
                    .await
                    .map(|(c, _)| c)
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        match result {
            Ok(client) => Some(client),
            Err(e) => {
                // dbg_log! so silent unless MBS_DEBUG — users without
                // MPD running shouldn't see per-poll error spam.
                crate::dbg_log!("[mpd] connect {} failed: {}", target, e);
                None
            }
        }
    }

    /// First value of a tag, or empty string. MPD can return multiple
    /// values per tag (e.g. multiple Artist entries); the first is what
    /// the capsule needs.
    fn first_tag(song: &Song, tag: Tag) -> String {
        song.tags
            .get(&tag)
            .and_then(|v| v.first())
            .cloned()
            .unwrap_or_default()
    }

    /// Poll current MPD state. Returns None if MPD isn't running,
    /// unreachable, or has no current song. Silent — the poll loop
    /// falls back to MPRIS on None.
    pub async fn poll() -> Option<MprisTrack> {
        let client = Self::connect().await?;
        // One round-trip: status (state/elapsed/volume) + current song
        // (tags/duration). `Status.current_song` only has IDs, so the
        // second command is required.
        let (status, song) = match client
            .command_list((Status, CurrentSong))
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                crate::dbg_log!("[mpd] command_list failed: {}", e);
                return None;
            }
        };
        // Finding 74: `CurrentSong` returns `Option<SongInQueue>`, which
        // wraps the metadata in `.song` alongside queue position/id.
        // Unwrap to `Song` for tag/duration access.
        let song = song?.song;

        let title = Self::first_tag(&song, Tag::Title);
        let artist = Self::first_tag(&song, Tag::Artist);
        // Empty metadata: nothing meaningful to render. Matches MPRIS
        // backend's `if title.is_empty() && artist.is_empty() → None`.
        if title.is_empty() && artist.is_empty() {
            return None;
        }

        Some(MprisTrack {
            title,
            artist,
            album: Self::first_tag(&song, Tag::Album),
            is_playing: matches!(status.state, PlayState::Playing),
            position_sec: status.elapsed.unwrap_or(Duration::ZERO).as_secs_f64(),
            length_sec: song.duration.unwrap_or(Duration::ZERO).as_secs_f64(),
            volume_pct: status.volume,
            url: None, // MPD doesn't expose a file:// URL in Status
        })
    }

    pub async fn next() {
        if let Some(client) = Self::connect().await {
            let _ = client.command(Next).await;
        }
    }

    /// Explicit play — used by the music cycle. `Play::current()`
    /// resumes from current position or starts from stopped.
    pub async fn play() {
        match Self::connect().await {
            Some(client) => match client.command(Play::current()).await {
                Ok(_) => crate::dbg_log!("[mpd] play OK"),
                Err(e) => crate::dbg_log!("[mpd] play failed: {}", e),
            },
            None => crate::dbg_log!("[mpd] play: connect failed"),
        }
    }

    /// Explicit pause. `SetPause(true)` is a no-op if already paused,
    /// harmless.
    pub async fn pause() {
        match Self::connect().await {
            Some(client) => match client.command(SetPause(true)).await {
                Ok(_) => crate::dbg_log!("[mpd] pause OK"),
                Err(e) => crate::dbg_log!("[mpd] pause failed: {}", e),
            },
            None => crate::dbg_log!("[mpd] pause: connect failed"),
        }
    }

    pub async fn previous() {
        if let Some(client) = Self::connect().await {
            let _ = client.command(Previous).await;
        }
    }

    /// No `TogglePause` in mpd_client 1.x — read state, then set the
    /// opposite. `Stopped` maps to "start playing" (SetPause(false)
    /// is a no-op when stopped, so use Play instead).
    pub async fn toggle_play_pause() {
        let Some(client) = Self::connect().await else {
            return;
        };
        let Ok(status) = client.command(Status).await else {
            return;
        };
        let _ = match status.state {
            PlayState::Playing => client.command(SetPause(true)).await,
            PlayState::Paused => client.command(SetPause(false)).await,
            PlayState::Stopped => client.command(Play::current()).await,
        };
    }
}

#[cfg(test)]
mod resolve_target_tests {
    use super::MpdBackend;

    #[test]
    fn unix_socket_from_env() {
        assert_eq!(
            MpdBackend::resolve_target(Some("/run/mpd/socket"), None, None, false),
            "/run/mpd/socket"
        );
    }

    #[test]
    fn tcp_with_port_env() {
        assert_eq!(
            MpdBackend::resolve_target(Some("example.com"), Some("6601"), None, false),
            "example.com:6601"
        );
    }

    #[test]
    fn tcp_default_port() {
        assert_eq!(
            MpdBackend::resolve_target(Some("localhost"), None, None, false),
            "localhost:6600"
        );
    }

    #[test]
    fn xdg_socket_when_exists() {
        let xdg = std::path::Path::new("/run/user/1000");
        assert_eq!(
            MpdBackend::resolve_target(None, None, Some(xdg), true),
            "/run/user/1000/mpd/socket"
        );
    }

    #[test]
    fn xdg_socket_when_missing_falls_through() {
        let xdg = std::path::Path::new("/run/user/1000");
        assert_eq!(
            MpdBackend::resolve_target(None, None, Some(xdg), false),
            "127.0.0.1:6600"
        );
    }

    #[test]
    fn no_env_no_xdg_default_tcp() {
        assert_eq!(
            MpdBackend::resolve_target(None, None, None, false),
            "127.0.0.1:6600"
        );
    }

    #[test]
    fn env_host_wins_over_xdg() {
        let xdg = std::path::Path::new("/run/user/1000");
        assert_eq!(
            MpdBackend::resolve_target(Some("remote.host"), None, Some(xdg), true),
            "remote.host:6600"
        );
    }
}
