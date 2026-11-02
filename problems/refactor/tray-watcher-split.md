# Split `bar/tray/watcher.rs` into a folder

**Effort:** 2h
**Subsystem:** tray
**Depends on:** —
**Good first issue:** no

## Current state

`src/bar/tray/watcher.rs` is ~790 lines. Contains the D-Bus watcher
server, the SNI client enumeration, icon resolution, dbusmenu client,
and the tray item structs.

## Desired state

    src/bar/tray/watcher/
    ├── mod.rs           re-exports + TrayItem/TrayMenuItem structs
    ├── host.rs          TrayWatcher + spawn() (D-Bus server side)
    ├── client.rs        fetch_tray_items, activate_tray_item
    ├── icons.rs         resolve_icon, save_pixmap
    └── menu.rs          get_menu_tree, trigger_menu_item

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: click tray capsule in bar; icons appear for running tray apps
  (Steam, Discord, etc.). Right-click icon → context menu opens.
  Left-click → activates app.
