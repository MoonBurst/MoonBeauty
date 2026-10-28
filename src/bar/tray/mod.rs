//! System tray (StatusNotifierWatcher host + popup + context menu).
//!
//! Contents:
//! - `watcher` : owns org.kde.StatusNotifierWatcher, enumerates registered
//!               SNI items, resolves icons, activates items, fetches menus
//! - `popup`   : the left-click popup (grid of tray icons)
//! - `context` : the right-click context menu (dbusmenu rendering)
//!
//! Both `popup` and `context` are standalone layer-shell processes spawned
//! from `main.rs` when the bar's tray capsule is clicked.
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

pub mod context;
pub mod popup;
pub mod watcher;

pub use watcher::{
    activate_tray_item, fetch_tray_items, get_menu_tree, spawn as spawn_tray_watcher,
    trigger_menu_item, TrayItem, TrayMenuItem,
};
