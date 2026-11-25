//! Music backend: native local player, MPRIS D-Bus client, and the
//! player-cycling state machine that picks between them.

pub mod cycle;
pub mod local;
pub mod mpris;

pub use cycle::{press, MusicCycleState, MusicSources, PlayerKind, PressOutcome};
pub use local::LocalMusicBackend;
pub use mpris::{MprisBackend, MprisTrack};

/// Which backend is currently serving the music capsule's data.
/// Local player is preferred over MPRIS when both report Playing —
/// local playback is dedicated, MPRIS usually means a browser tab.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TrackSource {
    Local,
    Mpris,
    None,
}

/// Poll both the local player and MPRIS, pick a winner.
///
/// Priority:
///   1. Local playing         -> Local
///   2. MPRIS playing         -> Mpris
///   3. Local loaded          -> Local
///   4. MPRIS exists          -> Mpris
///   5. Local idle fallback   -> Local
///   6. Neither               -> None
pub async fn poll_track() -> (Option<MprisTrack>, TrackSource) {
    let local = LocalMusicBackend::poll().await;
    let mpris = MprisBackend::poll().await;
    match (local, mpris) {
        (Some(m), _) if m.is_playing => (Some(m), TrackSource::Local),
        (_, Some(m)) if m.is_playing => (Some(m), TrackSource::Mpris),
        (Some(m), _) if m.url.is_some() => (Some(m), TrackSource::Local),
        (_, Some(m)) => (Some(m), TrackSource::Mpris),
        (Some(m), _) => (Some(m), TrackSource::Local),
        (None, None) => (None, TrackSource::None),
    }
}

pub mod cli;
