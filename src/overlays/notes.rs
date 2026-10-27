use chrono::Local;
use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Element, Length, Padding};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteItem {
    pub id: u64,
    pub content: String,
    pub ts: String,
}

pub const NOTES_INPUT_ID: &str = "notes_input";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Alphabetical,
    Newest,
    Oldest,
}

impl SortMode {
    fn next(self) -> Self {
        match self {
            SortMode::Alphabetical => SortMode::Newest,
            SortMode::Newest => SortMode::Oldest,
            SortMode::Oldest => SortMode::Alphabetical,
        }
    }
    fn label(self) -> &'static str {
        match self {
            SortMode::Alphabetical => "A-Z",
            SortMode::Newest => "Newest",
            SortMode::Oldest => "Oldest",
        }
    }
}

#[derive(Debug, Clone)]
pub enum NotesMessage {
    SearchOrNewChanged(String),
    AddNote,
    DeleteNote(u64),
    CopyNote(String),
    SelectNext,
    SelectPrev,
    DeleteSelected,
    CycleSortMode,
    Refocus,
    Dismiss,
}

pub struct NotesOverlay {
    pub input_text: String,
    pub notes: Vec<NoteItem>,
    pub selected_id: Option<u64>,
    pub sort_mode: SortMode,
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
            selected_id: None,
            sort_mode: SortMode::Newest,
        }
    }

    pub fn empty() -> Self {
        Self { input_text: String::new(), notes: Vec::new(), selected_id: None, sort_mode: SortMode::Newest }
    }

    fn visible_notes(&self) -> Vec<u64> {
        let query = self.input_text.trim().to_lowercase();
        let mut notes: Vec<&NoteItem> = self
            .notes
            .iter()
            .filter(|n| {
                query.is_empty()
                    || n.content.to_lowercase().contains(&query)
                    || n.ts.contains(&query)
            })
            .collect();
        match self.sort_mode {
            SortMode::Alphabetical => notes.sort_by_key(|a| a.content.to_lowercase()),
            SortMode::Newest => notes.sort_by(|a, b| b.id.cmp(&a.id)),
            SortMode::Oldest => notes.sort_by_key(|a| a.id),
        }
        notes.into_iter().map(|n| n.id).collect()
    }

    fn delete_note_advance(&mut self, id: u64) {
        let was_selected = self.selected_id == Some(id);
        let visible_before = self.visible_notes();
        let pos = visible_before.iter().position(|v| *v == id);

        self.notes.retain(|n| n.id != id);
        self.save();

        if !was_selected {
            return;
        }

        let visible_after = self.visible_notes();
        if visible_after.is_empty() {
            self.selected_id = None;
        } else {
            let new_pos = pos.map(|p| p.min(visible_after.len() - 1)).unwrap_or(0);
            self.selected_id = Some(visible_after[new_pos]);
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
                self.delete_note_advance(id);
            }
            NotesMessage::CopyNote(content) => {
                tokio::spawn(async move {
                    let _ = Command::new("wl-copy").arg(&content).status().await;
                    let _ = Command::new("notify-send")
                        .args(["-a", "Notes", "Note Copied", &content])
                        .spawn();
                });
            }
            NotesMessage::SelectNext => {
                let visible = self.visible_notes();
                if visible.is_empty() {
                    return;
                }
                let cur = self
                    .selected_id
                    .and_then(|id| visible.iter().position(|v| *v == id));
                self.selected_id = Some(match cur {
                    None => visible[0],
                    Some(i) => visible[(i + 1) % visible.len()],
                });
            }
            NotesMessage::SelectPrev => {
                let visible = self.visible_notes();
                if visible.is_empty() {
                    return;
                }
                let cur = self
                    .selected_id
                    .and_then(|id| visible.iter().position(|v| *v == id));
                self.selected_id = Some(match cur {
                    Some(0) | None => visible[visible.len() - 1],
                    Some(i) => visible[i - 1],
                });
            }
            NotesMessage::DeleteSelected => {
                if !self.input_text.is_empty() {
                    return;
                }
                if let Some(id) = self.selected_id {
                    self.delete_note_advance(id);
                }
            }
            NotesMessage::CycleSortMode => {
                self.sort_mode = self.sort_mode.next();
            }
            NotesMessage::Refocus => {}
            NotesMessage::Dismiss => {}
        }
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, NotesMessage> {
        let (win_w, win_h) = settings.get_window_size("notes", 840, 650);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
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

        let input = text_input("Type note to save [Enter to add]...", &self.input_text)
            .id(NOTES_INPUT_ID)
            .on_input(NotesMessage::SearchOrNewChanged)
            .on_submit(NotesMessage::AddNote)
            .padding(Padding {
                top: 18.0,
                bottom: 18.0,
                left: 20.0,
                right: 20.0,
            })
            .size(theme.font_size_overlay as f32);

        let visible_ids = self.visible_notes();
        let mut list_col = column![].spacing(8);

        for id in &visible_ids {
            let Some(note) = self.notes.iter().find(|n| n.id == *id) else {
                continue;
            };
            let id = note.id;
            let content_clone = note.content.clone();
            let is_sel = self.selected_id == Some(id);

            let row_item = row![
                text("📌").size(theme.font_size_overlay as f32),
                column![
                    text(&note.ts)
                        .size((theme.font_size_overlay as f32) * 0.85)
                        .color(theme.warning),
                    text(&note.content)
                        .size(theme.font_size_overlay as f32)
                        .color(theme.text),
                ]
                .width(Length::Fill)
                .spacing(4),
                button(
                    text("✕")
                        .size(theme.font_size_overlay as f32)
                        .color(theme.danger)
                )
                .on_press(NotesMessage::DeleteNote(id))
                .padding(6)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
            ]
            .spacing(16)
            .align_y(Alignment::Center);

            let bg = if is_sel {
                Some(iced::Background::Color(theme.surface_alt))
            } else {
                None
            };
            let border_col = if is_sel { theme.text } else { theme.border };
            let border_w = if is_sel {
                theme.control_border_width
            } else {
                0.0
            };

            let note_card = button(
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
            .on_press(NotesMessage::CopyNote(content_clone))
            .padding(0)
            .width(Length::Fill)
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

            // Inset the row slightly so highlight doesn't touch corners
            list_col = list_col.push(container(note_card).padding(Padding {
                top: 0.0,
                bottom: 0.0,
                left: 8.0,
                right: 8.0,
            }));
        }

        let sort_btn = button(
            text(format!("Sort: {}", self.sort_mode.label()))
                .size((theme.font_size_overlay as f32) * 0.85)
                .color(theme.text),
        )
        .on_press(NotesMessage::CycleSortMode)
        .padding([6, 12])
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(theme.surface_alt)),
            border: iced::Border {
                color: theme.border,
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: theme.text,
            ..Default::default()
        });

        let header_row = container(
            row![
                text("📝 QUICK NOTES")
                    .size(theme.font_size_overlay as f32)
                    .color(theme.text),
                iced::widget::space::horizontal(),
                sort_btn,
            ]
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

        let list_container = container(scrollable(list_col).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: body_inset,
                bottom: 0.0,
                left: body_inset,
            });

        let footer_row = container(
            text("Click note to copy  •  ✕ to delete  •  ESC to dismiss")
                .size((theme.font_size_overlay as f32) * 0.85)
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: footer_inset,
            bottom: 0.0,
            left: footer_inset,
        });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![header_row, input_row, list_container, footer_row]
                .spacing(14)
                .padding(Padding {
                    top: pad_top,
                    right: 0.0,
                    bottom: pad_bottom,
                    left: 0.0,
                }),
        )
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h))
        .style(|_t| iced::widget::container::Style::default());

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_t| iced::widget::container::Style::default())
            .into()
    }
}
