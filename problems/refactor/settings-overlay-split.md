# Split `overlays/settings.rs` into a folder

**Effort:** 6–8h
**Subsystem:** settings UI
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/settings.rs` is ~3,707 lines. Six settings tabs plus a
profile inspector, content-rule editor, and static IPC command lookup
table all in one file.

Tab `view_*` functions (verify with `grep -n 'fn view_' src/overlays/settings.rs`):
`view_notifications`, `view_colors_styles`, `view_hardware`,
`view_bar_layout`, `view_overlays_windows`, `view_ipc_commands`.

Plus `SettingsOverlay` struct, `SettingsMessage` enum, `update()` handler,
and dozens of small helper functions.

## Desired state

    src/overlays/settings/
    ├── mod.rs               struct + Message enum + top-level update()
    ├── notifications.rs
    ├── colors_styles.rs
    ├── hardware.rs
    ├── bar_layout.rs
    ├── overlays_windows.rs
    ├── ipc_commands.rs
    └── profile.rs           friend-profile inspector subsection

Each submodule exposes `pub(super) fn view_<tab>(...)` and, if special-
case handling is needed, `pub(super) fn handle_<tab>(...)`. `mod.rs`
delegates.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: open `moonbeauty --settings`, click each tab. Verify every
  slider, toggle, and colour swatch still works.

## Out of scope

- Refactoring `SettingsMessage` — leave as one flat enum
- Reworking the theme preview
