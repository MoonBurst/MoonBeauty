//! Top bar message dispatch.
//!
//! Extracted from `app/update.rs` (tracker item
//! `topbar-sub-extraction.md`, patch 3 of 3). Handles the entire
//! `TopBarMessage` enum:
//!
//! - `Action(CapsuleAction)` — every bar capsule click dispatches
//!   here. Delegates to `handle_capsule_action` (extracted in
//!   patch 2), which covers music control, audio mute/cycle, tray
//!   expand/activate/context-menu, GPU cycle, tooltip toggle, and
//!   overlay spawn.
//! - `Noop` — the no-operation message used by async Command
//!   completions (e.g. `Command::perform(async { ... }, |_|
//!   Message::TopBar(TopBarMessage::Noop))`). Falls through to
//!   `Command::none()`.

use super::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_top_bar(&mut self, msg: TopBarMessage) -> Command<Message> {
        moonbeauty::dbg_log!("[topbar-msg] {:?}", msg);
        if let TopBarMessage::Action(act) = msg {
            return self.handle_capsule_action(act);
        }
        Command::none()
    }
}
