pub mod audio;
pub mod clock;
pub mod email_client;
pub mod local_sort;
pub mod mpd;
pub mod mpris;
pub mod music_cycle;
pub mod system;

pub use audio::{play_alarm_sound, resolve_alarm_sound_path, AudioBackend, AudioStatus};
pub use clock::{CalendarViewData, ClockBackend, WorldZoneColumn};
pub use mpd::MpdBackend;
pub use mpris::{MprisBackend, MprisTrack};
pub use music_cycle::{MusicCycleState, PlayerKind, PressOutcome};
pub use system::{
    BatteryMetrics, CpuMetrics, GpuInfo, GpuMetrics, NetMetrics, RamMetrics, SystemCollector,
};

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
///   1. MPD playing    → Mpd  (dedicated daemon > incidental browser media)
///   2. MPRIS playing  → Mpris
///   3. MPD exists but paused → Mpd  (dedicated wins even when paused)
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
