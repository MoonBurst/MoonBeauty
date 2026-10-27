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

pub mod watcher;
pub mod popup;
pub mod context;

pub use watcher::{
    TrayItem, TrayMenuItem,
    fetch_tray_items, activate_tray_item,
    get_menu_tree, trigger_menu_item,
    spawn as spawn_tray_watcher,
};
