use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Element, Length, Padding};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: u64,
    pub task: String,
    pub completed: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodoDb {
    pub categories: Vec<String>,
    pub todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoFilter {
    All,
    Active,
    Completed,
}

#[derive(Debug, Clone)]
pub enum TodoMessage {
    SelectCategory(String),
    AddCategory(String),
    DeleteCategory(String),
    FilterChanged(TodoFilter),
    NewTaskInputChanged(String),
    AddTask,
    ToggleTask(u64),
    DeleteTask(u64),
    CategoryPrev,
    CategoryNext,
    FilterPrev,
    FilterNext,
    TaskSelectPrev,
    TaskSelectNext,
    ToggleSelected,
    DeleteSelected,
    EditSelected,
    EditTask(u64),
    CancelEdit,
    Refocus,
    Dismiss,
}

pub const TODO_INPUT_ID: &str = "todo_input";

pub struct TodoOverlay {
    pub db: TodoDb,
    pub active_category: String,
    pub filter: TodoFilter,
    pub new_task_text: String,
    /// Which task (by id) is keyboard-highlighted. Cleared when the task
    /// disappears (deleted, toggled off a filter, category switch).
    pub selected_task_id: Option<u64>,
    /// Which task (by id) is currently being edited. When Some, the input
    /// field contains that task's text and Enter saves back to it (instead
    /// of adding a new task). Cleared on Enter-save, Escape-cancel, or when
    /// the underlying task is deleted.
    pub editing_task_id: Option<u64>,
}

impl Default for TodoOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl TodoOverlay {
    pub fn new() -> Self {
        let mut db = Self::load();
        if db.categories.is_empty() {
            db.categories.push("Inbox".into());
        }
        let active_category = db
            .categories
            .first()
            .cloned()
            .unwrap_or_else(|| "Inbox".into());

        Self {
            db,
            active_category,
            filter: TodoFilter::All,
            new_task_text: String::new(),
            selected_task_id: None,
            editing_task_id: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            db: TodoDb { categories: vec!["Inbox".into()], todos: Vec::new() },
            active_category: "Inbox".into(),
            filter: TodoFilter::All,
            new_task_text: String::new(),
            selected_task_id: None,
            editing_task_id: None,
        }
    }

    fn visible_task_ids(&self) -> Vec<u64> {
        self.db
            .todos
            .iter()
            .filter(|t| t.category == self.active_category)
            .filter(|t| match self.filter {
                TodoFilter::All => true,
                TodoFilter::Active => !t.completed,
                TodoFilter::Completed => t.completed,
            })
            .map(|t| t.id)
            .collect()
    }

    fn storage_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let mbs = base.join("moon-beauty-shell").join("todos.json");
        let qs = base.join("quickshell").join("todos.json");
        // One-time migration: user's pre-migration todos lived at
        // ~/.local/share/quickshell/todos.json (Quickshell convention).
        // Copy them to the moon-beauty path on first launch so existing
        // tasks aren't invisible.
        if !mbs.exists() && qs.exists() {
            if let Some(parent) = mbs.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match fs::copy(&qs, &mbs) {
                Ok(n) => eprintln!("[todo] migrated {} bytes from quickshell → moon-beauty", n),
                Err(e) => eprintln!("[todo] quickshell migration failed: {e}"),
            }
        }
        mbs
    }

    pub fn load() -> TodoDb {
        // Finding 47: 8 MB cap.
        crate::libraries::process::read_json_capped::<TodoDb>(
            &Self::storage_path(),
            8 * 1024 * 1024,
        )
        .unwrap_or_else(|| TodoDb {
            categories: vec!["Inbox".into()],
            todos: Vec::new(),
        })
    }

    pub fn save(&self) {
        let path = Self::storage_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(&self.db) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, path);
            }
        }
    }

    pub fn update(&mut self, message: TodoMessage) {
        match message {
            TodoMessage::SelectCategory(cat) => {
                self.active_category = cat;
                self.selected_task_id = None;
                if self.editing_task_id.is_some() {
                    self.editing_task_id = None;
                    self.new_task_text.clear();
                }
            }
            TodoMessage::AddCategory(name) => {
                let clean = name.trim();
                if !clean.is_empty()
                    && !self
                        .db
                        .categories
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(clean))
                {
                    self.db.categories.push(clean.to_string());
                    self.active_category = clean.to_string();
                    self.save();
                }
            }
            TodoMessage::DeleteCategory(name) => {
                if name != "Inbox" {
                    self.db.categories.retain(|c| c != &name);
                    self.db.todos.retain(|t| t.category != name);
                    self.active_category = "Inbox".into();
                    self.save();
                }
            }
            TodoMessage::FilterChanged(f) => {
                self.filter = f;
                self.selected_task_id = None;
                if self.editing_task_id.is_some() {
                    self.editing_task_id = None;
                    self.new_task_text.clear();
                }
            }
            TodoMessage::NewTaskInputChanged(txt) => {
                // 'e' / 'E' as an edit shortcut. text_input captures every
                // character key so the subscription never sees this — the
                // only hook is here, on the resulting text change. Gate on:
                //   (a) input was empty before this keystroke
                //   (b) new value is exactly "e" or "E"
                //   (c) a task is selected
                //   (d) not already editing
                // Trade-off: cannot start a new task whose first letter is
                // 'e' while a task is selected. Deselect first (Escape or
                // click another category) to type 'e' as normal.
                let first_char_e = self.new_task_text.is_empty()
                    && txt.chars().count() == 1
                    && txt
                        .chars()
                        .next()
                        .map(|c| c.eq_ignore_ascii_case(&'e'))
                        .unwrap_or(false);
                if first_char_e && self.selected_task_id.is_some() && self.editing_task_id.is_none()
                {
                    if let Some(id) = self.selected_task_id {
                        if let Some(t) = self.db.todos.iter().find(|t| t.id == id) {
                            self.editing_task_id = Some(id);
                            self.new_task_text = t.task.clone();
                            eprintln!("[todo] 'e' intercepted → edit mode (id={})", id);
                            return;
                        }
                    }
                }
                self.new_task_text = txt;
            }
            TodoMessage::AddTask => {
                // 3-way dispatch on Enter (fires via text_input::on_submit):
                //   non-empty input + editing -> save edit
                //   non-empty input + not editing -> add new task
                //   empty input + selection -> toggle complete
                let clean = self.new_task_text.trim().to_string();
                if !clean.is_empty() {
                    if let Some(edit_id) = self.editing_task_id {
                        if let Some(t) = self.db.todos.iter_mut().find(|t| t.id == edit_id) {
                            t.task = clean;
                            self.save();
                        }
                        self.editing_task_id = None;
                        self.new_task_text.clear();
                        self.selected_task_id = Some(edit_id);
                    } else {
                        let max_id = self.db.todos.iter().map(|t| t.id).max().unwrap_or(0);
                        let new_id = max_id + 1;
                        self.db.todos.push(TodoItem {
                            id: new_id,
                            task: clean,
                            completed: false,
                            category: self.active_category.clone(),
                        });
                        self.new_task_text.clear();
                        self.save();
                        self.selected_task_id = Some(new_id);
                    }
                } else if let Some(id) = self.selected_task_id {
                    // Empty input: treat Enter as "toggle selected"
                    if let Some(t) = self.db.todos.iter_mut().find(|t| t.id == id) {
                        t.completed = !t.completed;
                        self.save();
                        if !self.visible_task_ids().contains(&id) {
                            self.selected_task_id = None;
                        }
                    }
                }
            }
            TodoMessage::ToggleTask(id) => {
                if let Some(t) = self.db.todos.iter_mut().find(|t| t.id == id) {
                    t.completed = !t.completed;
                    self.save();
                    if !self.visible_task_ids().contains(&id) {
                        self.selected_task_id = None;
                    }
                }
            }
            TodoMessage::DeleteTask(id) => {
                self.db.todos.retain(|t| t.id != id);
                if self.selected_task_id == Some(id) {
                    self.selected_task_id = None;
                }
                if self.editing_task_id == Some(id) {
                    self.editing_task_id = None;
                    self.new_task_text.clear();
                }
                self.save();
            }
            TodoMessage::CategoryPrev => {
                if !self.new_task_text.is_empty() {
                    return;
                }
                let n = self.db.categories.len();
                if n > 0 {
                    if let Some(idx) = self
                        .db
                        .categories
                        .iter()
                        .position(|c| c == &self.active_category)
                    {
                        self.active_category = self.db.categories[(idx + n - 1) % n].clone();
                    }
                }
            }
            TodoMessage::CategoryNext => {
                if !self.new_task_text.is_empty() {
                    return;
                }
                let n = self.db.categories.len();
                if n > 0 {
                    if let Some(idx) = self
                        .db
                        .categories
                        .iter()
                        .position(|c| c == &self.active_category)
                    {
                        self.active_category = self.db.categories[(idx + 1) % n].clone();
                    }
                }
            }
            TodoMessage::FilterPrev => {
                self.filter = match self.filter {
                    TodoFilter::All => TodoFilter::Completed,
                    TodoFilter::Active => TodoFilter::All,
                    TodoFilter::Completed => TodoFilter::Active,
                };
            }
            TodoMessage::FilterNext => {
                self.filter = match self.filter {
                    TodoFilter::All => TodoFilter::Active,
                    TodoFilter::Active => TodoFilter::Completed,
                    TodoFilter::Completed => TodoFilter::All,
                };
            }
            TodoMessage::TaskSelectPrev => {
                let visible = self.visible_task_ids();
                if visible.is_empty() {
                    return;
                }
                let cur = self
                    .selected_task_id
                    .and_then(|id| visible.iter().position(|v| *v == id));
                self.selected_task_id = Some(match cur {
                    Some(0) | None => visible[visible.len() - 1],
                    Some(i) => visible[i - 1],
                });
            }
            TodoMessage::TaskSelectNext => {
                if !self.new_task_text.is_empty() {
                    return;
                }
                if self.editing_task_id.is_some() {
                    return;
                }
                let visible = self.visible_task_ids();
                if visible.is_empty() {
                    return;
                }
                let cur = self
                    .selected_task_id
                    .and_then(|id| visible.iter().position(|v| *v == id));
                self.selected_task_id = Some(match cur {
                    None => visible[0],
                    Some(i) => visible[(i + 1) % visible.len()],
                });
            }
            TodoMessage::ToggleSelected => {
                if !self.new_task_text.is_empty() {
                    return;
                }
                if let Some(id) = self.selected_task_id {
                    if let Some(t) = self.db.todos.iter_mut().find(|t| t.id == id) {
                        t.completed = !t.completed;
                        self.save();
                        if !self.visible_task_ids().contains(&id) {
                            self.selected_task_id = None;
                        }
                    }
                }
            }
            TodoMessage::DeleteSelected => {
                // Guard: if input has text, Delete means "delete char
                // before/after cursor", NOT "delete selected task".
                // text_input handles the char; we no-op.
                if !self.new_task_text.is_empty() {
                    return;
                }
                eprintln!(
                    "[todo] DeleteSelected fired (sel={:?})",
                    self.selected_task_id
                );
                if let Some(id) = self.selected_task_id {
                    self.db.todos.retain(|t| t.id != id);
                    if self.editing_task_id == Some(id) {
                        self.editing_task_id = None;
                        self.new_task_text.clear();
                    }
                    self.selected_task_id = None;
                    self.save();
                }
            }
            TodoMessage::EditSelected => {
                if let Some(id) = self.selected_task_id {
                    if let Some(t) = self.db.todos.iter().find(|t| t.id == id) {
                        self.editing_task_id = Some(id);
                        self.new_task_text = t.task.clone();
                    }
                }
            }
            TodoMessage::EditTask(id) => {
                if let Some(t) = self.db.todos.iter().find(|t| t.id == id) {
                    self.selected_task_id = Some(id);
                    self.editing_task_id = Some(id);
                    self.new_task_text = t.task.clone();
                }
            }
            TodoMessage::CancelEdit => {
                self.editing_task_id = None;
                self.new_task_text.clear();
            }
            TodoMessage::Refocus => {}
            TodoMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &crate::libraries::config::settings::ShellSettings,
    ) -> Element<'a, TodoMessage> {
        {
            use std::sync::OnceLock;
            static ONCE: OnceLock<()> = OnceLock::new();
            ONCE.get_or_init(|| {
                eprintln!(
                    "[todo-view] overlay_font_size={} normal_font_size={}",
                    theme.font_size_overlay, theme.font_size_normal
                );
            });
        }

        let (win_w, win_h) = settings.get_window_size("todo", 840, 650);
        let mut cat_row = row![].spacing(8).align_y(Alignment::Center);
        for cat in &self.db.categories {
            let is_sel = cat == &self.active_category;
            let col = if is_sel { theme.success } else { theme.text };
            let cat_clone = cat.clone();

            let cat_btn = button(
                text(cat)
                    .size((theme.font_size_normal) as f32)
                    .color(col)
                    .bold(),
            )
            .on_press(TodoMessage::SelectCategory(cat_clone))
            .padding([6, 12])
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

            cat_row = cat_row.push(cat_btn);
        }

        let input = text_input("Add a task and press [Enter]...", &self.new_task_text)
            .id(TODO_INPUT_ID)
            .on_input(TodoMessage::NewTaskInputChanged)
            .on_submit(TodoMessage::AddTask)
            .padding(14)
            .size(theme.font_size_overlay as f32);

        let mut list_col = column![].spacing(6);
        for item in self.db.todos.iter().filter(|t| {
            t.category == self.active_category
                && match self.filter {
                    TodoFilter::All => true,
                    TodoFilter::Active => !t.completed,
                    TodoFilter::Completed => t.completed,
                }
        }) {
            let id = item.id;
            let is_done = item.completed;

            let is_editing = self.editing_task_id == Some(id);
            let row_item = row![
                checkbox(is_done).on_toggle(move |_| TodoMessage::ToggleTask(id)),
                text(&item.task)
                    .size(theme.font_size_overlay as f32)
                    .color(if is_done { theme.muted } else { theme.text })
                    .width(Length::Fill),
                button(
                    text("✎")
                        .size(theme.font_size_overlay as f32)
                        .color(if is_editing {
                            theme.success
                        } else {
                            theme.text
                        })
                )
                .on_press(TodoMessage::EditTask(id))
                .padding(4)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
                button(
                    text("✕")
                        .size(theme.font_size_overlay as f32)
                        .color(theme.danger)
                )
                .on_press(TodoMessage::DeleteTask(id))
                .padding(4)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
            ]
            .spacing(16)
            .align_y(Alignment::Center);

            let is_sel = self.selected_task_id == Some(id);
            // Row bg is applied to a narrower wrapper than the row
            // content itself (below), so at the top of the list the
            // highlight can't touch the octagon corners.
            let bg: Option<iced::Background> = if is_editing {
                Some(iced::Background::Color(theme.background))
            } else if is_sel {
                Some(iced::Background::Color(theme.surface_alt))
            } else {
                None
            };
            let border_col = if is_editing {
                theme.success
            } else if is_sel {
                theme.text
            } else {
                theme.border
            };
            let border_w = if is_editing {
                theme.control_border_width * 1.5
            } else if is_sel {
                theme.control_border_width
            } else {
                0.0
            };
            // Wrap the styled row in an outer container with horizontal
            // margin, so the highlight never touches the octagon edges.
            list_col = list_col.push(
                container(
                    container(row_item)
                        .padding([10, 14])
                        .width(Length::Fill)
                        .style(move |_t| container::Style {
                            background: bg,
                            border: iced::Border {
                                color: border_col,
                                width: border_w,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                )
                .width(Length::Fill)
                .padding(Padding {
                    top: 0.0,
                    bottom: 0.0,
                    left: 8.0,
                    right: 8.0,
                }),
            );
        }

        let filter_row = row![
            text("Filter:")
                .size((theme.font_size_normal) as f32)
                .bold()
                .color(theme.text),
            button(
                text("All")
                    .size((theme.font_size_overlay as f32) * 0.85)
                    .color(if self.filter == TodoFilter::All {
                        theme.success
                    } else {
                        theme.text
                    })
            )
            .on_press(TodoMessage::FilterChanged(TodoFilter::All))
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            }),
            button(
                text("Active")
                    .size((theme.font_size_overlay as f32) * 0.85)
                    .color(if self.filter == TodoFilter::Active {
                        theme.success
                    } else {
                        theme.text
                    })
            )
            .on_press(TodoMessage::FilterChanged(TodoFilter::Active))
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            }),
            button(
                text("Completed")
                    .size((theme.font_size_overlay as f32) * 0.85)
                    .color(if self.filter == TodoFilter::Completed {
                        theme.success
                    } else {
                        theme.text
                    })
            )
            .on_press(TodoMessage::FilterChanged(TodoFilter::Completed))
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            }),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        // Angled hexagon matches the rest of the shell (launcher, calc,
        // lockscreen, dictionary). The shape needs an explicit size —
        // without it, ShapeBox fills the parent and gets squeezed when
        // rendered inside the bar's 42px-tall window.
        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        // Per-row horizontal insets so content clears the angled-hexagon
        // corner cuts. Matches the pattern used by launcher, clipboard,
        // dictionary, settings. Without this, content clips at diagonals.
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let footer_inset = if dy > pad_bottom {
            (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let body_inset = header_inset.min(footer_inset) + 4.0;
        eprintln!(
            "[todo-size] win={}x{} cut={} angle={} header_inset={:.1} body_inset={:.1} footer_inset={:.1}",
            win_w, win_h, cut_f, angle_deg, header_inset, body_inset, footer_inset
        );
        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        // Per-row containers get independent horizontal insets so content
        // clears the octagon diagonals at top and bottom.
        let header_row = container(
            row![
                text("✅ TASK BOARD")
                    .size(theme.font_size_overlay as f32)
                    .color(theme.text),
                scrollable(cat_row).width(Length::Fill),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let input_row = container(input).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: body_inset,
            bottom: 0.0,
            left: body_inset,
        });

        let list_container = container(
            scrollable(list_col)
                .id(iced::widget::Id::new("todo_list"))
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: body_inset,
            bottom: 0.0,
            left: body_inset,
        });

        let filter_container = container(filter_row).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: footer_inset,
            bottom: 0.0,
            left: footer_inset,
        });

        let card_content = container(
            column![header_row, input_row, list_container, filter_container]
                .spacing(14)
                .padding(Padding {
                    top: pad_top,
                    right: 0.0,
                    bottom: pad_bottom,
                    left: 0.0,
                }),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
