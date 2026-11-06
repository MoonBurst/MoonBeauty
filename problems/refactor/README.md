# Refactor queue

Scoped, independently-mergeable structural work items. One problem per
file.

**Verification for every item:**
- `cargo check --locked` — zero warnings
- `cargo test --locked --lib` — 76 passing
- Manual test after `systemctl --user restart moonbeauty`

**Effort** is wall-clock for an experienced Rust/Iced dev.
First-time contributors: multiply by 2-3×.

**Depends on:** is a hard block. Do not start an item whose dependency
isn't done. Everything else is independent.

**Good first issue:** yes = safe entry point, no prior knowledge needed.

---

## Bugs / gaps

- [try-into-pattern.md](try-into-pattern.md) — 2-4h — `TryInto<LayershellCustomActionWithId>` is opaque
- [lockscreen-integration-tests.md](lockscreen-integration-tests.md) — 2-3h — no automated coverage for coordinator pure fns and shutdown

## File splits

Sizes are current line counts.

- [settings-overlay-split.md](settings-overlay-split.md) — 6-8h — 3,707 lines
- [info-tooltip-split.md](info-tooltip-split.md) — 2-4h — 2,241 lines
- [notifications-ui-split.md](notifications-ui-split.md) — 2-4h — 1,961 lines
- [launcher-split.md](launcher-split.md) — 2-4h — 1,708 lines
- [annotator-split.md](annotator-split.md) — 2-4h — 1,601 lines
- [email-client-split.md](email-client-split.md) — 4-6h — 1,415 lines (partial: types/validation/secure_fs already out)
- [email-overlay-split.md](email-overlay-split.md) — 3-4h — 1,383 lines
- [system-backend-split.md](system-backend-split.md) — 3-4h — 1,286 lines
- [system-tooltip-split.md](system-tooltip-split.md) — 2-3h — 1,205 lines
- [lockscreen-coordinator-split.md](lockscreen-coordinator-split.md) — 2-3h — 1,009 lines
- [config-settings-split.md](config-settings-split.md) — 1-2h — 841 lines
- [process-rs-split.md](process-rs-split.md) — 2h — 801 lines
- [tray-watcher-split.md](tray-watcher-split.md) — 2h — 790 lines

## Extraction

- [update-rs-remaining-extractions.md](update-rs-remaining-extractions.md) — 4-6h — remaining ~1,085 lines in `app/update.rs`
- [topbar-sub-extraction.md](topbar-sub-extraction.md) — 2h across 3 patches — the 600-line `Message::TopBar` arm

## How to pick one

1. Match your time (30m → dedup or a small extraction; 2h → smaller
   split; 4h+ → largest split)
2. Match your comfort level (Good first issue = yes → start here)
3. Comment on the corresponding GitHub issue before starting so you
   don't duplicate work
4. Verify before opening a PR — restart the bar and exercise the
   feature, not just `cargo test`

## See also

- [../README.md](../README.md) — maintainer bug notes
- [../../docs/REFACTOR.md](../../docs/REFACTOR.md) — prose on known
  sharp edges (still partly stale; refactor/ supersedes it)

## Recently fixed
- `runtime-new-unwrap` — 5 sites converted from `Runtime::new().unwrap()` (panics on rare failure) to `helpers::run_then_exit(future)` which logs the failure and exits anyway. Original `block_on` behavior preserved because these branches `exit(0)` before `update()` returns, so `Command::perform` cannot work here (documented in the tracker Caution). Reference pattern already used in `bar/tray/context.rs`.
- `magick-or-convert-dedup` — resolved. Single `pub(crate) fn` in `overlays/screenshot/mod.rs`, imported by both `annotator.rs` and `watermark.rs`. Was two duplicate copies when the tracker item was filed.
- `tooltip-zombie-reap` — `Message::IdleTick` now calls `try_wait()` on `weather_tooltip_child` and clears the handle when the child has exited. Self-terminating tooltips (AUTO_MS timeout, dismissal) no longer leave `<defunct>` children accumulating over hours of use.
- `zombie-children` — replaced untracked `.spawn()` calls with `spawn_and_reap()` for notif-surface, wl-paste, and restart-bar.
- `socket-path-mismatch` — IPC socket renamed from `moon-beauty-shell.sock` to `moonbeauty.sock`.
