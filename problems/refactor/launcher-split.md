# Split `overlays/launcher.rs` into a folder

**Effort:** 2–4h
**Subsystem:** launcher
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/launcher.rs` is ~1,708 lines. Contains XDG .desktop
scanning, Exec parsing, unit conversion engine (7 categories),
dictionary integration, password-store integration, subprocess launch,
and the search-input + result-list view.

## Desired state

    src/overlays/launcher/
    ├── mod.rs           AppLauncher struct + Message enum + view()
    ├── scan.rs          scan_desktop_files(), routing_alias()
    ├── exec.rs          parse_exec(), strip_field_codes(), execute_app()
    ├── convert.rs       unit conversion engine
    ├── dict.rs          dict: prefix handling
    └── pass.rs          pass: prefix handling

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing — parse_exec tests move to
  `launcher/exec.rs`, should still run)
- Manual: Super+d opens; "firefox" → matches .desktop; "2+2" → math;
  "10usd" → currency; "def cat" → dictionary; Enter launches.
