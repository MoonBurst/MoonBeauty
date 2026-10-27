use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack};
use iced::{Alignment, Element, Length, Padding};
use tokio::process::Command;

use crate::libraries::config::theme::ShellTheme;
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone)]
pub struct UnicodeEntry {
    pub symbol: &'static str,
    pub name: &'static str,
}

pub const UNICODE_INPUT_ID: &str = "unicode_search";

#[derive(Debug, Clone)]
pub enum UnicodeMessage {
    SearchChanged(String),
    CopySymbol(&'static str),
    SelectNext,
    SelectPrev,
    SelectNextRow,
    SelectPrevRow,
    CopySelected,
    Refocus,
    Dismiss,
}

pub struct UnicodeOverlay {
    pub search_query: String,
    pub catalog: Vec<UnicodeEntry>,
    pub selected_idx: Option<usize>,
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
            selected_idx: None,
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

    fn filtered(&self) -> Vec<&UnicodeEntry> {
        let q = self.search_query.trim().to_lowercase();
        self.catalog.iter()
            .filter(|e| q.is_empty() || e.symbol == q || e.name.contains(&q))
            .collect()
    }

    pub fn update(&mut self, message: UnicodeMessage) {
        match message {
            UnicodeMessage::SearchChanged(q) => {
                self.search_query = q;
                self.selected_idx = None;
            }
            UnicodeMessage::SelectNext => {
                if !self.search_query.is_empty() { return; }
                let n = self.filtered().len();
                if n == 0 { return; }
                let cur = self.selected_idx.unwrap_or(usize::MAX);
                self.selected_idx = Some(if cur == usize::MAX { 0 } else { (cur + 1) % n });
            }
            UnicodeMessage::SelectPrev => {
                if !self.search_query.is_empty() { return; }
                let n = self.filtered().len();
                if n == 0 { return; }
                let cur = self.selected_idx.unwrap_or(usize::MAX);
                self.selected_idx = Some(if cur == usize::MAX { n.saturating_sub(1) } else { (cur + n - 1) % n });
            }
            UnicodeMessage::SelectNextRow => {
                if !self.search_query.is_empty() { return; }
                let n = self.filtered().len();
                if n == 0 { return; }
                let cur = self.selected_idx.unwrap_or(0);
                self.selected_idx = Some((cur + 5).min(n - 1));
            }
            UnicodeMessage::SelectPrevRow => {
                if !self.search_query.is_empty() { return; }
                let n = self.filtered().len();
                if n == 0 { return; }
                let cur = self.selected_idx.unwrap_or(0);
                self.selected_idx = Some(cur.saturating_sub(5));
            }
            UnicodeMessage::CopySelected => {
                if let Some(idx) = self.selected_idx {
                    if let Some(entry) = self.filtered().get(idx) {
                        let sym = entry.symbol;
                        tokio::spawn(async move {
                            let _ = Command::new("wl-copy").arg(sym).status().await;
                            let _ = Command::new("notify-send")
                                .args(["-a", "Unicode", "Glyph Copied", sym])
                                .spawn();
                        });
                    }
                }
            }
            UnicodeMessage::Refocus => {}
            UnicodeMessage::Dismiss => {}
            UnicodeMessage::CopySymbol(_) => {}
        }
    }

    pub async fn copy(symbol: &str) {
        let _ = Command::new("wl-copy").arg(symbol).status().await;
        let _ = Command::new("notify-send")
            .args(["-a", "Unicode", "Glyph Copied", symbol])
            .spawn();
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &ShellSettings) -> Element<'a, UnicodeMessage> {
        let (win_w, win_h) = settings.get_window_size("unicode", 780, 620);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top { (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0 } else { 16.0 };
        let footer_inset = if dy > pad_bottom { (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0 } else { 16.0 };
        let body_inset = header_inset.min(footer_inset) + 4.0;

        let input = text_input("Search unicode glyphs and symbols...", &self.search_query)
            .id(UNICODE_INPUT_ID)
            .on_input(UnicodeMessage::SearchChanged)
            .padding(Padding { top: 18.0, bottom: 18.0, left: 20.0, right: 20.0 })
            .size(theme.font_size_overlay as f32);

        let filtered = self.filtered();
        let item_gap = 8.0;
        let per_row = 5;
        let grid_usable = win_w - body_inset * 2.0;
        let item_w = (grid_usable - (per_row as f32 - 1.0) * item_gap) / per_row as f32;
        let sym_size = (theme.font_size_overlay as f32) * 1.6;
        let label_size = (theme.font_size_overlay as f32) * 0.65;

        let mut grid_col = column![].spacing(item_gap);

        for (row_idx, chunk) in filtered.chunks(per_row).enumerate() {
            let mut grid_row = row![].spacing(item_gap).align_y(Alignment::Center);
            for (col_idx, entry) in chunk.iter().enumerate() {
                let global_idx = row_idx * per_row + col_idx;
                let is_sel = self.selected_idx == Some(global_idx);
                let sym = entry.symbol;

                let bg = if is_sel { Some(iced::Background::Color(theme.surface_alt)) } else { None };
                let border_col = if is_sel { theme.text } else { theme.border };
                let border_w = if is_sel { theme.control_border_width } else { 0.0 };

                let item_content = column![
                    text(entry.symbol).size(sym_size).color(theme.text).align_x(Alignment::Center),
                    text(entry.name.split_whitespace().next().unwrap_or(""))
                        .size(label_size)
                        .color(theme.text)
                        .align_x(Alignment::Center),
                ]
                .align_x(Alignment::Center)
                .spacing(2);

                let item_btn = button(
                    container(item_content)
                        .padding([10, 4])
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .style(move |_t| container::Style {
                            background: bg,
                            border: iced::Border { color: border_col, width: border_w, radius: 4.0.into() },
                            ..Default::default()
                        })
                )
                .on_press(UnicodeMessage::CopySymbol(sym))
                .padding(0)
                .width(Length::Fixed(item_w))
                .height(Length::Fixed(66.0))
                .style(|_t, _s| button::Style { background: None, ..Default::default() });

                grid_row = grid_row.push(item_btn);
            }
            // Pad incomplete last row so items don't stretch
            for _ in chunk.len()..per_row {
                grid_row = grid_row.push(
                    container(text(""))
                        .width(Length::Fixed(item_w))
                        .height(Length::Fixed(66.0))
                );
            }
            grid_col = grid_col.push(grid_row);
        }

        let header_row = container(
            text("🔣 UNICODE SEARCH").size(theme.font_size_overlay as f32).color(theme.text)
        )
        .width(Length::Fill)
        .padding(Padding { top: 0.0, right: header_inset, bottom: 0.0, left: header_inset });

        let input_row = container(input)
            .width(Length::Fill)
            .padding(Padding { top: 0.0, right: body_inset, bottom: 0.0, left: body_inset });

        let list_container = container(scrollable(grid_col).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding { top: 0.0, right: body_inset, bottom: 0.0, left: body_inset });

        let footer_row = container(
            text("Click glyph to copy  •  ↑/↓/←/→ navigate  •  Enter copy  •  ESC dismiss")
                .size((theme.font_size_overlay as f32) * 0.85)
                .color(theme.muted)
                .align_x(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding { top: 0.0, right: footer_inset, bottom: 0.0, left: footer_inset });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![header_row, input_row, list_container, footer_row]
                .spacing(14)
                .padding(Padding { top: pad_top, right: 0.0, bottom: pad_bottom, left: 0.0 }),
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
