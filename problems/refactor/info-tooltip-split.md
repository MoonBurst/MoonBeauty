# Split `overlays/info_tooltip.rs` into a folder

**Effort:** 2–4h
**Subsystem:** bar tooltips
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/info_tooltip.rs` is ~2,241 lines. Six tooltip modes
crammed into one `match &self.data` with inline rendering per mode:
Calendar, Music, Unified, Net, Clock, Custom.

## Desired state

    src/overlays/info_tooltip/
    ├── mod.rs         InfoData, Message, App struct, run()
    ├── calendar.rs
    ├── music.rs
    ├── unified.rs
    ├── net.rs
    ├── clock.rs
    └── custom.rs

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: click each of these capsules in the bar, verify the tooltip
  opens with the right content: calendar, music, unified, net, clock,
  custom.
