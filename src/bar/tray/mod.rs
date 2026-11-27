//! System tray (StatusNotifierWatcher host + context menu).
//!
//! Contents:
//! - `watcher` : owns org.kde.StatusNotifierWatcher, enumerates registered
//!               SNI items, resolves icons, activates items, fetches menus
//! - `context` : the right-click context menu (dbusmenu rendering)
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

pub mod context;
pub mod watcher;

pub use watcher::{
    activate_tray_item, fetch_tray_items, get_menu_tree, spawn as spawn_tray_watcher,
    trigger_menu_item, TrayItem, TrayMenuItem,
};

pub mod cli;
