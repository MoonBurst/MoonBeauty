use iced::widget::{button, column, container, text, text_input, Stack};
use iced::{Alignment, Element, Length};
use tokio::process::Command;

use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

#[derive(Debug, Clone)]
pub enum WebSearchMessage {
    InputChanged(String),
    ExecuteSearch,
    Dismiss,
}

pub struct WebSearchOverlay {
    pub query: String,
}

impl Default for WebSearchOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSearchOverlay {
    pub fn new() -> Self {
        Self { query: String::new() }
    }

    pub fn update(&mut self, message: WebSearchMessage) {
        if let WebSearchMessage::InputChanged(q) = message {
            self.query = q;
        }
    }

    pub fn build_url(raw: &str) -> Option<String> {
        let q = raw.trim();
        if q.is_empty() {
            return None;
        }

        let (base, clean_q) = if let Some(stripped) = q.strip_prefix("!g ") {
            ("https://www.google.com/search?q=", stripped)
        } else if let Some(stripped) = q.strip_prefix("!d ").or_else(|| q.strip_prefix("!ddg ")) {
            ("https://duckduckgo.com/?q=", stripped)
        } else if let Some(stripped) = q.strip_prefix("!w ").or_else(|| q.strip_prefix("!wiki ")) {
            ("https://en.wikipedia.org/wiki/Special:Search?search=", stripped)
        } else if let Some(stripped) = q.strip_prefix("!gh ") {
            ("https://github.com/search?q=", stripped)
        } else if let Some(stripped) = q.strip_prefix("!nix ") {
            ("https://search.nixos.org/packages?channel=unstable&query=", stripped)
        } else {
            ("https://www.startpage.com/sp/search?query=", q)
        };

        let encoded: String = clean_q
            .chars()
            .map(|c| match c {
                ' ' => "+".into(),
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u32),
            })
            .collect();

        Some(format!("{}{}", base, encoded))
    }

    pub async fn launch_browser(url: &str) {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, WebSearchMessage> {
        let input = text_input("Type search query [!g, !d, !w, !nix, !gh]...", &self.query)
            .on_input(WebSearchMessage::InputChanged)
            .on_submit(WebSearchMessage::ExecuteSearch)
            .padding(14)
            .size((theme.font_size_overlay) as f32);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let search_label = if self.query.trim().is_empty() {
            "Search with Startpage".to_string()
        } else {
            format!("Search for \"{}\"", self.query.trim())
        };

        let card_content = container(
            column![
                text("🌐 WEB SEARCH")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.text)
                    .align_x(Alignment::Center),
                input,
                button(
                    text(search_label)
                        .size((theme.font_size_normal) as f32)
                        .color(theme.text)
                        .bold()
                        .align_x(Alignment::Center),
                )
                .on_press(WebSearchMessage::ExecuteSearch)
                .padding(12)
                .width(Length::Fill)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
                text("Press [Enter] to open browser  •  ESC to dismiss")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(16)
            .padding(24),
        )
        .width(Length::Fixed(740.0))
        .height(Length::Fixed(300.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card).center_x(Length::Fill).center_y(Length::Fill).into()
    }
}
