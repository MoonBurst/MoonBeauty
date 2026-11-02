# Sub-extract `Message::TopBar`

**Effort:** 2h across 3 patches
**Subsystem:** bar actions
**Depends on:** —
**Good first issue:** no

## Current state

`Message::TopBar(msg)` in `app/update.rs` is ~600 lines (40% of the
remaining file). The `OpenOverlay` arm alone is ~250 lines of capsule
x-position math, tooltip spawn env-var setup, `SpawnRequest`
construction, spawn call.

## Desired state

Three sequential patches, each independently verifiable.

### Patch 1 of 3 — `spawn_tooltip()` helper

    impl MoonBeautyShellApp {
        pub(crate) fn spawn_tooltip(
            &mut self,
            id: &str,
            tooltip_w: u32, tooltip_h: u32,
            capsule_cx: i32, cap_w: f32, screen_w: u32,
            slant_l: &str, slant_r: &str, grow_right: bool,
            bias_l: i32, bias_r: i32,
            output: Option<&str>,
        ) -> Command<Message> { ... }
    }

Verify: `moonbeauty --toggle cpu` opens the CPU tooltip.

### Patch 2 of 3 — `handle_capsule_action()`

    impl MoonBeautyShellApp {
        pub(crate) fn handle_capsule_action(
            &mut self,
            act: CapsuleAction,
        ) -> Command<Message> { ... }
    }

`Message::TopBar` becomes:

    Message::TopBar(TopBarMessage::Action(act)) => return self.handle_capsule_action(act),

Verify: each capsule click works (music play/pause, audio mute,
notification DND, tray expand, GPU cycle).

### Patch 3 of 3 — `app/update/top_bar.rs`

    Message::TopBar(msg) => return self.handle_top_bar(msg),

Verify: same as Patch 2 plus Ctrl+U (open in browser), Ctrl+click
(tooltip toggle-off).

## Verification (all patches)

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: exercise every capsule action listed above
