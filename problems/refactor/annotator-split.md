# Split `overlays/screenshot/annotator.rs` into a folder

**Effort:** 2–4h
**Subsystem:** annotator
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/screenshot/annotator.rs` is ~1,601 lines. Contains 7
annotation tools with canvas rendering + hit-testing, ImageMagick
compose, JXL encoding, multi-recipient save pipeline, and the full
toolbar/canvas/status-bar view.

## Desired state

    src/overlays/screenshot/annotator/
    ├── mod.rs           AnnotatorOverlay struct + Message + update()
    ├── canvas.rs        OverlayCanvas, draw_ann, hit-testing
    ├── tools.rs         Tool enum + per-tool rendering helpers
    ├── compose.rs       compose_annotations()
    ├── save.rs          save_annotated_image(), png_to_jxl*
    └── ui.rs            view() — toolbar, canvas stack, status bar

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: `moonbeauty --screenshot --annotate`, draw with each of the 7
  tools (Rect/Ellipse/Arrow/Pen/Text/Counter/Redact), save with Enter.
  Verify output file appears and is readable.
