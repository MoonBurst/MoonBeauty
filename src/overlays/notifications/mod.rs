//! Notifications overlay — D-Bus server, toast rendering, history panel, DND.
//!
//! Three pieces:
//! - `ui`: NotificationOverlay (toasts + history + DND held queue), NotifMessage,
//!   view_toasts, view_history, push_notification, tick_toasts
//! - `dbus`: org.freedesktop.Notifications D-Bus server, forwards incoming
//!   notifications over a channel to the UI process
//! - (implicit) re-exports: overlays/mod.rs pulls NotifMessage + NotificationOverlay
//!   from here for the main dispatcher

pub mod dbus;
pub mod ui;

pub use dbus::{emit_action_invoked, spawn, IncomingNotification};
pub use ui::{
    dnd_active, held_notifs_count, load_held_notifs, read_active_count, save_held_notifs,
    take_held_notifs, toggle_dnd, write_active_count, NotifMessage, NotificationOverlay,
    NotificationToast,
};
