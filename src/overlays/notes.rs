use chrono::Local;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteItem {
    pub id: u64,
    pub content: String,
    pub ts: String,
}

#[derive(Debug, Clone)]
pub enum NotesMessage {
    SearchOrNewChanged(String),
    AddNote,
    DeleteNote(u64),
    CopyNote(String),
    Dismiss,
}

pub struct NotesOverlay {
    pub input_text: String,
    pub notes: Vec<NoteItem>,
}

impl Default for NotesOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl NotesOverlay {
    pub fn new() -> Self {
        Self {
            input_text: String::new(),
            notes: Self::load(),
        }
    }

    fn storage_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("moon-beauty-shell").join("notes.json")
    }

    pub fn load() -> Vec<NoteItem> {
        let path = Self::storage_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(items) = serde_json::from_str::<Vec<NoteItem>>(&raw) {
                    return items;
                }
            }
        }
        Vec::new()
    }

    pub fn save(&self) {
        let path = Self::storage_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(&self.notes) {
            let tmp = path.with_extension("tmp");
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(tmp, path);
            }
        }
    }

    pub fn update(&mut self, message: NotesMessage) {
        match message {
            NotesMessage::SearchOrNewChanged(val) => {
                self.input_text = val;
            }
            NotesMessage::AddNote => {
                let clean = self.input_text.trim();
                if !clean.is_empty() {
                    let max_id = self.notes.iter().map(|n| n.id).max().unwrap_or(0);
                    let ts = Local::now().format("%Y-%m-%d %H:%M").to_string();
                    self.notes.insert(
                        0,
                        NoteItem {
                            id: max_id + 1,
                            content: clean.to_string(),
                            ts,
                        },
                    );
                    self.input_text.clear();
                    self.save();
                }
            }
            NotesMessage::DeleteNote(id) => {
                self.notes.retain(|n| n.id != id);
                self.save();
            }
            NotesMessage::CopyNote(content) => {
                tokio::spawn(async move {
                    let _ = Command::new("wl-copy").arg(&content).status().await;
                    let _ = Command::new("notify-send")
                        .args(["-a", "Notes", "Note Copied", &content])
                        .spawn();
                });
            }
            NotesMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, NotesMessage> {
        let input = text_input("Type note to save [Enter to add]...", &self.input_text)
            .on_input(NotesMessage::SearchOrNewChanged)
            .on_submit(NotesMessage::AddNote)
            .padding(14)
            .size((theme.font_size_overlay) as f32);

        let query = self.input_text.trim().to_lowercase();
        let mut list_col = column![].spacing(8);

        for note in self.notes.iter().filter(|n| {
            query.is_empty()
                || n.content.to_lowercase().contains(&query)
                || n.ts.contains(&query)
        }) {
            let id = note.id;
            let content_clone = note.content.clone();

            let row_item = row![
                text("📌").size(14),
                column![
                    text(&note.ts)
                        .size((theme.font_size_normal - 4) as f32)
                        .color(theme.warning),
                    text(&note.content)
                        .size((theme.font_size_normal) as f32)
                        .color(theme.text),
                ]
                .width(Length::Fill)
                .spacing(2),
                button(text("✕").size(12).color(theme.danger))
                    .on_press(NotesMessage::DeleteNote(id))
                    .padding(6)
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    }),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            let note_card = button(container(row_item).padding(10))
                .on_press(NotesMessage::CopyNote(content_clone))
                .width(Length::Fill)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });

            list_col = list_col.push(note_card);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("📝 QUICK NOTES")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.text),
                input,
                scrollable(list_col).height(Length::Fill),
                text("Click note to copy  •  ✕ to delete  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(840.0))
        .height(Length::Fixed(650.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(NotesMessage::Dismiss)
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
