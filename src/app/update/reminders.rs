//! Reminders-related update handlers, extracted from `app/update.rs`.
//!
//! Covers:
//!   - `Message::Reminders(msg)` — ReminderMessage dispatch
//!   - `Message::TickClock` reminder-alert block — background one-shot
//!     that fires when a reminder's `target_epoch` passes

#![allow(clippy::collapsible_match)]

use super::*;

use moonbeauty::overlays::reminders::{ReminderMessage, RemindersOverlay};

impl MoonBeautyShellApp {
    pub(crate) fn handle_reminders(&mut self, msg: ReminderMessage) -> Command<Message> {
        match msg {
            ReminderMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
                Command::none()
            }
            ReminderMessage::InitialFocus => iced::widget::operation::focus(
                moonbeauty::overlays::reminders::REMINDER_TASK_ID,
            )
            .map(Message::Reminders),
            ReminderMessage::Refocus => iced::widget::operation::focus(
                moonbeauty::overlays::reminders::REMINDER_WHEN_ID,
            )
            .map(Message::Reminders),
            ReminderMessage::FocusNextField => {
                self.reminders.cycle_focus_forward();
                let target = if self.reminders.active_field == 0 {
                    moonbeauty::overlays::reminders::REMINDER_TASK_ID
                } else {
                    moonbeauty::overlays::reminders::REMINDER_WHEN_ID
                };
                iced::widget::operation::focus(target).map(Message::Reminders)
            }
            ReminderMessage::FocusPrevField => {
                self.reminders.cycle_focus_backward();
                let target = if self.reminders.active_field == 0 {
                    moonbeauty::overlays::reminders::REMINDER_TASK_ID
                } else {
                    moonbeauty::overlays::reminders::REMINDER_WHEN_ID
                };
                iced::widget::operation::focus(target).map(Message::Reminders)
            }
            ReminderMessage::ScrollUp
            | ReminderMessage::ScrollDown
            | ReminderMessage::SelectFirst
            | ReminderMessage::SelectLast
            | ReminderMessage::DeleteSelected => {
                self.reminders.update(msg);
                iced::widget::operation::scroll_to(
                    iced::widget::Id::new("reminders_list"),
                    iced::widget::operation::AbsoluteOffset {
                        x: 0.0,
                        y: self.reminders.scroll_y,
                    },
                )
            }
            _ => {
                self.reminders.update(msg);
                Command::none()
            }
        }
    }

    /// Fire notifications + alarm sounds for any reminders whose
    /// `target_epoch` has passed and which haven't fired yet.
    ///
    /// **Only the primary bar process fires reminders.**  The standalone
    /// reminders overlay and the notification surface also run TickClock
    /// every second; without the guard both see `alerted = false`
    /// simultaneously and each fires a notify-send + writes `alerted = true`,
    /// producing duplicate alerts.
    pub(crate) fn check_reminder_alerts(&mut self) -> Command<Message> {
        if self.is_standalone || self.is_notif_surface {
            return Command::none();
        }
        let now = chrono::Local::now().timestamp();
        let mut entries = RemindersOverlay::load();
        let mut changed = false;
        for e in entries.iter_mut() {
            if e.target_epoch <= now && !e.alerted {
                e.alerted = true;
                changed = true;
                let text = if e.text.is_empty() {
                    "Timer Finished!".into()
                } else {
                    e.text.clone()
                };
                let _ = moonbeauty::libraries::process::spawn_and_reap(
                    std::process::Command::new("notify-send").args([
                        "-a",
                        "Alarm",
                        "-u",
                        "critical",
                        "--",
                        "⏰ Timer Complete!",
                        &text,
                    ]),
                );
                if e.play_sound {
                    if let Some(sound) = &self.settings.alarm_sound {
                        moonbeauty::libraries::backend::play_alarm_sound(
                            sound,
                            self.settings.alarm_sound_volume,
                            self.settings.alarm_sound_duration_sec,
                        );
                    }
                }
            }
        }
        if changed {
            moonbeauty::overlays::reminders::save_reminders(&entries);
        }
        Command::none()
    }
}
