# Split `libraries/config/settings.rs` into a folder

**Effort:** 1–2h
**Subsystem:** config
**Depends on:** —
**Good first issue:** yes

## Current state

`src/libraries/config/settings.rs` is ~841 lines. Contains
`ShellSettings` struct (~50 fields) + Default impl, all related type
definitions, `load()` / `save()` / `config_path()`, friend-profile
load/save, and dozens of `default_*` functions.

## Desired state

    src/libraries/config/settings/
    ├── mod.rs           ShellSettings struct + Default + load/save
    ├── types.rs         ScreenshotSaveFormat, GpuThresholds, WindowDim,
    │                    ContentRule, FriendProfile, UnifiedCheck,
    │                    UnifiedToggle, UnifiedItem
    ├── defaults.rs      default_* functions
    └── profiles.rs      friend-profile load/save

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: snapshot `~/.config/moonbeauty/settings.json` before; after
  the split, open Settings overlay, verify your values are intact. Make
  a change and verify the JSON updates.
