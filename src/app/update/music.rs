//! Music-related update handlers, extracted from `app/update.rs`.
//!
//! Two entry points:
//!   `music_*` — called from `Message::TopBar(Action(CapsuleAction::Music*))`
//!   `music_*_ipc` — called from `Message::IpcReceived(IpcCommand::Music*)`
//!
//! Both paths run identical async bodies; the only difference is the
//! completion Message (`TopBarMessage::Noop` vs `Message::TickHardware`).

#![allow(clippy::collapsible_match)]

use crate::app::{Message, MoonBeautyShellApp};

use iced::Task as Command;

use moonbeauty::bar::TopBarMessage;
use moonbeauty::libraries::backend::music::{
    MusicCycleState, MpdBackend, MprisBackend, TrackSource, poll_track,
};

impl MoonBeautyShellApp {
    // ── Bar capsule paths ─────────────────────────────────────────────

    pub(crate) fn music_toggle_play_pause(&self) -> Command<Message> {
        Command::perform(toggle_play_pause_async(), |_| {
            Message::TopBar(TopBarMessage::Noop)
        })
    }

    pub(crate) fn music_next_track(&self) -> Command<Message> {
        Command::perform(next_track_async(), |_| Message::TopBar(TopBarMessage::Noop))
    }

    pub(crate) fn music_previous_track(&self) -> Command<Message> {
        Command::perform(previous_track_async(), |_| {
            Message::TopBar(TopBarMessage::Noop)
        })
    }

    // ── IPC paths ─────────────────────────────────────────────────────

    pub(crate) fn music_cycle(&mut self) -> Command<Message> {
        if self.is_cycling {
            eprintln!("[music-cycle] ignored spam (already cycling)");
            return Command::none();
        }
        self.is_cycling = true;
        let mut state = self.music_cycle_state.clone();
        Command::perform(
            async move {
                moonbeauty::libraries::backend::music::press(&mut state).await;
                state
            },
            Message::MusicCycleDone,
        )
    }

    pub(crate) fn music_next_track_ipc(&self) -> Command<Message> {
        Command::perform(next_track_async(), |_| Message::TickHardware)
    }

    pub(crate) fn music_previous_track_ipc(&self) -> Command<Message> {
        Command::perform(previous_track_async(), |_| Message::TickHardware)
    }

    pub(crate) fn music_toggle_play_pause_ipc(&self) -> Command<Message> {
        Command::perform(toggle_play_pause_async(), |_| Message::TickHardware)
    }

    // ── Completion handler ────────────────────────────────────────────

    pub(crate) fn handle_music_cycle_done(
        &mut self,
        new_state: MusicCycleState,
    ) -> Command<Message> {
        self.is_cycling = false;
        self.music_cycle_state = new_state;
        Command::done(Message::TickHardware)
    }
}

// ── Shared async bodies ──────────────────────────────────────────────

async fn toggle_play_pause_async() {
    let (_, src) = poll_track().await;
    match src {
        TrackSource::Mpd => MpdBackend::toggle_play_pause().await,
        TrackSource::Mpris => MprisBackend::toggle_play_pause().await,
        TrackSource::None => {}
    }
}

async fn next_track_async() {
    let (_, src) = poll_track().await;
    match src {
        TrackSource::Mpd => MpdBackend::next().await,
        TrackSource::Mpris => MprisBackend::next().await,
        TrackSource::None => {}
    }
}

async fn previous_track_async() {
    let (_, src) = poll_track().await;
    match src {
        TrackSource::Mpd => MpdBackend::previous().await,
        TrackSource::Mpris => MprisBackend::previous().await,
        TrackSource::None => {}
    }
}
