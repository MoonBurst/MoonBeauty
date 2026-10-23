use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length};
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone)]
pub struct UnicodeEntry {
    pub symbol: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Clone)]
pub enum UnicodeMessage {
    SearchChanged(String),
    CopySymbol(&'static str),
    Dismiss,
}

pub struct UnicodeOverlay {
    pub search_query: String,
    pub catalog: Vec<UnicodeEntry>,
}

impl Default for UnicodeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl UnicodeOverlay {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            catalog: Self::build_catalog(),
        }
    }

    fn build_catalog() -> Vec<UnicodeEntry> {
        vec![
            UnicodeEntry { symbol: "✓", name: "check mark tick yes pass ok" },
            UnicodeEntry { symbol: "✔", name: "heavy check mark tick verified" },
            UnicodeEntry { symbol: "✕", name: "multiplication x cross cancel close delete" },
            UnicodeEntry { symbol: "✖", name: "heavy multiplication x cross" },
            UnicodeEntry { symbol: "⚠", name: "warning caution alert danger notice" },
            UnicodeEntry { symbol: "⚡", name: "lightning bolt power electric energy fast" },
            UnicodeEntry { symbol: "🔒", name: "lock secure private encrypted" },
            UnicodeEntry { symbol: "🔓", name: "unlock open insecure public" },
            UnicodeEntry { symbol: "🔑", name: "key password secret credential auth" },
            UnicodeEntry { symbol: "★", name: "black star favorite bookmark rating" },
            UnicodeEntry { symbol: "☆", name: "white star favorite outline rating" },
            UnicodeEntry { symbol: "✦", name: "four pointed black star sparkle shine" },
            UnicodeEntry { symbol: "✧", name: "four pointed white star sparkle outline" },
            UnicodeEntry { symbol: "♥", name: "heart love favorite like red" },
            UnicodeEntry { symbol: "→", name: "right arrow forward next pointer" },
            UnicodeEntry { symbol: "←", name: "left arrow back previous return" },
            UnicodeEntry { symbol: "↑", name: "up arrow top ascend increase" },
            UnicodeEntry { symbol: "↓", name: "down arrow bottom descend decrease" },
            UnicodeEntry { symbol: "↺", name: "counterclockwise arrow reload refresh reset" },
            UnicodeEntry { symbol: "↻", name: "clockwise arrow reload repeat restart" },
            UnicodeEntry { symbol: "▶", name: "play forward media start resume" },
            UnicodeEntry { symbol: "⏸", name: "pause media hold suspend" },
            UnicodeEntry { symbol: "⏹", name: "stop media finish terminate" },
            UnicodeEntry { symbol: "⏮", name: "previous track media skip rewind" },
            UnicodeEntry { symbol: "⏭", name: "next track media skip forward" },
            UnicodeEntry { symbol: "∞", name: "infinity infinite endless loop unlimited" },
            UnicodeEntry { symbol: "≈", name: "approximately equal almost similar math" },
            UnicodeEntry { symbol: "≠", name: "not equal inequality math" },
            UnicodeEntry { symbol: "≤", name: "less than or equal math comparison" },
            UnicodeEntry { symbol: "≥", name: "greater than or equal math comparison" },
            UnicodeEntry { symbol: "±", name: "plus minus sign tolerance math" },
            UnicodeEntry { symbol: "∑", name: "summation sigma sum math series" },
            UnicodeEntry { symbol: "√", name: "square root radical math" },
            UnicodeEntry { symbol: "π", name: "pi greek math circle 3.14159" },
            UnicodeEntry { symbol: "⬡", name: "hexagon geometric polygon shape" },
            UnicodeEntry { symbol: "▢", name: "white square rounded rectangle shape" },
            UnicodeEntry { symbol: "■", name: "black square block solid shape" },
            UnicodeEntry { symbol: "$", name: "dollar sign currency usd money" },
            UnicodeEntry { symbol: "€", name: "euro sign currency eur money" },
            UnicodeEntry { symbol: "£", name: "pound sign currency gbp money" },
            UnicodeEntry { symbol: "¥", name: "yen yuan sign currency jpy cny money" },
            UnicodeEntry { symbol: "₿", name: "bitcoin crypto currency btc money" },
        ]
    }

    pub fn update(&mut self, message: UnicodeMessage) {
        if let UnicodeMessage::SearchChanged(q) = message {
            self.search_query = q;
        }
    }

    pub async fn copy(symbol: &str) {
        let _ = Command::new("wl-copy").arg(symbol).status().await;
        let _ = Command::new("notify-send")
            .args(["-a", "Unicode", "Glyph Copied", symbol])
            .spawn();
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, UnicodeMessage> {
        let input = text_input("Search unicode glyphs and symbols...", &self.search_query)
            .on_input(UnicodeMessage::SearchChanged)
            .padding(14)
            .size((theme.font_size_overlay) as f32);

        let query = self.search_query.trim().to_lowercase();
        let filtered: Vec<&UnicodeEntry> = self
            .catalog
            .iter()
            .filter(|e| query.is_empty() || e.symbol == query || e.name.contains(&query))
            .collect();

        let mut grid_col = column![].spacing(8);
        for chunk in filtered.chunks(5) {
            let mut grid_row = row![].spacing(8).align_y(Alignment::Center);
            for entry in chunk {
                let sym = entry.symbol;
                let btn_item = button(
                    column![
                        text(entry.symbol).size(26).align_x(Alignment::Center),
                        text(entry.name.split_whitespace().next().unwrap_or(""))
                            .size(10)
                            .color(theme.muted)
                            .align_x(Alignment::Center),
                    ]
                    .align_x(Alignment::Center)
                    .spacing(2),
                )
                .on_press(UnicodeMessage::CopySymbol(sym))
                .padding(10)
                .width(Length::Fixed(130.0))
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });

                grid_row = grid_row.push(btn_item);
            }
            grid_col = grid_col.push(grid_row);
        }

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                text("🔣 UNICODE SEARCH")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.text),
                input,
                scrollable(grid_col).height(Length::Fill),
                text("Click glyph to copy  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(780.0))
        .height(Length::Fixed(600.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(UnicodeMessage::Dismiss)
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
