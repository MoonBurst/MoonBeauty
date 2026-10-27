//! Notifications overlay — D-Bus server, toast rendering, history panel, DND.
//!
//! Three pieces:
//! - `ui`: NotificationOverlay (toasts + history + DND held queue), NotifMessage,
//!   view_toasts, view_history, push_notification, tick_toasts
//! - `dbus`: org.freedesktop.Notifications D-Bus server, forwards incoming
//!   notifications over a channel to the UI process
//! - (implicit) re-exports: overlays/mod.rs pulls NotifMessage + NotificationOverlay
//!   from here for the main dispatcher

pub mod ui;
pub mod dbus;

pub use ui::{
    NotificationOverlay, NotificationToast, NotifMessage,
    dnd_active, toggle_dnd,
    held_notifs_count, take_held_notifs,
    load_held_notifs, save_held_notifs,
    write_active_count, read_active_count,
};
pub use dbus::{spawn, emit_action_invoked, IncomingNotification};
