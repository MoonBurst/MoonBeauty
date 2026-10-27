use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Element, Length, Padding};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::libraries::style::TextExt;

pub const DICT_SEARCH_ID: &str = "dict_search";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeaningEntry {
    pub part_of_speech: String,
    pub definition: String,
}

#[derive(Debug, Clone)]
pub enum DictMessage {
    InputChanged(String),
    SearchWord,
    DefinitionsLoaded(String, Vec<MeaningEntry>),
    CopyDefinition(String),
    Dismiss,
    Refocus,
}

pub struct DictionaryOverlay {
    pub word_query: String,
    pub active_definitions: Vec<MeaningEntry>,
    pub cache: HashMap<String, Vec<MeaningEntry>>,
}

#[derive(Deserialize)]
struct ApiResponse {
    meanings: Option<Vec<ApiMeaning>>,
}

#[derive(Deserialize)]
struct ApiMeaning {
    #[serde(rename = "partOfSpeech")]
    part_of_speech: Option<String>,
    definitions: Option<Vec<ApiDef>>,
}

#[derive(Deserialize)]
struct ApiDef {
    definition: Option<String>,
}

impl Default for DictionaryOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl DictionaryOverlay {
    pub fn new() -> Self {
        Self {
            word_query: String::new(),
            active_definitions: Vec::new(),
            cache: Self::load_cache(),
        }
    }

    fn cache_path() -> PathBuf {
        let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("moon_dict_cache.json")
    }

    pub fn load_cache() -> HashMap<String, Vec<MeaningEntry>> {
        let path = Self::cache_path();
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(map) = serde_json::from_str(&raw) {
                    return map;
                }
            }
        }
        HashMap::new()
    }

    fn save_cache(&self) {
        let path = Self::cache_path();
        if let Ok(data) = serde_json::to_string(&self.cache) {
            let _ = fs::write(path, data);
        }
    }

    pub fn update(&mut self, message: DictMessage) {
        match message {
            DictMessage::InputChanged(w) => {
                self.word_query = w;
            }
            DictMessage::DefinitionsLoaded(word, defs) => {
                self.active_definitions = defs.clone();
                self.cache.insert(word.to_lowercase(), defs);
                self.save_cache();
            }
            DictMessage::CopyDefinition(def) => {
                tokio::spawn(async move {
                    let _ = Command::new("wl-copy").arg(&def).status().await;
                    let _ = Command::new("notify-send")
                        .args(["-a", "Dictionary", "Definition Copied", &def])
                        .spawn();
                });
            }
            _ => {}
        }
    }

    pub async fn lookup_word(word: &str) -> Vec<MeaningEntry> {
        let clean = word.trim().to_lowercase();
        if clean.is_empty() {
            return Vec::new();
        }

        let url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{}", clean);
        let Ok(out) = Command::new("curl")
            .args(["-s", "--max-time", "5", &url])
            .output()
            .await
        else {
            return Vec::new();
        };

        let mut results = Vec::new();
        if let Ok(api_list) = serde_json::from_slice::<Vec<ApiResponse>>(&out.stdout) {
            for entry in api_list {
                if let Some(meanings) = entry.meanings {
                    for m in meanings {
                        let pos = m.part_of_speech.unwrap_or_else(|| "word".into());
                        if let Some(defs) = m.definitions {
                            for d in defs {
                                if let Some(def_text) = d.definition {
                                    results.push(MeaningEntry {
                                        part_of_speech: pos.clone(),
                                        definition: def_text,
                                    });
                                    if results.len() >= 12 {
                                        return results;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &ShellSettings) -> Element<'a, DictMessage> {
        let (win_w, win_h) = settings.get_window_size("dictionary", 820, 600);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let pad_top = 18.0_f32;
        let pad_bottom = 16.0_f32;

        let search_inward = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        let list_top_inset = if dy > 72.0 {
            (dx * (1.0 - (72.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_bottom_inset = if dy > 45.0 {
            (dx * (1.0 - (45.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_side_inset = list_top_inset.max(list_bottom_inset).max(20.0);

        let input = text_input("Type a word and press Enter...", &self.word_query)
            .id(DICT_SEARCH_ID)
            .on_input(DictMessage::InputChanged)
            .on_submit(DictMessage::SearchWord)
            .padding(Padding { top: 12.0, right: 24.0, bottom: 12.0, left: 24.0 })
            .size((theme.font_size_overlay) as f32)
            .width(Length::Fill)
            .style(move |_t, status| text_input::Style {
                background: iced::Background::Color(theme.surface_alt),
                border: iced::Border {
                    color: match status {
                        text_input::Status::Focused { .. } => theme.focus,
                        text_input::Status::Hovered => theme.border,
                        _ => theme.border,
                    },
                    width: theme.control_border_width,
                    radius: 8.0.into(),
                },
                icon: theme.muted,
                placeholder: theme.muted,
                value: theme.text,
                selection: theme.focus,
            });

        let input_row = container(input)
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: search_inward,
                bottom: 0.0,
                left: search_inward,
            });

        // Slanted definition rows
        let mut list_col = column![].spacing(6).width(Length::Fill);
        for item in &self.active_definitions {
            let def_clone = item.definition.clone();

            let item_bg = ShapeBox::new()
                .shape(ShapeType::Slant { left: SlantSide::Left, right: SlantSide::Left })
                .background(theme.background)
                .border(theme.border, theme.control_border_width)
                .slant_width(8.0)
                .size(Length::Fill, Length::Fixed(60.0));

            let item_content = container(
                row![
                    text(format!("[{}]", item.part_of_speech))
                        .size(12)
                        .color(theme.warning)
                        .bold(),
                    text(&item.definition)
                        .size((theme.font_size_normal) as f32)
                        .color(theme.text)
                        .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .padding(Padding { top: 0.0, right: 20.0, bottom: 0.0, left: 20.0 })
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center);

            let item_btn = button(Stack::new().push(item_bg.view()).push(item_content))
                .on_press(DictMessage::CopyDefinition(def_clone))
                .padding(0)
                .width(Length::Fill)
                .height(Length::Fixed(60.0))
                .style(|_t, _s| button::Style { background: None, ..Default::default() });

            list_col = list_col.push(item_btn);
        }

        let list_container = container(scrollable(list_col).height(Length::Fill).width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: list_top_inset,
                right: list_side_inset,
                bottom: list_bottom_inset,
                left: list_side_inset,
            });

        let footer = container(
            text("Enter to search  •  Click definition to copy  •  ESC dismiss")
                .size(11)
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![input_row, list_container, footer]
                .spacing(12)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .padding(Padding {
            top: pad_top,
            right: 0.0,
            bottom: pad_bottom,
            left: 0.0,
        })
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card).center_x(Length::Fill).center_y(Length::Fill).into()
    }
}
