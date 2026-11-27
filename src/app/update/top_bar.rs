//! Top bar message dispatch.
//!
//! Handles the `TopBarMessage` enum:
//! - `Action(CapsuleAction)` — bar capsule clicks, delegated to `handle_capsule_action`.
//! - `Noop` — no-op completion message for async commands.

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
