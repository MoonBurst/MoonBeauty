use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

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
    Dismiss,
}

pub struct TodoOverlay {
    pub db: TodoDb,
    pub active_category: String,
    pub filter: TodoFilter,
    pub new_task_text: String,
}

impl TodoOverlay {
    pub fn new() -> Self {
        let mut db = Self::load();
        if db.categories.is_empty() {
            db.categories.push("Inbox".into());
        }
        let active_category = db.categories.first().cloned().unwrap_or_else(|| "Inbox".into());

        Self {
            db,
            active_category,
            filter: TodoFilter::All,
            new_task_text: String::new(),
        }
    }

    fn storage_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("moon-beauty-shell").join("todos.json")
    }

    pub fn load() -> TodoDb {
        let path = Self::storage_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(db) = serde_json::from_str::<TodoDb>(&raw) {
                    return db;
                }
            }
        }
        TodoDb {
            categories: vec!["Inbox".into()],
            todos: Vec::new(),
        }
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
            }
            TodoMessage::AddCategory(name) => {
                let clean = name.trim();
                if !clean.is_empty() && !self.db.categories.iter().any(|c| c.eq_ignore_ascii_case(clean)) {
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
            }
            TodoMessage::NewTaskInputChanged(txt) => {
                self.new_task_text = txt;
            }
            TodoMessage::AddTask => {
                let clean = self.new_task_text.trim();
                if !clean.is_empty() {
                    let max_id = self.db.todos.iter().map(|t| t.id).max().unwrap_or(0);
                    self.db.todos.push(TodoItem {
                        id: max_id + 1,
                        task: clean.to_string(),
                        completed: false,
                        category: self.active_category.clone(),
                    });
                    self.new_task_text.clear();
                    self.save();
                }
            }
            TodoMessage::ToggleTask(id) => {
                if let Some(t) = self.db.todos.iter_mut().find(|t| t.id == id) {
                    t.completed = !t.completed;
                    self.save();
                }
            }
            TodoMessage::DeleteTask(id) => {
                self.db.todos.retain(|t| t.id != id);
                self.save();
            }
            TodoMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, TodoMessage> {
        let mut cat_row = row![].spacing(8).align_y(Alignment::Center);
        for cat in &self.db.categories {
            let is_sel = cat == &self.active_category;
            let col = if is_sel { theme.success } else { theme.text };
            let cat_clone = cat.clone();

            let cat_btn = button(
                text(cat)
                    .size(theme.font_size_normal)
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
            .on_input(TodoMessage::NewTaskInputChanged)
            .on_submit(TodoMessage::AddTask)
            .padding(14)
            .size(theme.font_size_overlay);

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

            let row_item = row![
                checkbox("", is_done).on_toggle(move |_| TodoMessage::ToggleTask(id)),
                text(&item.task)
                    .size(theme.font_size_normal)
                    .color(if is_done { theme.muted } else { theme.text })
                    .width(Length::Fill),
                button(text("✕").size(12).color(theme.danger))
                    .on_press(TodoMessage::DeleteTask(id))
                    .padding(4)
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    }),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            list_col = list_col.push(container(row_item).padding([6, 10]).width(Length::Fill));
        }

        let filter_row = row![
            text("Filter:").size(theme.font_size_normal).bold().color(theme.text),
            button(text("All").size(12).color(if self.filter == TodoFilter::All { theme.success } else { theme.text }))
                .on_press(TodoMessage::FilterChanged(TodoFilter::All))
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("Active").size(12).color(if self.filter == TodoFilter::Active { theme.success } else { theme.text }))
                .on_press(TodoMessage::FilterChanged(TodoFilter::Active))
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            button(text("Completed").size(12).color(if self.filter == TodoFilter::Completed { theme.success } else { theme.text }))
                .on_press(TodoMessage::FilterChanged(TodoFilter::Completed))
                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                row![
                    text("✅ TASK BOARD")
                        .size(theme.font_size_overlay)
                        .color(theme.text),
                    scrollable(cat_row).width(Length::Fill),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
                input,
                scrollable(list_col).height(Length::Fill),
                filter_row,
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(860.0))
        .height(Length::Fixed(720.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(TodoMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.6))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
