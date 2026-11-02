# Replace `TryInto<LayershellCustomActionWithId>` with explicit enum

**Effort:** 2–4h
**Subsystem:** app / layer-shell integration
**Depends on:** —
**Good first issue:** no

## Current state

`Message` implements `TryInto<LayershellCustomActionWithId, Error = Message>`.
Only `ResizeNotifSurface` and `SetToastInputRegion` return `Ok(...)`.
All other variants return `Err(self)`, which is how `update()` returns
`Command::none()` for those.

Downsides:
- `update.rs` has stub arms for the two `Ok` variants that exist only
  for exhaustiveness — they have `eprintln!` warnings to remind readers
  not to touch them.
- Adding a new layer-shell action means editing `Message`, `update()`,
  and the `TryInto` impl.
- The `Debug` impl on `LayershellCustomAction` is deliberately lossy for
  the `ActionCallback` variant, so debugging is awkward.

## Desired state

Introduce:

    pub enum LayerAction {
        Size(u32, u32),
        InputRegion(Vec<(i32, i32, i32, i32)>),
    }

    impl Message {
        pub fn into_layer_action(self) -> Result<LayerAction, Message> { ... }
    }

Then `update()` returns `Command<Message>` and the iced runtime converts
`LayerAction → LayershellCustomAction`.

Two steps, both testable:
1. Add `LayerAction` + `into_layer_action` alongside the existing
   `TryInto` impl (both exist during migration)
2. Delete the `TryInto` impl once no caller needs it

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: open a notification toast, verify surface resizes and the
  click target outside the toast is transparent (clicks pass through)
