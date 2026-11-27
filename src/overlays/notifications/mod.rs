//! Notifications overlay — D-Bus server, toast rendering, history panel, DND.
//!
//! Pieces:
//! - `ui`: NotificationOverlay (toasts + history + DND held queue), NotifMessage,
//!   push_notification, tick_toasts, state updates
//! - `view`: toast deck and history drawer rendering + row geometry
//! - `dbus`: org.freedesktop.Notifications D-Bus server, forwards incoming
//!   notifications over a channel to the UI process
//! - `state`: unified cross-process runtime state and command IPC
//! - `icon_resolve`: desktop-entry, theme icon, and avatar resolution
//! - `tts`: notification text-to-speech readout formatting and dispatch

pub mod dbus;
pub mod icon_resolve;
pub mod state;
pub mod tts;
pub mod ui;
pub mod view;

pub use dbus::{
    emit_action_invoked, emit_click_handshake, emit_notification_closed, spawn,
    IncomingNotification,
};
pub use state::{
    dnd_active, held_notifs_count, load_held_notifs, read_active_count, read_bar_status,
    save_held_notifs, take_held_notifs, toggle_dnd, write_active_count, NotifRuntimeCmd,
    NotifRuntimeState, NotifStateReader,
};
pub use ui::{NotifMessage, NotificationOverlay, NotificationToast};

pub mod cli;
