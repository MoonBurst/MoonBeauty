# Deduplicate `magick_or_convert()`

**Effort:** 10 min
**Subsystem:** screenshot
**Depends on:** —
**Good first issue:** yes

## Current state

Two identical ~10-line functions in `src/overlays/screenshot/annotator.rs`
(`pub fn`) and `src/overlays/screenshot/watermark.rs` (private `fn`):

    fn magick_or_convert() -> Option<&'static str> {
        if Command::new("magick").arg("--version")... { Some("magick") }
        else if Command::new("convert").arg("--version")... { Some("convert") }
        else { None }
    }

## Desired state

Move to `src/overlays/screenshot/mod.rs` as
`pub(crate) fn magick_or_convert() -> Option<&'static str>`,
delete both copies, add `use` at top of the two callers.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: save an annotated screenshot (Super+Shift+S → draw → Enter).
  Verify the file lands in `~/Pictures/Screenshots/`.
