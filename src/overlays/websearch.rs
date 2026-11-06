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
        Self {
            query: String::new(),
        }
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
            (
                "https://en.wikipedia.org/wiki/Special:Search?search=",
                stripped,
            )
        } else if let Some(stripped) = q.strip_prefix("!gh ") {
            ("https://github.com/search?q=", stripped)
        } else if let Some(stripped) = q.strip_prefix("!nix ") {
            (
                "https://search.nixos.org/packages?channel=unstable&query=",
                stripped,
            )
        } else {
            ("https://www.startpage.com/sp/search?query=", q)
        };

        // RFC 3986 percent-encoding over UTF-8 bytes. The previous version
        // used `c as u32` (Unicode code point) which:
        //   (a) emitted the wrong byte sequence for any non-ASCII input
        //       (é U+00E9 -> "%E9", should be "%C3%A9"); and
        //   (b) emitted 3+ hex digits for code points >= U+0100, producing
        //       malformed escapes like "%100" that parsers split as
        //       "%10" + "0".
        // Iterating bytes fixes both, matching curl --data-urlencode.
        let encoded: String = clean_q
            .bytes()
            .map(|b| match b {
                b' ' => "+".into(),
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{:02X}", b),
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

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::WebSearchOverlay;

    fn encode(query: &str) -> String {
        WebSearchOverlay::build_url(query).expect("non-empty query")
    }

    #[test]
    fn ascii_space_becomes_plus() {
        assert!(encode("hello world").ends_with("hello+world"));
    }

    #[test]
    fn unreserved_ascii_passes_through() {
        assert!(encode("abcXYZ019-_.~").ends_with("abcXYZ019-_.~"));
    }

    #[test]
    fn reserved_ascii_is_percent_encoded() {
        // & = ? # / must be encoded or they break out of the query param.
        let out = encode("a&b=c?d#e/f");
        assert!(out.ends_with("a%26b%3Dc%3Fd%23e%2Ff"), "got {out}");
    }

    #[test]
    fn latin1_non_ascii_uses_utf8_bytes() {
        // é = U+00E9, UTF-8 = C3 A9. Regression: old code emitted %E9.
        let out = encode("café");
        assert!(out.ends_with("caf%C3%A9"), "got {out}");
        assert!(!out.contains("%E9"), "old broken encoding: {out}");
    }

    #[test]
    fn cjk_uses_utf8_bytes() {
        // 日 = U+65E5 (E6 97 A5); 本 = U+672C (E6 9C AC).
        let out = encode("日本");
        assert!(out.ends_with("%E6%97%A5%E6%9C%AC"), "got {out}");
    }

    #[test]
    fn emoji_uses_utf8_bytes() {
        // 🦀 = U+1F980, UTF-8 = F0 9F A6 80. Old code emitted %1F980.
        let out = encode("🦀");
        assert!(out.ends_with("%F0%9F%A6%80"), "got {out}");
    }

    #[test]
    fn no_oversized_hex_escapes() {
        // Ā = U+0100, UTF-8 = C4 80. Old code emitted %100 (malformed).
        let out = encode("Ā");
        assert!(out.ends_with("%C4%80"), "got {out}");
        let bytes = out.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                assert!(i + 2 < bytes.len(), "truncated escape at {i} in {out}");
                assert!(
                    bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit(),
                    "bad escape at {i} in {out}"
                );
                i += 3;
            } else {
                i += 1;
            }
        }
    }

    #[test]
    fn prefix_stripping_still_works() {
        assert!(encode("!g rust").starts_with("https://www.google.com/search?q="));
        assert!(encode("!ddg rust").starts_with("https://duckduckgo.com/?q="));
        assert!(encode("!w rust").contains("wikipedia.org"));
        assert!(encode("!gh rust").starts_with("https://github.com/search?q="));
        assert!(encode("!nix rust").contains("search.nixos.org"));
        assert!(encode("rust").starts_with("https://www.startpage.com/sp/search?query="));
    }

    #[test]
    fn empty_query_returns_none() {
        assert!(WebSearchOverlay::build_url("").is_none());
        assert!(WebSearchOverlay::build_url("   ").is_none());
    }
}
