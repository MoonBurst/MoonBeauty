//! Small overlay handlers, extracted from `app/update.rs`.
//!
//! These overlays are individually small (~10-40 lines each); grouping them
//! in one file keeps `app/update/` from becoming 10 nearly-empty files.
//! Larger overlays (music, notifications, reminders, email, lockscreen,
//! clipboard, settings) have their own files.

#![allow(clippy::collapsible_match)]

use super::*;

async fn copy_pass_key(key: String) {
    let clean = key.trim();
    if clean.is_empty() {
        return;
    }
    let _ = tokio::process::Command::new("pass")
        .args(["-c", clean])
        .status()
        .await;
    let _ = tokio::process::Command::new("notify-send")
        .args(["-a", "Password Store", "Credential Copied", clean])
        .spawn();
}

async fn execute_power_action(action: PowerAction) {
    use tokio::process::Command as TokioCommand;
    match action {
        PowerAction::Lock => {
            // Triggers session lockscreen
        }
        PowerAction::Sleep => {
            let _ = TokioCommand::new("systemctl").arg("suspend").status().await;
        }
        PowerAction::Reboot => {
            let _ = TokioCommand::new("systemctl").arg("reboot").status().await;
        }
        PowerAction::Shutdown => {
            let _ = TokioCommand::new("systemctl").arg("poweroff").status().await;
        }
        PowerAction::Logout => {
            if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
                let _ = TokioCommand::new("hyprctl")
                    .args(["dispatch", "exit"])
                    .status()
                    .await;
            } else if std::env::var("SWAYSOCK").is_ok() {
                let _ = TokioCommand::new("swaymsg").arg("exit").status().await;
            } else {
                let _ = TokioCommand::new("loginctl")
                    .args(["terminate-session", "self"])
                    .status()
                    .await;
            }
        }
    }
}

