# Remaining Tasks & Backlog

This file tracks the outstanding technical debt, minor bugs, and maintenance tasks remaining after the major refactor and bugfixing session on 2026-10-27.

## 1. Dependency Upgrades (Quick Wins)

- [ ] **Upgrade `imap` Crate to 3.0**
  - *Context:* The `imap = "2.4"` dependency triggers a `future-incompat` warning from Cargo (`imap-proto v0.10.2` uses deprecated Rust syntax).
  - *Action:* Bump to `imap = "3.0"` in `Cargo.toml`. This is an API-breaking change. `session.fetch`, `session.store`, and mailbox methods will need updating in `overlays/email/client.rs`.
  - *Priority:* Low (zero functional change, just silences a compiler warning).

## 2. Technical Debt (Architectural)

- [ ] **Remove Global Mutex for Capsule Widths (Problem #23)**
  - *Context:* `bar/capsules.rs` uses a global `static Mutex<HashMap>` to record capsule widths so tooltips know where to spawn.
  - *Action:* Return widths from the render function and store them in `MoonBeautyShellApp` state.
  - *Priority:* Low. It works, but it's fragile if the render order ever changes.

## 3. Deferred / Known Limitations

- [ ] **Lockscreen TTY-Switch Unlock**
  - *Context:* Switching to TTY 2 and back exposes the desktop because Sway detaches the Wayland surface, causing a C-level segfault in the main bar process (bypassing Rust's panic handler).
  - *Action:* Requires migrating to `ext-session-lock-v1` (the protocol used by `swaylock`). This is a ~400 line project.
  - *Interim fix:* Use `swaylock` as your primary lockscreen.
  - *Documentation:* See `things I can't fix/lockscreen-tty-switch.md`.

---

## 🏆 Completed in the 2026-10-27 Session

For historical record, here is what was accomplished:
- **App Extraction:** `main.rs` reduced from 3,700 lines to ~200 lines (`update`, `view`, `subscription`, `new_app` moved to `src/app/`).
- **Settings Refactor:** `settings.rs` view logic split into 5 distinct `view_*` functions; tabs consolidated from 6 to 5.
- **Feature Modules:** Extracted `lockscreen`, `notifications`, `screenshot`, `email`, `tray`, `weather`, and `unified` into dedicated module directories.
- **Bug Fixes:**
  - Notification FIFO ordering and z-order targets.
  - Tooltip override height now actually applies at spawn time.
  - Tray animation now completes fully via a dedicated subscription timer.
  - Steam icon re-encoded via `image` crate (fixing ImageMagick silent failures).
  - Duplicate alarms and RAM tooltip hijacking fixed.
- **Testing:** Added 7 unit tests for lockscreen keyboard capture.
