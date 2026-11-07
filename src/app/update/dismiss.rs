use super::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_dismiss_active_overlay(&mut self) -> Command<Message> {
        // Todo edit mode: Escape cancels the edit rather than
        // closing the overlay.
        if matches!(self.active_overlay, Some(OverlayTarget::Todo))
            && self.todo.editing_task_id.is_some()
        {
            self.todo
                .update(moonbeauty::overlays::todo::TodoMessage::CancelEdit);
            return Command::none();
        }
        // Reminders: Escape clears the current input field, then the
        // list selection, then finally dismisses. Prevents a mistyped
        // reminder from wiping out the whole overlay on Esc.
        if matches!(self.active_overlay, Some(OverlayTarget::Reminders)) {
            if !self.reminders.task_text.is_empty() {
                self.reminders.task_text.clear();
                return Command::none();
            }
            if !self.reminders.when_text.is_empty() {
                self.reminders.when_text.clear();
                return Command::none();
            }
            if self.reminders.selected_idx.is_some() {
                self.reminders.selected_idx = None;
                return Command::none();
            }
        }
        // Persist if we're closing the Settings overlay via Esc.
        if matches!(self.active_overlay, Some(OverlayTarget::Settings)) {
            let _ = self.settings.save();
        }
        if self.is_standalone {
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(200));
                std::process::exit(0);
            });
            return Command::none();
        }
        self.active_overlay = None;

        Command::none()
    }
}
