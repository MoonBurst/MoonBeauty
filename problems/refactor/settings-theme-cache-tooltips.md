# Cache `ShellSettings` / `ShellTheme` in weather and tray tooltips

**Effort:** 15-20m
**Subsystem:** tooltips
**Depends on:** —
**Good first issue:** yes

## Current state

Two tooltip processes call `ShellSettings::load()` at the top of
`view()`:

- `src/bar/weather/tooltip.rs::App::view`
- `src/bar/tray/popup.rs::App::view`

`view()` runs at 60fps during the drop/expand animation, so each
animated tooltip does ~60 `read_to_string` + `serde_json::from_str`
per second during its ~400ms entrance, plus a fresh
`ShellTheme::from_settings()` allocation each frame.

The other two tooltips (`system_tooltip.rs`, `info_tooltip.rs`) do
this correctly — they cache `settings` and `theme` as struct fields
at `new_app()` and reference `self.settings` / `self.theme` in `view()`.

## Desired state

Same pattern as `system_tooltip.rs`:

    pub struct App {
        // ... existing
        settings: ShellSettings,
        theme: ShellTheme,
    }

    fn new_app() -> (Self, Task<Message>) {
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);
        // ...
    }

    fn view(&self) -> Element<'_, Message> {
        let settings = &self.settings;
        let theme = &self.theme;
        // ...
    }

Behavior change: a settings change while a tooltip is open won't be
picked up until the tooltip is reopened. That matches the existing
behavior of `system_tooltip.rs` / `info_tooltip.rs`; no user surprise.

## Verification

- `cargo check --locked`
- `cargo test --locked --lib` (76 passing)
- Manual: open weather tooltip, open tray popup, verify animation smooth
- Manual: `ls -la /proc/<weather-tooltip-pid>/fd` during animation —
  should NOT show repeated opens of `settings.json`

## Out of scope

Also caching in `bar/weather/client.rs` or `bar/tray/watcher.rs` — those
run in the bar process, not per-tooltip.
