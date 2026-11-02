# Split `overlays/email.rs` into a folder

**Effort:** 3–4h
**Subsystem:** email UI
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/email.rs` is ~1,383 lines. Contains the struct + Message
enum + `update()`, and `view()` with three inline branches (login, main
3-pane, compose) plus reading mode.

## Desired state

    src/overlays/email/
    ├── mod.rs           EmailOverlay struct + Message enum + update()
    ├── folders.rs       folder sidebar rendering
    ├── list.rs          envelope list rendering
    ├── reader.rs        message body + attachment rendering
    ├── compose.rs       composer + autocomplete
    └── login.rs         login form + sops integration

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: `moonbeauty --toggle email`:
  - Login form appears if not configured
  - Folders, list, preview render
  - Compose: 'n' to open, Tab cycles To/Subject/Body
  - 's' stars, 'd' deletes, 'c' saves contact
