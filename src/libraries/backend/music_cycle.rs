//! Music cycle — press the tooltip's Cycle button (or `--music-cycle`)
//! to cycle through available players: play, pause, next, play, pause,
//! next, ... Only one plays at a time.
//!
//! Available = probe result. Only players with a loaded track count.
//! MPD with empty queue won't appear (nothing to play). Firefox with
//! no video loaded → no Metadata → no appearance. Users close a player
//! and the cycle redistributes automatically on the next press.
//!
//! State: `next_idx` (which to play next) + `last_playing` (which we
//! most recently started). Both updated atomically per press.

use super::mpd::MpdBackend;
use super::mpris::MprisBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicSources {
    pub local: bool,
    pub spotify: bool,
    pub browser: bool,
}

impl Default for MusicSources {
    fn default() -> Self {
        Self {
            local: true,
            spotify: true,
            browser: true,
        }
    }
}

impl MusicSources {
    fn path() -> std::path::PathBuf {
        crate::libraries::process::mbs_runtime_dir().join("mbs-music-sources.json")
    }

    pub fn load() -> Self {
        crate::libraries::process::read_json_capped(&Self::path(), 1024).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string(self) {
            let _ = crate::libraries::process::private_write_file(&Self::path(), json.as_bytes());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerKind {
    Mpd,
    Mpris(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressOutcome {
    Played,
    Paused,
}

#[derive(Debug, Default, Clone)]
pub struct MusicCycleState {
    pub(crate) next_idx: usize,
    pub(crate) last_playing: Option<PlayerKind>,
}

/// Pure state-machine step. Separated from the async `press` so
/// tests can exercise every branch without spawning D-Bus connections.
#[derive(Debug)]
pub enum Decision {
    Pause(PlayerKind, usize),
    Play(PlayerKind, usize),
    Nothing,
}

pub fn decide_step(
    available: &[PlayerKind],
    playing_set: &[PlayerKind],
    last_playing: Option<&PlayerKind>,
    next_idx: usize,
) -> Decision {
    if available.is_empty() {
        return Decision::Nothing;
    }
    let n = available.len();

    // Which to pause? Prefer last-playing (honor intent). Otherwise
    // the first in the playing set (probe order = MPD first, then
    // MPRIS registration order, which is also priority order).
    let to_pause = last_playing
        .filter(|p| playing_set.contains(p))
        .cloned()
        .or_else(|| playing_set.first().cloned());

    if let Some(p) = to_pause {
        let pause_idx = available.iter().position(|a| a == &p).unwrap_or(0);
        let new_next = (pause_idx + 1) % n;
        return Decision::Pause(p, new_next);
    }

    let target = &available[next_idx % n];
    Decision::Play(target.clone(), (next_idx + 1) % n)
}

pub async fn probe_all() -> Vec<(PlayerKind, bool)> {
    let mut out = Vec::new();
    let sources = MusicSources::load();

    if sources.local {
        if let Some(track) = MpdBackend::poll().await {
            out.push((PlayerKind::Mpd, track.is_playing));
        }
    }

    for bus in MprisBackend::list_players().await {
        let b = bus.to_lowercase();
        let is_spotify = b.contains("spotify");
        let is_browser = b.contains("firefox")
            || b.contains("chrome")
            || b.contains("chromium")
            || b.contains("brave")
            || b.contains("vivaldi");

        if is_spotify && !sources.spotify {
            continue;
        }
        if is_browser && !sources.browser {
            continue;
        }

        if let Some(track) = MprisBackend::poll_for(&bus).await {
            out.push((PlayerKind::Mpris(bus), track.is_playing));
        }
    }

    out
}

async fn dispatch_play(kind: &PlayerKind, play: bool) {
    match kind {
        PlayerKind::Mpd => {
            if play {
                MpdBackend::play().await
            } else {
                MpdBackend::pause().await
            }
        }
        PlayerKind::Mpris(bus) => MprisBackend::set_playing_for(bus, play).await,
    }
}

/// Perform one cycle press. Updates `state` in place. Returns what
/// was acted on (for logging / UI feedback), or None if no players
/// are available.
pub async fn press(state: &mut MusicCycleState) -> Option<(PlayerKind, PressOutcome)> {
    super::mpd::MpdBackend::clear_cache();
    let probed = probe_all().await;
    let available: Vec<PlayerKind> = probed.iter().map(|(k, _)| k.clone()).collect();
    let playing: Vec<PlayerKind> = probed
        .iter()
        .filter(|(_, p)| *p)
        .map(|(k, _)| k.clone())
        .collect();

    let decision = decide_step(
        &available,
        &playing,
        state.last_playing.as_ref(),
        state.next_idx,
    );

    match decision {
        Decision::Nothing => {
            crate::dbg_log!("[music-cycle] no players available");
            None
        }
        Decision::Pause(target, new_next) => {
            crate::dbg_log!("[music-cycle] pausing {:?}", target);
            dispatch_play(&target, false).await;
            super::mpd::MpdBackend::clear_cache();
            state.last_playing = None;
            state.next_idx = new_next;
            Some((target, PressOutcome::Paused))
        }
        Decision::Play(target, new_next) => {
            crate::dbg_log!("[music-cycle] playing {:?}", target);
            dispatch_play(&target, true).await;
            super::mpd::MpdBackend::clear_cache();
            state.last_playing = Some(target.clone());
            state.next_idx = new_next;
            Some((target, PressOutcome::Played))
        }
    }
}

#[cfg(test)]
mod decide_tests {
    use super::{decide_step, Decision, PlayerKind};

    fn browser() -> PlayerKind {
        PlayerKind::Mpris("org.mpris.MediaPlayer2.firefox.instance_1".into())
    }
    fn mpd() -> PlayerKind {
        PlayerKind::Mpd
    }

    #[test]
    fn empty_available_returns_nothing() {
        assert!(matches!(decide_step(&[], &[], None, 0), Decision::Nothing));
    }

    #[test]
    fn nothing_playing_plays_next() {
        let avail = vec![browser(), mpd()];
        match decide_step(&avail, &[], None, 0) {
            Decision::Play(t, next) => {
                assert_eq!(t, browser());
                assert_eq!(next, 1);
            }
            _ => panic!("expected Play browser"),
        }
    }

    #[test]
    fn playing_pauses_last_targeted_first() {
        let avail = vec![browser(), mpd()];
        let playing = vec![browser(), mpd()]; // both playing (external)
        match decide_step(&avail, &playing, Some(&mpd()), 0) {
            Decision::Pause(t, next) => {
                // last = MPD wins over priority order
                assert_eq!(t, mpd());
                assert_eq!(next, 0); // MPD at idx 1, next = 0
            }
            _ => panic!("expected Pause mpd"),
        }
    }

    #[test]
    fn playing_without_last_falls_to_priority() {
        let avail = vec![browser(), mpd()];
        let playing = vec![browser()];
        match decide_step(&avail, &playing, None, 0) {
            Decision::Pause(t, next) => {
                assert_eq!(t, browser());
                assert_eq!(next, 1);
            }
            _ => panic!("expected Pause browser"),
        }
    }

    #[test]
    fn full_cycle_four_presses() {
        let avail = vec![browser(), mpd()];
        let mut next = 0;
        let mut last: Option<PlayerKind> = None;
        let mut playing: Vec<PlayerKind> = vec![];

        // Press 1: play browser
        match decide_step(&avail, &playing, last.as_ref(), next) {
            Decision::Play(t, n) => {
                assert_eq!(t, browser());
                playing.push(t.clone());
                last = Some(t);
                next = n;
            }
            _ => panic!("press 1 should play browser"),
        }
        // Press 2: pause browser
        match decide_step(&avail, &playing, last.as_ref(), next) {
            Decision::Pause(t, n) => {
                assert_eq!(t, browser());
                playing.clear();
                last = None;
                next = n;
            }
            _ => panic!("press 2 should pause browser"),
        }
        // Press 3: play mpd
        match decide_step(&avail, &playing, last.as_ref(), next) {
            Decision::Play(t, n) => {
                assert_eq!(t, mpd());
                playing.push(t.clone());
                last = Some(t);
                next = n;
            }
            _ => panic!("press 3 should play mpd"),
        }
        // Press 4: pause mpd
        match decide_step(&avail, &playing, last.as_ref(), next) {
            Decision::Pause(t, _) => {
                assert_eq!(t, mpd());
                playing.clear();
            }
            _ => panic!("press 4 should pause mpd"),
        }
    }

    #[test]
    fn external_interrupt_respects_reality() {
        // We last played browser, but user externally started MPD.
        // Playing = [MPD], last = browser. Should pause MPD, not
        // continue the abandoned plan.
        let avail = vec![browser(), mpd()];
        let playing = vec![mpd()];
        let last = Some(browser());
        match decide_step(&avail, &playing, last.as_ref(), 1) {
            Decision::Pause(t, next) => {
                // last (browser) not in playing set → use playing[0]
                assert_eq!(t, mpd());
                assert_eq!(next, 0);
            }
            _ => panic!("external MPD should be paused"),
        }
    }
}
