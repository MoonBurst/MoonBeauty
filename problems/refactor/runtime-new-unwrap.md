# Replace `Runtime::new().unwrap()` in update handlers

**Effort:** 30–60 min per site
**Subsystem:** app/update
**Depends on:** —
**Good first issue:** no

## Current state

Three sites in `app/update/` call `tokio::runtime::Runtime::new().unwrap()`
inside `update()`, which is synchronous. This works only because iced
hands control back immediately after `update()` returns. Each site
block-on's a `wl-copy` subprocess so the shell exits with the value on
the clipboard.

Sites (verify with `grep -rn 'Runtime::new' src/app/update/`):
- Calc `CopyResult` — writes result via `wl-copy`
- Pass `CopyKey` — writes password via `pass -c`
- Annotator `Save` — reveal-mode branch, saves the reveal image

## Desired state

Convert each to `Command::perform(async { ... }, |_| Message::...)`.
The iced runtime drives the future on its own executor, no nested
runtime needed.

## Caution

**Do not move these into an async context.** They are called from a
synchronous `update()` and rely on iced returning control immediately.
Moving the `block_on` into a `tokio::spawn` or `Command::perform`
re-broke this in earlier attempts. Do one site per PR.

## Verification (per site)

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: exercise the specific copy/save flow, verify the value lands
  in `wl-paste`. Close overlay. Verify it does NOT deadlock.
