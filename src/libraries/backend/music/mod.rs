//! Music backend: MPD protocol client, MPRIS D-Bus client, and the
//! player-cycling state machine that picks between them.

pub mod cycle;
pub mod mpd;
pub mod mpris;

pub use cycle::{press, MusicCycleState, MusicSources, PlayerKind, PressOutcome};
pub use mpd::MpdBackend;
pub use mpris::{MprisBackend, MprisTrack};

/// Which backend is currently serving the music capsule's data.
/// MPD is preferred over MPRIS when both report Playing — MPD is a
/// dedicated music daemon, MPRIS usually means a browser media tab.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TrackSource {
    Mpd,
    Mpris,
    None,
}

/// Poll both MPD and MPRIS, pick a winner.
///
/// Priority:
///   1. MPD playing    → Mpd
///   2. MPRIS playing  → Mpris
///   3. MPD exists but paused → Mpd
///   4. MPRIS exists   → Mpris
///   5. Neither        → None
pub async fn poll_track() -> (Option<MprisTrack>, TrackSource) {
    let mpd = MpdBackend::poll().await;
    let mpris = MprisBackend::poll().await;
    match (mpd, mpris) {
        (Some(m), _) if m.is_playing => (Some(m), TrackSource::Mpd),
        (_, Some(m)) if m.is_playing => (Some(m), TrackSource::Mpris),
        (Some(m), _) => (Some(m), TrackSource::Mpd),
        (_, Some(m)) => (Some(m), TrackSource::Mpris),
        (None, None) => (None, TrackSource::None),
    }
}
