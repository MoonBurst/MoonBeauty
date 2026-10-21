use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

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

    fn load_cache() -> HashMap<String, Vec<MeaningEntry>> {
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

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, DictMessage> {
        let input = text_input("Type word to look up [Enter to search]...", &self.word_query)
            .on_input(DictMessage::InputChanged)
            .on_submit(DictMessage::SearchWord)
            .padding(14)
            .size(theme.font_size_overlay);

        let mut list_col = column![].spacing(8);
        for item in &self.active_definitions {
            let def_clone = item.definition.clone();
            let row_item = button(
                row![
                    text(format!("[{}]", item.part_of_speech))
                        .size(12)
                        .color(theme.warning)
                        .bold(),
                    text(&item.definition)
                        .size(theme.font_size_normal)
                        .color(theme.text)
                        .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .on_press(DictMessage::CopyDefinition(def_clone))
            .padding([8, 12])
            .width(Length::Fill)
            .style(|_t, _s| button::Style { background: None, ..Default::default() });

            list_col = list_col.push(row_item);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("📖 DICTIONARY")
                    .size(theme.font_size_overlay)
                    .color(theme.text),
                input,
                scrollable(list_col).height(Length::Fill),
                text("Click definition to copy  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(820.0))
        .height(Length::Fixed(600.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(DictMessage::Dismiss)
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
