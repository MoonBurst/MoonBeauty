# Split `overlays/notifications/ui.rs` into a folder

**Effort:** 2–4h
**Subsystem:** notifications
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/notifications/ui.rs` is ~1,961 lines. Four concerns:
toast rendering + stack animation, history panel + scroll math, icon
resolution, DND flag + held-notifs queue.

## Desired state

    src/overlays/notifications/ui/
    ├── mod.rs            NotificationOverlay struct + update()
    ├── toasts.rs         toast rendering, stack animation
    ├── history.rs        history panel + scroll math
    ├── icons.rs          icon resolution
    └── dnd.rs            DND flag, held notifs

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: `notify-send Test Test` → toast appears. Super+h → history
  shows past entries. Click notification capsule → DND toggles.
  Delete a history entry → persists across reopen.

## Out of scope

- Reworking the D-Bus server (`notifications/dbus.rs`, separate file)
- Changing toast animation timing
