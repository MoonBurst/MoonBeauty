# Split `overlays/system_tooltip.rs` into a folder

**Effort:** 2–3h
**Subsystem:** bar tooltips
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/system_tooltip.rs` is ~1,205 lines. Three tooltip modes
(CPU/GPU/RAM) inline in `build_content()`, plus shared process row
rendering, `slanted_row` insets helper, and `KillButtonShape` canvas.

## Desired state

    src/overlays/system_tooltip/
    ├── mod.rs         App struct + Message enum + update()
    ├── cpu.rs
    ├── gpu.rs
    ├── ram.rs
    └── render.rs      render_proc_row(), slanted_row(), KillButtonShape

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: click CPU, GPU, RAM capsules; each tooltip opens with correct
  metrics and process list. Kill button works.

## Out of scope

- Refactoring the `slanted_row` insets math — currently correct, don't
  touch (see `docs/REFACTOR.md`)
