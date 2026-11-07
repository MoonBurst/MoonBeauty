# Remaining `app/update.rs` extractions

**Effort:** 4–6h (excluding TopBar — see topbar-sub-extraction.md)
**Subsystem:** app/update
**Depends on:** —
**Good first issue:** no

## Current state

`src/app/update.rs` is ~1,085 lines. Eight domains already extracted
to `app/update/` (music, notifications, reminders, email, lockscreen,
clipboard, settings, overlays).

Still inline:
- `Message::TopBar(msg)` — ~600 lines (see topbar-sub-extraction.md)
- `Message::Launcher(msg)` — ~100 lines
- `Message::Annotator(msg)` — ~150 lines (has an `is_text_press`
  post-match check that MUST be preserved verbatim)
- `Message::DismissActiveOverlay` — ~50 lines (cross-overlay router)
- ~~Tray family~~ — *Extracted to `app/update/tray.rs`*
- Email sync — `TickEmailSync`, `EmailSyncedWithDiff` (~90 lines)
- Background ticks — `TickHardware`, `MetricsCollected`, `TickWeather`,
  `WeatherLoaded`, `IdleTick` (~30 lines)
- Remaining `IpcReceived` arms (~120 lines)

## Desired state

Extract in this order (each independently mergeable, each a commit):

1. `Message::Launcher` → `app/update/launcher.rs`
2. `Message::Annotator` → `app/update/annotator.rs`
3. ~~Tray family → `app/update/tray.rs`~~ *(Done)*
4. Email sync → `app/update/email_sync.rs`
5. Background ticks → `app/update/hardware.rs`
6. Remaining IPC arms → `app/update/ipc.rs`
7. `Message::DismissActiveOverlay` → `app/update/dismiss.rs`
8. `Message::TopBar` — separate, see topbar-sub-extraction.md

## Pattern

Same as the existing 8 extractions. Each becomes:

    pub(crate) fn handle_<name>(&mut self, msg: <Type>) -> Command<Message> { ... }

and `update.rs` becomes:

    Message::<Name>(msg) => return self.handle_<name>(msg),

## Verification (per extraction)

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: exercise the specific feature (open the overlay, press keys,
  verify behavior unchanged)

## Caution

`Message::Annotator` has a `let is_text_press = matches!(msg,
AnnotatorMessage::Pressed(_)) && self.annotator.tool == Tool::Text;`
captured before the match and checked after. If the extraction
reorders this, text-mode annotation silently breaks. Copy verbatim.