impl MoonBeautyShellApp {
    pub(crate) fn handle_calc(&mut self, msg: CalcMessage) -> Command<Message> {
        match msg {
            CalcMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            CalcMessage::Refocus => {
                return iced::widget::operation::focus(moonbeauty::overlays::calc::CALC_INPUT_ID)
                    .map(Message::Calc);
            }
            CalcMessage::CopyResult(val) => {
                if self.is_standalone {
                    // Run copy to completion, then exit. On runtime
                    // failure, log + exit anyway — copy is best-effort.
                    let v = val.clone();
                    crate::app::helpers::run_then_exit(async move {
                        CalculatorOverlay::copy_to_clipboard(&v).await;
                    });
                }
                self.active_overlay = None;
                return Command::perform(
                    async move {
                        CalculatorOverlay::copy_to_clipboard(&val).await;
                    },
                    |_| Message::DismissActiveOverlay,
                );
            }
            _ => self.calc.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_notes(&mut self, msg: NotesMessage) -> Command<Message> {
        match msg {
            NotesMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            NotesMessage::Refocus => {
                return iced::widget::operation::focus(moonbeauty::overlays::notes::NOTES_INPUT_ID)
                    .map(Message::Notes);
            }
            NotesMessage::CopyNote(content) => {
                return Command::perform(
                    async move {
                        let _ = tokio::process::Command::new("wl-copy")
                            .arg(&content)
                            .status()
                            .await;
                        let _ = tokio::process::Command::new("notify-send")
                            .args(["-a", "Notes", "Note Copied", &content])
                            .spawn();
                    },
                    |_| Message::Noop,
                );
            }
            _ => self.notes.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_todo(&mut self, msg: TodoMessage) -> Command<Message> {
        match msg {
            TodoMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            TodoMessage::Refocus => {
                return iced::widget::operation::focus(moonbeauty::overlays::todo::TODO_INPUT_ID)
                    .map(Message::Todo);
            }
            TodoMessage::EditSelected | TodoMessage::EditTask(_) => {
                self.todo.update(msg);
                // Refocus the input so the user can type immediately
                // after clicking ✎ or pressing F2.
                return iced::widget::operation::focus(moonbeauty::overlays::todo::TODO_INPUT_ID)
                    .map(Message::Todo);
            }
            TodoMessage::TaskSelectPrev | TodoMessage::TaskSelectNext => {
                self.todo.update(msg);
                if let Some(sel_id) = self.todo.selected_task_id {
                    // Scroll to selected task using row height of ~40px.
                    let visible = self.todo.visible_task_ids();
                    let idx = visible.iter().position(|v| *v == sel_id).unwrap_or(0);
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("todo_list"),
                        iced::widget::operation::AbsoluteOffset {
                            x: 0.0,
                            y: (idx as f32 * 40.0 - 100.0).max(0.0),
                        },
                    );
                }
            }
            _ => self.todo.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_dict(&mut self, msg: DictMessage) -> Command<Message> {
        match msg {
            DictMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            DictMessage::Refocus => {
                return iced::widget::operation::focus(
                    moonbeauty::overlays::dictionary::DICT_SEARCH_ID,
                )
                .map(Message::Dict);
            }
            DictMessage::SearchWord => {
                let w = self.dict.word_query.clone();
                // Check cache first
                if let Some(cached) = self.dict.cache.get(&w.to_lowercase()).cloned() {
                    self.dict.update(DictMessage::DefinitionsLoaded(w, cached));
                    return Command::none();
                }
                return Command::perform(
                    async move { (w.clone(), DictionaryOverlay::lookup_word(&w).await) },
                    |(w, defs)| Message::Dict(DictMessage::DefinitionsLoaded(w, defs)),
                );
            }
            DictMessage::CopyDefinition(def) => {
                if self.is_standalone {
                    let def_clone = def.clone();
                    crate::app::helpers::run_then_exit(async move {
                        let _ = tokio::process::Command::new("wl-copy")
                            .arg(&def_clone)
                            .status()
                            .await;
                    });
                }
                return Command::perform(
                    async move {
                        let _ = tokio::process::Command::new("wl-copy")
                            .arg(&def)
                            .status()
                            .await;
                    },
                    |_| Message::DismissActiveOverlay,
                );
            }
            _ => self.dict.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_unicode(&mut self, msg: UnicodeMessage) -> Command<Message> {
        match msg {
            UnicodeMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            UnicodeMessage::Refocus => {
                return iced::widget::operation::focus(
                    moonbeauty::overlays::unicode::UNICODE_INPUT_ID,
                )
                .map(Message::Unicode);
            }
            UnicodeMessage::CopySymbol(sym) => {
                if self.is_standalone {
                    crate::app::helpers::run_then_exit(UnicodeOverlay::copy(sym));
                }
                self.active_overlay = None;
                return Command::perform(
                    async move {
                        UnicodeOverlay::copy(sym).await;
                    },
                    |_| Message::DismissActiveOverlay,
                );
            }
            UnicodeMessage::SelectNext
            | UnicodeMessage::SelectPrev
            | UnicodeMessage::SelectNextRow
            | UnicodeMessage::SelectPrevRow => {
                self.unicode.update(msg);
                // Auto-scroll to selected item. Grid is 5 columns,
                // item height = 66 + 8 gap = 74. Scroll so selected
                // is roughly centered vertically.
                if let Some(idx) = self.unicode.selected_idx {
                    let row = idx / 5;
                    let y = (row as f32) * 74.0 - 200.0;
                    return iced::widget::operation::scroll_to(
                        iced::widget::Id::new("unicode_grid"),
                        iced::widget::operation::AbsoluteOffset {
                            x: 0.0,
                            y: y.max(0.0),
                        },
                    );
                }
            }
            UnicodeMessage::CopySelected => {
                if let Some(sym) = self.unicode.selected_symbol() {
                    return self.handle_unicode(UnicodeMessage::CopySymbol(sym));
                }
            }
            _ => self.unicode.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_web(&mut self, msg: WebSearchMessage) -> Command<Message> {
        match msg {
            WebSearchMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            WebSearchMessage::ExecuteSearch => {
                if let Some(url) = WebSearchOverlay::build_url(&self.web.query) {
                    if self.is_standalone {
                        // Fork synchronously so the child process exists
                        // before exit(0), preventing tokio::spawn races.
                        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(
                        async move {
                            WebSearchOverlay::launch_browser(&url).await;
                        },
                        |_| Message::DismissActiveOverlay,
                    );
                }
            }
            _ => self.web.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_power(&mut self, msg: PowerMessage) -> Command<Message> {
        match msg {
            PowerMessage::Dismiss => {
                let _ = self.power.update(PowerMessage::Dismiss);
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            _ => {
                if let Some(action) = self.power.update(msg) {
                    if self.is_standalone {
                        crate::app::helpers::run_then_exit(execute_power_action(action));
                    }
                    self.active_overlay = None;
                    return Command::perform(execute_power_action(action), |_| {
                        Message::DismissActiveOverlay
                    });
                }
            }
        }
        Command::none()
    }

    pub(crate) fn handle_amogus(&mut self, msg: AmogusMessage) -> Command<Message> {
        match msg {
            AmogusMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            AmogusMessage::TriggerScan => {
                self.amogus.update(AmogusMessage::TriggerScan);
                return Command::perform(AmogusOverlay::run_scan(), |cols| {
                    Message::Amogus(AmogusMessage::ScanFinished(cols))
                });
            }
            _ => self.amogus.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_rng(&mut self, msg: RngMessage) -> Command<Message> {
        match msg {
            RngMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            _ => self.rng.update(msg),
        }
        Command::none()
    }

    pub(crate) fn handle_pass(&mut self, msg: PassMessage) -> Command<Message> {
        match msg {
            PassMessage::Dismiss => {
                if self.is_standalone {
                    std::process::exit(0);
                }
                self.active_overlay = None;
            }
            PassMessage::Refocus => {
                return iced::widget::operation::focus(moonbeauty::overlays::pass::PASS_SEARCH_ID)
                    .map(Message::Pass);
            }
            PassMessage::CopyKey(k) => {
                if self.is_standalone {
                    crate::app::helpers::run_then_exit(copy_pass_key(k));
                }
                self.active_overlay = None;
                return Command::perform(copy_pass_key(k), |_| Message::DismissActiveOverlay);
            }
            _ => self.pass.update(msg),
        }
        Command::none()
    }
}
