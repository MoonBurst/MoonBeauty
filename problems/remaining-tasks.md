# Remaining Tasks & Backlog

This file tracks the outstanding technical debt, minor bugs, and maintenance tasks remaining after the major refactor and bugfixing session on 2026-10-27.

## 1. Verification Leftovers (Quick Wins)

- [ ] **Verify Steam Icon Fix**
  - *Context:* We updated `bar/tray/watcher.rs` to run `rm -f base_dest` before ImageMagick to fix the "magick produced empty output" error (caused by trying to overwrite a read-only cache file). 
  - *Action:* Restart the bar, click the tray, and verify `steam_tray_mono.png` renders the Steam logo instead of a fallback letter.
- [ ] **Run Final Clippy Pass**
  - *Context:* Recent patches (lockscreen pause detection, tray animation, settings tab reorg) may have left unused imports or variables.
  - *Action:* Run `cargo fix --allow-dirty --allow-staged` and `cargo clippy --fix`.

## 2. Technical Debt (Architectural)

- [ ] **Extract `MoonBeautyShellApp::update()` and `view()` (Problems #21 / 10b-10g)**
  - *Context:* `main.rs` is still ~2,500 lines. We moved the type definitions to `src/app.rs`, but the massive 1,500-line `update()` match statement and the `view()` function still live in `main.rs`.
  - *Action:* Move `impl MoonBeautyShellApp` blocks into `src/app/update.rs`, `src/app/view.rs`, etc.
  - *Priority:* Low (unless you start adding many new features and find `main.rs` unwieldy again).

- [ ] **Split `settings.rs` View Logic (Problem #22)**
  - *Context:* `settings.rs` is still ~2,000 lines because the `view()` function has a massive `match` statement with every tab's UI inlined.
  - *Action:* Refactor into `fn view_notifications()`, `fn view_colors_styles()`, etc., and call them from `view()`.
  - *Priority:* Low.

- [ ] **Remove Global Mutex for Capsule Widths (Problem #23)**
  - *Context:* `bar/capsules.rs` uses a global `static Mutex<HashMap>` to record capsule widths so tooltips know where to spawn.
  - *Action:* Return widths from the render function and store them in `MoonBeautyShellApp` state.
  - *Priority:* Low. It works, but it's fragile if the render order ever changes.

## 3. Dependency Upgrades

- [ ] **Upgrade `imap` Crate**
  - *Context:* The `imap = "2.4"` dependency triggers a `future-incompat` warning from Cargo (`imap-proto v0.10.2` uses deprecated Rust syntax).
  - *Action:* Bump to `imap = "3.0"`. This is an API-breaking change. `session.fetch`, `session.store`, and mailbox methods will need updating in `overlays/email/client.rs`.
  - *Priority:* Low (zero functional change, just silences a compiler warning).

## 4. Deferred / Known Limitations

- [ ] **Lockscreen TTY-Switch Unlock**
  - *Context:* Switching to TTY 2 and back exposes the desktop because Sway detaches the Wayland surface, causing a C-level segfault in the main bar process (bypassing Rust's panic handler).
  - *Action:* Requires migrating to `ext-session-lock-v1` (the protocol used by `swaylock`). This is a ~400 line project.
  - *Interim fix:* Use `swaylock` as your primary lockscreen.
  - *Documentation:* See `things I can't fix/lockscreen-tty-switch.md`.
