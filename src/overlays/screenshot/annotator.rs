//! Annotator overlay.
//! - Watermark: LSB steganography — 1 bit per RGB channel, imperceptible.
//! - Reveal: decodes LSBs in-memory, shows recovered text as a UI overlay.
//!   No image capture, no temp files, no ImageMagick for reveal.
//! - Save: composes annotations via ImageMagick, then LSB-embeds watermark
//!   per recipient, writes `annotated_STAMP_{name}.png`.
//! - Enter: save (annotated+watermarked) then close. Always.

use iced::widget::canvas::{self, Canvas, Frame, Geometry};
use iced::widget::{button, container, text, text_input, Column, Row, Stack};
use iced::{mouse, Alignment, Color, Element, Length, Point, Rectangle, Size};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Rect,
    Ellipse,
    Arrow,
    Pen,
    Text,
    Counter,
    Redact,
}
impl Tool {
    fn glyph(self) -> &'static str {
        match self {
            Self::Rect => "Rect",
            Self::Ellipse => "Circle",
            Self::Arrow => "Arrow",
            Self::Pen => "Pen",
            Self::Text => "Text",
            Self::Counter => "Num",
            Self::Redact => "Redact",
        }
    }
    pub fn all() -> [Self; 7] {
        [
            Self::Rect,
            Self::Ellipse,
            Self::Arrow,
            Self::Pen,
            Self::Text,
            Self::Counter,
            Self::Redact,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub tool: Tool,
    pub start: Point,
    pub end: Point,
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
    pub number: u32,
    pub text: String,
    pub font_size: f32,
}

#[derive(Debug, Clone)]
pub enum AnnotatorMessage {
    Pressed(Point),
    Moved(Point),
    Released(Point),
    SetTool(Tool),
    SetColor(Color),
    Undo,
    ClearAll,
    Save,
    Copy,
    Cancel,
    Loaded(PathBuf, Vec<u8>),
    Saved(Result<Vec<PathBuf>, String>),
    WatermarkInputChanged(String),
    ToggleReveal,
    RevealLoaded(Result<Vec<u8>, String>),
    TextEditingChanged(String),
    TextCommitted,
    CharacterInput(String),
    BackspacePressed,
    CancelTextEditing,
    ConfirmText,
    CancelText,
    FontSizeUp,
    FontSizeDown,
    NewLinePressed,
}

pub const PALETTE: [Color; 8] = [
    Color::from_rgb(1.0, 0.27, 0.23),
    Color::from_rgb(1.0, 0.62, 0.04),
    Color::from_rgb(1.0, 0.84, 0.04),
    Color::from_rgb(0.20, 0.84, 0.29),
    Color::from_rgb(0.04, 0.52, 1.0),
    Color::from_rgb(0.37, 0.36, 0.90),
    Color::from_rgb(1.0, 1.0, 1.0),
    Color::from_rgb(0.11, 0.11, 0.12),
];

pub struct AnnotatorOverlay {
    pub handle: Option<iced::widget::image::Handle>,
    pub text_editing: Option<usize>,
    pub text_buffer: String,
    pub text_dragging: bool,
    pub text_drag_offset: Point,
    pub text_edit_original: Option<String>,
    pub text_font_size: f32,
    pub image_size: Size,
    pub annotations: Vec<Annotation>,
    in_progress: Option<Annotation>,
    pub tool: Tool,
    pub color: Color,
    pub stroke_width: f32,
    pub status: String,
    pub source_path: Option<PathBuf>,
    pub watermark_input: String,
    pub watermark_cli: Option<String>,
    drawing: bool,
    pub reveal_mode: bool,
    pub reveal_text: Option<String>,
    pub reveal_handle: Option<iced::widget::image::Handle>,
    pub reveal_bytes: Option<Vec<u8>>,
    pub reveal_computing: bool,
}

impl Default for AnnotatorOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl AnnotatorOverlay {
    pub fn new() -> Self {
        Self {
            handle: None,
            text_editing: None,
            text_buffer: String::new(),
            text_dragging: false,
            text_drag_offset: Point::ORIGIN,
            text_edit_original: None,
            text_font_size: 20.0,
            image_size: Size::new(0.0, 0.0),
            annotations: Vec::new(),
            in_progress: None,
            tool: Tool::Rect,
            color: PALETTE[0],
            stroke_width: 4.0,
            status: "Draw · Enter saves & closes · ESC cancels · 👁 reveals watermark".into(),
            source_path: None,
            watermark_input: String::new(),
            watermark_cli: None,
            drawing: false,
            reveal_mode: false,
            reveal_text: None,
            reveal_handle: None,
            reveal_bytes: None,
            reveal_computing: false,
        }
    }

    pub fn load_image(&mut self, path: PathBuf, bytes: Vec<u8>) {
        match ::image::load_from_memory(&bytes) {
            Ok(img) => {
                self.image_size = Size::new(img.width() as f32, img.height() as f32);
                self.handle = Some(iced::widget::image::Handle::from_bytes(bytes));
                self.source_path = Some(path);
                self.status = format!(
                    "{}×{} · draw · Enter saves & closes",
                    img.width(),
                    img.height()
                );
            }
            Err(e) => self.status = format!("decode failed: {e}"),
        }
    }

    pub fn update(&mut self, msg: AnnotatorMessage) {
        match msg {
            AnnotatorMessage::Pressed(p) if !self.reveal_mode => self.on_press(p),
            AnnotatorMessage::Moved(p) if !self.reveal_mode => self.on_move(p),
            AnnotatorMessage::Released(p) if !self.reveal_mode => self.on_release(p),
            AnnotatorMessage::SetTool(t) => self.tool = t,
            AnnotatorMessage::SetColor(c) => self.color = c,
            AnnotatorMessage::Undo => {
                self.annotations.pop();
            }
            AnnotatorMessage::ClearAll => self.annotations.clear(),
            AnnotatorMessage::WatermarkInputChanged(s) => self.watermark_input = s,

            AnnotatorMessage::CharacterInput(s) => {
                if let Some(idx) = self.text_editing {
                    self.text_buffer.push_str(&s);
                    if let Some(a) = self.annotations.get_mut(idx) {
                        a.text = self.text_buffer.clone();
                    }
                }
            }
            AnnotatorMessage::BackspacePressed => {
                if let Some(idx) = self.text_editing {
                    self.text_buffer.pop();
                    if let Some(a) = self.annotations.get_mut(idx) {
                        a.text = self.text_buffer.clone();
                    }
                }
            }
            AnnotatorMessage::NewLinePressed => {
                if let Some(idx) = self.text_editing {
                    self.text_buffer.push('\n');
                    if let Some(a) = self.annotations.get_mut(idx) {
                        a.text = self.text_buffer.clone();
                    }
                }
            }
            AnnotatorMessage::ConfirmText => {
                if let Some(idx) = self.text_editing.take() {
                    if self.text_buffer.trim().is_empty() {
                        self.annotations.remove(idx);
                    } else if let Some(a) = self.annotations.get_mut(idx) {
                        a.text = self.text_buffer.clone();
                        a.font_size = self.text_font_size;
                    }
                }
                self.text_edit_original = None;
            }
            AnnotatorMessage::CancelText => {
                if let Some(idx) = self.text_editing.take() {
                    match self.text_edit_original.take() {
                        Some(original) => {
                            // Resume-cancel: restore original text
                            if let Some(a) = self.annotations.get_mut(idx) {
                                a.text = original;
                            }
                        }
                        None => {
                            // New-cancel: remove the empty annotation
                            self.annotations.remove(idx);
                        }
                    }
                }
                self.text_buffer.clear();
            }
            AnnotatorMessage::FontSizeUp => {
                self.text_font_size = (self.text_font_size + 2.0).min(96.0);
                if let Some(idx) = self.text_editing {
                    if let Some(a) = self.annotations.get_mut(idx) {
                        a.font_size = self.text_font_size;
                    }
                }
            }
            AnnotatorMessage::FontSizeDown => {
                self.text_font_size = (self.text_font_size - 2.0).max(10.0);
                if let Some(idx) = self.text_editing {
                    if let Some(a) = self.annotations.get_mut(idx) {
                        a.font_size = self.text_font_size;
                    }
                }
            }
            _ => {}
        }
    }

    fn on_press(&mut self, p: Point) {
        crate::dbg_log!(
            "[mbs-child] on_press ({:.0},{:.0}) tool={:?}",
            p.x,
            p.y,
            self.tool
        );

        // If already editing text, check if click is inside the box (for drag)
        if let Some(idx) = self.text_editing {
            let in_box = if let Some(a) = self.annotations.get(idx) {
                let font_size = a.font_size.max(10.0);
                let line_height = font_size * 1.2;
                let max_chars = a
                    .text
                    .lines()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(4);
                let pad = 24.0;
                let bw = (max_chars as f32 * font_size * 0.55) + pad * 2.0;
                let bh = (a.text.lines().count() as f32).max(1.0) * line_height + pad * 2.0;
                let bx = a.start.x - pad;
                let by = a.start.y - pad;
                p.x >= bx && p.x <= bx + bw && p.y >= by && p.y <= by + bh
            } else {
                false
            };

            if in_box {
                self.text_dragging = true;
                if let Some(a) = self.annotations.get(idx) {
                    self.text_drag_offset = Point::new(p.x - a.start.x, p.y - a.start.y);
                }
                crate::dbg_log!(
                    "[mbs-child] Text drag started, offset=({:.0},{:.0})",
                    self.text_drag_offset.x,
                    self.text_drag_offset.y
                );
                return;
            }

            // Click outside → commit current text and start new
            if let Some(a) = self.annotations.get(idx) {
                if a.text.trim().is_empty() {
                    self.annotations.remove(idx);
                }
            }
            self.text_editing = None;
            self.text_buffer.clear();
            self.text_edit_original = None;
        }

        // Text tool doesn't use drag; it opens a multiline editor at the click position
        if self.tool == Tool::Text {
            crate::dbg_log!("[mbs-child] Text tool pressed at ({:.0},{:.0})", p.x, p.y);
            // Hit-test existing text annotations (newest first = topmost)
            for idx in (0..self.annotations.len()).rev() {
                let a = &self.annotations[idx];
                if a.tool != Tool::Text {
                    continue;
                }
                let font_size = a.font_size.max(10.0);
                let line_height = font_size * 1.2;
                let max_chars = a
                    .text
                    .lines()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(4);
                let pad = 24.0;
                let bw = (max_chars as f32 * font_size * 0.55) + pad * 2.0;
                let bh = (a.text.lines().count() as f32).max(1.0) * line_height + pad * 2.0;
                let bx = a.start.x - pad;
                let by = a.start.y - pad;
                if p.x >= bx && p.x <= bx + bw && p.y >= by && p.y <= by + bh {
                    crate::dbg_log!("[mbs-child] Resuming text annotation idx={}", idx);
                    self.text_buffer = a.text.clone();
                    self.text_font_size = a.font_size;
                    self.text_edit_original = Some(a.text.clone());
                    self.text_editing = Some(idx);
                    return;
                }
            }

            // No existing hit — start a new text annotation
            self.text_edit_original = None;
            self.text_buffer = String::new();
            self.text_font_size = 20.0;
            let idx = self.annotations.len();
            self.annotations.push(Annotation {
                tool: Tool::Text,
                start: p,
                end: p,
                points: Vec::new(),
                color: self.color,
                width: self.stroke_width,
                number: 0,
                text: String::new(),
                font_size: self.text_font_size,
            });
            self.text_buffer = String::new();
            self.text_font_size = 20.0;
            self.text_editing = Some(idx);
            crate::dbg_log!("[mbs-child] Text editing started, idx={}", idx);
            return;
        }

        self.drawing = true;
        let counter_num = self
            .annotations
            .iter()
            .filter(|a| a.tool == Tool::Counter)
            .count() as u32
            + 1;
        self.in_progress = Some(Annotation {
            tool: self.tool,
            start: p,
            end: p,
            points: if self.tool == Tool::Pen {
                vec![p]
            } else {
                Vec::new()
            },
            color: self.color,
            width: self.stroke_width,
            number: if self.tool == Tool::Counter {
                counter_num
            } else {
                0
            },
            text: String::new(),
            font_size: 0.0, // only used by Text tool
        });
    }
    fn on_move(&mut self, p: Point) {
        if self.text_dragging {
            if let Some(idx) = self.text_editing {
                if let Some(a) = self.annotations.get_mut(idx) {
                    a.start =
                        Point::new(p.x - self.text_drag_offset.x, p.y - self.text_drag_offset.y);
                }
            }
            return;
        }
        if !self.drawing {
            return;
        }
        if let Some(ref mut ann) = self.in_progress {
            if ann.tool == Tool::Pen {
                ann.points.push(p); // Accumulate points for pen
            } else {
                ann.end = p; // Update end point for shapes
            }
        }
    }
    fn on_release(&mut self, p: Point) {
        if self.text_dragging {
            self.text_dragging = false;
            return;
        }
        crate::dbg_log!(
            "[mbs-child] on_release ({:.0},{:.0}) in_progress={}",
            p.x,
            p.y,
            self.in_progress.is_some()
        );
        crate::dbg_log!(
            "[mbs-child] on_release at ({:.0},{:.0}) — in_progress={}",
            p.x,
            p.y,
            self.in_progress.is_some()
        );
        self.drawing = false;
        if let Some(mut a) = self.in_progress.take() {
            if a.tool != Tool::Pen {
                a.end = p;
            }
            let ok = match a.tool {
                Tool::Pen => a.points.len() > 2,
                Tool::Counter => true, // single-click is valid for counters
                _ => (a.end.x - a.start.x).abs() > 0.5 || (a.end.y - a.start.y).abs() > 0.5,
            };
            if ok {
                crate::dbg_log!(
                    "[mbs-child] annotation ACCEPTED: {} total",
                    self.annotations.len() + 1
                );
                self.annotations.push(a);
            } else {
                crate::dbg_log!(
                    "[mbs-child] annotation REJECTED (too small: dx={:.2} dy={:.2})",
                    (a.end.x - a.start.x).abs(),
                    (a.end.y - a.start.y).abs()
                );
            }
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a, AnnotatorMessage> {
        if self.handle.is_none() {
            return container(text(self.status.clone()).size(18).color(Color::WHITE))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_t| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.05, 0.02, 0.02))),
                    ..Default::default()
                })
                .into();
        }

        // Image widget (below) + canvas overlay (above)
        let active_handle = if self.reveal_mode {
            self.reveal_handle.clone().or_else(|| self.handle.clone())
        } else {
            self.handle.clone()
        };

        // ContentFit::Contain preserves 1:1 pixel mapping so the auto-leveled
        // high-pass reveal isn't blurred by upsampling. Source is also shown
        // at 1:1 this way (matches how QML displays the reveal overlay).
        let image_widget = iced::widget::image(active_handle.unwrap())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(iced::ContentFit::Contain);

        let canvas = Canvas::new(OverlayCanvas {
            handle: None,
            image_size: self.image_size,
            annotations: if self.reveal_mode {
                Vec::new()
            } else {
                self.annotations.clone()
            },
            in_progress: if self.reveal_mode {
                None
            } else {
                self.in_progress.clone()
            },
            reveal_mode: self.reveal_mode,
            text_editing: self.text_editing.is_some(),
            editing_idx: self.text_editing,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        let reveal_banner: Element<'a, AnnotatorMessage> =
            if self.reveal_mode && self.reveal_text.is_some() {
                let txt = self.reveal_text.clone().unwrap_or_default();
                container(
                    text(txt)
                        .size(56)
                        .color(Color::from_rgb(0.2, 1.0, 0.3))
                        .font(iced::Font {
                            weight: iced::font::Weight::Bold,
                            ..Default::default()
                        }),
                )
                .width(Length::Fill)
                .padding([32, 20])
                .center_x(Length::Fill)
                .style(|_t| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.88,
                    ))),
                    border: iced::Border {
                        color: Color::WHITE,
                        width: 2.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                })
                .into()
            } else if self.reveal_mode {
                // Plate case: amplified image is the reveal. No banner.
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fixed(0.0))
                    .into()
            } else {
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fixed(0.0))
                    .into()
            };

        let canvas_stack = container(
            Stack::new().push(image_widget).push(canvas).push(
                container(reveal_banner)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::End)
                    .padding(iced::Padding {
                        top: 0.0,
                        right: 0.0,
                        bottom: 40.0,
                        left: 0.0,
                    }),
            ),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_t| container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.02, 0.02, 0.03))),
            ..Default::default()
        });

        // Toolbar
        let mut tool_row = Row::new().spacing(6).align_y(Alignment::Center);
        for t in Tool::all() {
            let active = self.tool == t;
            tool_row = tool_row.push(
                button(text(t.glyph()).size(20))
                    .on_press(AnnotatorMessage::SetTool(t))
                    .padding([6, 12])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(if active {
                            Color::from_rgb(0.04, 0.52, 1.0)
                        } else {
                            Color::from_rgba(0.15, 0.15, 0.20, 0.9)
                        })),
                        text_color: Color::WHITE,
                        border: iced::Border {
                            color: Color::WHITE,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }

        let mut color_row = Row::new().spacing(4).align_y(Alignment::Center);
        for c in PALETTE {
            let active = self.color == c;
            color_row = color_row.push(
                button(text("").size(1))
                    .on_press(AnnotatorMessage::SetColor(c))
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0))
                    .padding(0)
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(c)),
                        border: iced::Border {
                            color: if active {
                                Color::WHITE
                            } else {
                                Color::from_rgb(0.3, 0.3, 0.4)
                            },
                            width: if active { 2.5 } else { 1.0 },
                            radius: 12.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }

        let wm_input = text_input("recipient(s) for LSB watermark", &self.watermark_input)
            .on_input(AnnotatorMessage::WatermarkInputChanged)
            .on_submit(AnnotatorMessage::Save)
            .padding([6, 10])
            .size(13)
            .width(Length::Fixed(220.0));

        let reveal_label = if self.reveal_mode {
            "◀ Exit Reveal"
        } else {
            "👁 Reveal"
        };
        let reveal_btn = button(text(reveal_label).size(13))
            .on_press(AnnotatorMessage::ToggleReveal)
            .padding([6, 12])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if self.reveal_mode {
                    Color::from_rgb(0.45, 0.25, 0.85)
                } else {
                    Color::from_rgb(0.04, 0.52, 1.0)
                })),
                text_color: Color::WHITE,
                border: iced::Border {
                    color: Color::WHITE,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });

        let mk = |lbl: &'static str, msg: AnnotatorMessage| {
            button(text(lbl).size(13))
                .on_press(msg)
                .padding([6, 12])
                .style(|_t, _s| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.15, 0.15, 0.20, 0.9,
                    ))),
                    text_color: Color::WHITE,
                    border: iced::Border {
                        color: Color::WHITE,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
        };

        let mut font_controls = Row::new().spacing(6).align_y(Alignment::Center);
        if self.text_editing.is_some() {
            font_controls = font_controls
                .push(text(" | Font:").size(13).color(Color::WHITE))
                .push(
                    button(text("-").size(16))
                        .on_press(AnnotatorMessage::FontSizeDown)
                        .padding([2, 8]),
                )
                .push(
                    text(format!("{:.0}px", self.text_font_size))
                        .size(14)
                        .color(Color::WHITE)
                        .width(Length::Fixed(45.0)),
                )
                .push(
                    button(text("+").size(16))
                        .on_press(AnnotatorMessage::FontSizeUp)
                        .padding([2, 8]),
                );
        }

        let toolbar = container(
            Column::new()
                .push(
                    Row::new()
                        .push(text("Tools:").size(13).color(Color::WHITE))
                        .push(tool_row)
                        .push(text(" ").size(13))
                        .push(color_row)
                        .push(font_controls) // <-- Added font controls here
                        .spacing(10)
                        .align_y(Alignment::Center),
                )
                .push(
                    Row::new()
                        .push(text("Watermark:").size(13).color(Color::WHITE))
                        .push(wm_input)
                        .push(reveal_btn)
                        .push(mk("Undo", AnnotatorMessage::Undo))
                        .push(mk("Clear", AnnotatorMessage::ClearAll))
                        .push(mk("Cancel", AnnotatorMessage::Cancel))
                        .spacing(8)
                        .align_y(Alignment::Center),
                )
                .spacing(6),
        )
        .width(Length::Fill)
        .padding([10, 12])
        .style(|_t| container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.05, 0.05, 0.08))),
            ..Default::default()
        });

        let status_bar = if self.text_editing.is_some() {
            let hint = format!("[TEXT MODE] Drag the box to reposition · F2=place · Esc=cancel · [ ]=font ({:.0}px)", self.text_font_size);
            container(text(hint).size(12).color(Color::WHITE))
                .width(Length::Fill)
                .padding([6, 14])
                .style(|_t| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.2, 0.2, 0.25))),
                    ..Default::default()
                })
        } else {
            container(
                text(self.status.clone())
                    .size(12)
                    .color(Color::from_rgb(0.7, 0.7, 0.75)),
            )
            .width(Length::Fill)
            .padding([6, 14])
            .style(|_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.05, 0.05, 0.08))),
                ..Default::default()
            })
        };

        Column::new()
            .push(toolbar)
            .push(canvas_stack)
            .push(status_bar)
            .into()
    }
}

// ── Canvas ─────────────────────────────────────────────────────────────

#[derive(Clone)]
#[allow(dead_code)]
struct OverlayCanvas {
    handle: Option<iced::widget::image::Handle>,
    image_size: Size,
    annotations: Vec<Annotation>,
    in_progress: Option<Annotation>,
    reveal_mode: bool,
    text_editing: bool,
    editing_idx: Option<usize>,
}

fn compute_display_rect(bounds: Rectangle, image: Size) -> Option<Rectangle> {
    if image.width <= 0.0 || image.height <= 0.0 {
        return None;
    }
    let fit = (bounds.width / image.width).min(bounds.height / image.height);
    let scale = fit;
    let w = image.width * scale;
    let h = image.height * scale;
    Some(Rectangle::new(
        Point::new((bounds.width - w) / 2.0, (bounds.height - h) / 2.0),
        Size::new(w, h),
    ))
}

impl canvas::Program<AnnotatorMessage> for OverlayCanvas {
    type State = ();
    fn draw(
        &self,
        _s: &(),
        renderer: &iced::Renderer,
        _t: &iced::Theme,
        bounds: Rectangle,
        _c: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let dr_opt = compute_display_rect(bounds, self.image_size);

        if let Some(dr) = dr_opt {
            if !self.reveal_mode {
                let scale = dr.width / self.image_size.width;
                let to_disp = |p: Point| Point::new(dr.x + p.x * scale, dr.y + p.y * scale);
                let redacts: Vec<(usize, Point, Point)> = self
                    .annotations
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.tool == Tool::Redact)
                    .map(|(i, a)| (i, to_disp(a.start), to_disp(a.end)))
                    .collect();
                for (idx, ann) in self.annotations.iter().enumerate() {
                    draw_ann(&mut frame, ann, &to_disp, scale, &redacts, idx);
                    if self.editing_idx == Some(idx) && ann.tool == Tool::Text {
                        let p = to_disp(ann.start);
                        let font_size = (ann.font_size * scale).max(8.0);
                        let line_height = font_size * 1.2;
                        let max_chars = ann
                            .text
                            .lines()
                            .map(|l| l.chars().count())
                            .max()
                            .unwrap_or(1);
                        let est_w = max_chars as f32 * font_size * 0.55 + 12.0;
                        let est_h = (ann.text.lines().count() as f32).max(1.0) * line_height + 8.0;
                        let bx = p.x - 6.0;
                        let by = p.y - 4.0;
                        let bc = Color::from_rgba(1.0, 1.0, 1.0, 0.6);
                        let sw = 1.5;
                        frame.fill_rectangle(Point::new(bx, by), Size::new(est_w, sw), bc);
                        frame.fill_rectangle(
                            Point::new(bx, by + est_h - sw),
                            Size::new(est_w, sw),
                            bc,
                        );
                        frame.fill_rectangle(Point::new(bx, by), Size::new(sw, est_h), bc);
                        frame.fill_rectangle(
                            Point::new(bx + est_w - sw, by),
                            Size::new(sw, est_h),
                            bc,
                        );
                    }
                }
                if let Some(a) = &self.in_progress {
                    draw_ann(
                        &mut frame,
                        a,
                        &to_disp,
                        scale,
                        &redacts,
                        self.annotations.len(),
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        _s: &mut (),
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<iced::widget::Action<AnnotatorMessage>> {
        if self.reveal_mode {
            return None;
        }
        let dr = compute_display_rect(bounds, self.image_size)?;
        let scale = (dr.width / self.image_size.width).max(0.0001);
        let to_img = |p: Point| Point::new((p.x - dr.x) / scale, (p.y - dr.y) / scale);
        let clamp_img = |p: Point| {
            Point::new(
                p.x.clamp(0.0, self.image_size.width),
                p.y.clamp(0.0, self.image_size.height),
            )
        };
        let inside_img = |p: Point| {
            p.x >= 0.0
                && p.x <= self.image_size.width
                && p.y >= 0.0
                && p.y <= self.image_size.height
        };
        if let iced::Event::Mouse(m) = event {
            match m {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(p) = cursor.position_in(bounds) {
                        let img = to_img(p);
                        if inside_img(img) {
                            return Some(iced::widget::Action::publish(AnnotatorMessage::Pressed(
                                img,
                            )));
                        }
                        return None;
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if let Some(p) = cursor.position_in(bounds) {
                        return Some(iced::widget::Action::publish(AnnotatorMessage::Moved(
                            clamp_img(to_img(p)),
                        )));
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if let Some(p) = cursor.position_in(bounds) {
                        return Some(iced::widget::Action::publish(AnnotatorMessage::Released(
                            clamp_img(to_img(p)),
                        )));
                    }
                }
                _ => {}
            }
        }
        None
    }
}

fn draw_ann<F: Fn(Point) -> Point>(
    frame: &mut Frame,
    a: &Annotation,
    to_disp: &F,
    scale: f32,
    redacts: &[(usize, Point, Point)],
    ann_idx: usize,
) {
    let sp = to_disp;
    let sw = (a.width * scale).max(3.0);
    let r = a.color;

    // Skip text/counter if a LATER redact covers its position.
    // (Text always renders on top of shapes in iced — so we manually
    //  hide it if a redact drawn afterward would have covered it.)
    if matches!(a.tool, Tool::Counter | Tool::Text) {
        let p = sp(a.start);
        let covered = redacts.iter().any(|&(ridx, ra, rb)| {
            ridx > ann_idx
                && p.x >= ra.x.min(rb.x)
                && p.x <= ra.x.max(rb.x)
                && p.y >= ra.y.min(rb.y)
                && p.y <= ra.y.max(rb.y)
        });
        if covered {
            return;
        }
    }

    match a.tool {
        Tool::Rect => {
            let p1 = sp(a.start);
            let p2 = sp(a.end);
            let x = p1.x.min(p2.x);
            let y = p1.y.min(p2.y);
            let w = (p2.x - p1.x).abs().max(6.0);
            let h = (p2.y - p1.y).abs().max(6.0);
            frame.fill_rectangle(Point::new(x, y), Size::new(w, sw), r);
            frame.fill_rectangle(Point::new(x, y + h - sw), Size::new(w, sw), r);
            frame.fill_rectangle(Point::new(x, y), Size::new(sw, h), r);
            frame.fill_rectangle(Point::new(x + w - sw, y), Size::new(sw, h), r);
        }
        Tool::Redact => {
            let p1 = sp(a.start);
            let p2 = sp(a.end);
            let x = p1.x.min(p2.x);
            let y = p1.y.min(p2.y);
            let w = (p2.x - p1.x).abs().max(4.0);
            let h = (p2.y - p1.y).abs().max(4.0);
            // Draw redact as a massive black block. It covers other shapes.
            frame.fill_rectangle(Point::new(x, y), Size::new(w, h), Color::BLACK);
        }
        Tool::Arrow => {
            let p1 = sp(a.start);
            let p2 = sp(a.end);
            let dx = p2.x - p1.x;
            let dy = p2.y - p1.y;
            let ang = dy.atan2(dx);
            let hl = (sw * 4.0).max(14.0);
            let spread = 0.45;

            // Arrow shaft (stops slightly before the tip)
            let body_path = canvas::Path::new(|b| {
                b.move_to(p1);
                let bx = p2.x - hl * 0.6 * ang.cos();
                let by = p2.y - hl * 0.6 * ang.sin();
                b.line_to(Point::new(bx, by));
            });
            frame.stroke(
                &body_path,
                canvas::Stroke::default()
                    .with_color(r)
                    .with_width(sw)
                    .with_line_cap(canvas::LineCap::Round),
            );

            // Arrow head (triangle)
            let head_path = canvas::Path::new(|b| {
                b.move_to(p2);
                b.line_to(Point::new(
                    p2.x - hl * (ang - spread).cos(),
                    p2.y - hl * (ang - spread).sin(),
                ));
                b.line_to(Point::new(
                    p2.x - hl * (ang + spread).cos(),
                    p2.y - hl * (ang + spread).sin(),
                ));
                b.close();
            });
            frame.fill(&head_path, r);
        }
        Tool::Text => {
            if a.text.is_empty() {
                return;
            }
            let p = sp(a.start);
            let font_size = (a.font_size * scale).max(8.0);
            let line_height = font_size * 1.2;
            for (i, line) in a.text.lines().enumerate() {
                frame.fill_text(canvas::Text {
                    content: line.to_string(),
                    position: Point::new(p.x, p.y + i as f32 * line_height),
                    color: r,
                    size: iced::Pixels(font_size),
                    ..canvas::Text::default()
                });
            }
        }
        Tool::Counter => {
            let p = sp(a.start);
            let radius = (sw * 3.0).max(14.0);
            let path = canvas::Path::new(|b| {
                b.circle(p, radius);
            });
            frame.fill(&path, r);
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(Color::WHITE)
                    .with_width(2.5),
            );

            frame.fill_text(canvas::Text {
                content: a.number.to_string(),
                position: p, // Center of the circle
                color: Color::WHITE,
                size: iced::Pixels(radius * 1.2),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }
        Tool::Ellipse => {
            let p1 = sp(a.start);
            let p2 = sp(a.end);
            let cx = (p1.x + p2.x) / 2.0;
            let cy = (p1.y + p2.y) / 2.0;
            let rx = ((p2.x - p1.x).abs() / 2.0).max(3.0);
            let ry = ((p2.y - p1.y).abs() / 2.0).max(3.0);
            let segments = 32;
            let path = canvas::Path::new(|b| {
                for i in 0..=segments {
                    let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
                    let x = cx + rx * theta.cos();
                    let y = cy + ry * theta.sin();
                    if i == 0 {
                        b.move_to(Point::new(x, y));
                    } else {
                        b.line_to(Point::new(x, y));
                    }
                }
            });
            frame.stroke(
                &path,
                canvas::Stroke::default().with_color(r).with_width(sw),
            );
        }
        Tool::Pen => {
            if a.points.len() < 2 {
                return;
            }
            let path = canvas::Path::new(|b| {
                b.move_to(sp(a.points[0]));
                for p in &a.points[1..] {
                    b.line_to(sp(*p));
                }
            });
            frame.stroke(
                &path,
                canvas::Stroke::default().with_color(r).with_width(sw),
            );
        }
    }
}

// ── LSB steganography ─────────────────────────────────────────────────
//
// 16-bit length header + UTF-8 payload, 1 bit per RGB channel (LSB).
// Changes each channel's value by at most 1/255 — imperceptible.
// Survives PNG round-tripping (grim + wl-copy + image viewers).

// ── ImageMagick annotation compose ────────────────────────────────────

pub fn magick_or_convert() -> Option<&'static str> {
    if std::process::Command::new("magick")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("magick")
    } else if std::process::Command::new("convert")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("convert")
    } else {
        None
    }
}

fn find_annotation_font() -> Option<String> {
    // Try fc-match first (works on NixOS, Arch, Debian, etc.)
    if let Ok(out) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "sans-serif:bold"])
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                return Some(path);
            }
        }
    }
    // Fallback: check common font paths
    for p in [
        "/run/current-system/sw/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/run/current-system/sw/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ] {
        if std::path::Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    None
}

pub fn compose_annotations(
    source: &Path,
    annotations: &[Annotation],
    out: &Path,
) -> Result<(), String> {
    let Some(bin) = magick_or_convert() else {
        return Err("ImageMagick not installed".into());
    };
    let font_path = find_annotation_font();
    let mut args: Vec<String> = vec![source.to_str().unwrap_or("").into()];
    let color_arg = |c: Color| {
        format!(
            "rgba({},{},{},{})",
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            c.a
        )
    };

    for a in annotations {
        match a.tool {
            Tool::Rect => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                let (lx, ty) = (x1.min(x2), y1.min(y2));
                let (rx, by) = (x1.max(x2), y1.max(y2));
                args.extend([
                    "-stroke".into(),
                    color_arg(a.color),
                    "-strokewidth".into(),
                    format!("{}", a.width as i32),
                    "-fill".into(),
                    "none".into(),
                    "-draw".into(),
                    format!("rectangle {},{} {},{}", lx, ty, rx, by),
                ]);
            }
            Tool::Redact => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                let (lx, ty) = (x1.min(x2), y1.min(y2));
                let (rx, by) = (x1.max(x2), y1.max(y2));
                args.extend([
                    "-fill".into(),
                    "black".into(),
                    "-stroke".into(),
                    "none".into(),
                    "-draw".into(),
                    format!("rectangle {},{} {},{}", lx, ty, rx, by),
                ]);
            }
            Tool::Arrow => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                args.extend([
                    "-stroke".into(),
                    color_arg(a.color),
                    "-strokewidth".into(),
                    format!("{}", a.width as i32),
                    "-draw".into(),
                    format!("line {},{} {},{}", x1, y1, x2, y2),
                ]);
                let ang = ((y2 - y1) as f32).atan2((x2 - x1) as f32);
                let hl = (a.width * 4.0).max(12.0);
                let p1x = (x2 as f32 - hl * (ang - 0.5).cos()) as i32;
                let p1y = (y2 as f32 - hl * (ang - 0.5).sin()) as i32;
                let p2x = (x2 as f32 - hl * (ang + 0.5).cos()) as i32;
                let p2y = (y2 as f32 - hl * (ang + 0.5).sin()) as i32;
                args.extend([
                    "-fill".into(),
                    color_arg(a.color),
                    "-stroke".into(),
                    "none".into(),
                    "-draw".into(),
                    format!("polygon {},{} {},{} {},{}", x2, y2, p1x, p1y, p2x, p2y),
                ]);
            }
            Tool::Text => {
                if a.text.is_empty() {
                    continue;
                }
                if let Some(ref fp) = font_path {
                    args.extend(["-font".into(), fp.clone()]);
                }
                args.extend([
                    "-fill".into(),
                    color_arg(a.color),
                    "-stroke".into(),
                    "none".into(),
                    "-pointsize".into(),
                    format!("{}", a.font_size as i32),
                    "-gravity".into(),
                    "NorthWest".into(),
                    "-annotate".into(),
                    format!("+{}+{}", a.start.x as i32, a.start.y as i32),
                    a.text.clone(),
                ]);
            }
            Tool::Counter => {
                let (x, y) = (a.start.x as i32, a.start.y as i32);
                let r = (a.width * 3.0).max(14.0) as i32;
                args.extend([
                    "-fill".into(),
                    color_arg(a.color),
                    "-stroke".into(),
                    "white".into(),
                    "-strokewidth".into(),
                    "2".into(),
                    "-draw".into(),
                    format!("circle {},{} {},{}", x, y, x + r, y),
                ]);
                let font_size = (r as f32 * 1.2).max(12.0);
                let text = a.number.to_string();
                let char_offset_x = (text.len() as f32 * font_size * 0.25) as i32;
                let char_offset_y = (font_size * 0.4) as i32;
                if let Some(ref fp) = font_path {
                    args.extend(["-font".into(), fp.clone()]);
                }
                args.extend([
                    "-fill".into(),
                    "white".into(),
                    "-stroke".into(),
                    "none".into(),
                    "-pointsize".into(),
                    format!("{}", font_size as i32),
                    "-gravity".into(),
                    "NorthWest".into(),
                    "-annotate".into(),
                    format!("+{}+{}", x - char_offset_x, y - char_offset_y),
                    text,
                ]);
            }
            Tool::Ellipse => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                let (lx, ty) = (x1.min(x2), y1.min(y2));
                let (rx, by) = (x1.max(x2), y1.max(y2));
                args.extend([
                    "-stroke".into(),
                    color_arg(a.color),
                    "-strokewidth".into(),
                    format!("{}", a.width as i32),
                    "-fill".into(),
                    "none".into(),
                    "-draw".into(),
                    format!(
                        "ellipse {},{} {},{} 0,360",
                        (lx + rx) / 2,
                        (ty + by) / 2,
                        (rx - lx) / 2,
                        (by - ty) / 2
                    ),
                ]);
            }
            Tool::Pen => {
                if a.points.len() < 2 {
                    continue;
                }
                let pts: Vec<String> = a
                    .points
                    .iter()
                    .map(|p| format!("{},{}", p.x as i32, p.y as i32))
                    .collect();
                args.extend([
                    "-stroke".into(),
                    color_arg(a.color),
                    "-strokewidth".into(),
                    format!("{}", a.width as i32),
                    "-fill".into(),
                    "none".into(),
                    "-draw".into(),
                    format!("polyline {}", pts.join(" ")),
                ]);
            }
        }
    }
    args.push(out.to_str().unwrap_or("").into());
    // Finding 07: bound ImageMagick resource use. Input is a PNG, but
    // a crafted PNG can encode decompression bombs that exhaust memory.
    let ok = std::process::Command::new(bin)
        .args([
            "-limit", "memory", "512MiB",
            "-limit", "map", "1GiB",
            "-limit", "disk", "2GiB",
            "-limit", "width", "16384",
            "-limit", "height", "16384",
        ])
        .args(&args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return Err(format!("{bin} failed"));
    }
    Ok(())
}

pub fn save_annotated_image(
    source: PathBuf,
    annotations: Vec<Annotation>,
    watermark: Option<String>,
) -> Result<Vec<PathBuf>, String> {
    let save_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Screenshots");
    std::fs::create_dir_all(&save_dir).map_err(|e| format!("mkdir: {e}"))?;

    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let time_str = chrono::Local::now().format("%H-%M").to_string();

    let composed_bytes = if annotations.is_empty() {
        std::fs::read(&source).map_err(|e| format!("read source: {e}"))?
    } else {
        let composed_tmp = crate::libraries::process::mbs_subdir("mbs-annotator")
            .join(format!("mbs-composed-{}.png", stamp));
        compose_annotations(&source, &annotations, &composed_tmp)?;
        let bytes = std::fs::read(&composed_tmp).map_err(|e| format!("read composed: {e}"))?;
        let _ = std::fs::remove_file(&composed_tmp);
        bytes
    };

    let mut produced = Vec::new();

    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let names: Vec<String> = watermark
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if !s.contains(&date_str) {
                format!("{} {}", s, date_str)
            } else {
                s.to_string()
            }
        })
        .collect();

    let final_names = if names.is_empty() {
        vec!["Screenshot".to_string()]
    } else {
        names
    };

    for (i, name) in final_names.iter().enumerate() {
        // Stagger multiple saves so cliphist/wl-copy have time to register them
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        let out = save_dir.join(format!("{} {} {}.png", safe_name, date_str, time_str));

        match crate::overlays::plate_watermark::add_plate_watermark(&composed_bytes, name) {
            Ok(wm_bytes) => {
                std::fs::write(&out, &wm_bytes).map_err(|e| format!("write: {e}"))?;
                eprintln!("[save] plate OK, {} bytes -> {:?}", wm_bytes.len(), out);

                let filename = out
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                // Get existing IDs to find the new one
                let mut existing_clip_ids = std::collections::HashSet::new();
                if let Ok(list_out) = std::process::Command::new("cliphist")
                    .args(["list"])
                    .output()
                {
                    for line in String::from_utf8_lossy(&list_out.stdout).lines() {
                        if let Some((id_str, _)) = line.split_once('\t') {
                            if let Ok(id) = id_str.parse::<u64>() {
                                existing_clip_ids.insert(id);
                            }
                        }
                    }
                }

                // Copy the UNIQUE watermarked bytes
                if let Ok(mut child) = std::process::Command::new("wl-copy")
                    .args(["-t", "image/png"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    use std::io::Write;
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(&wm_bytes);
                    }
                    let _ = child.wait();

                    let cache_dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
                    let _ = std::fs::create_dir_all(&cache_dir);
                    let map_path = cache_dir.join("mbs-clip-names.txt");

                    let mut found = false;
                    for _ in 0..20 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if let Ok(list_out) = std::process::Command::new("cliphist")
                            .args(["list"])
                            .output()
                        {
                            let text = String::from_utf8_lossy(&list_out.stdout);
                            for line in text.lines() {
                                if let Some((id_str, _)) = line.split_once('\t') {
                                    if let Ok(id) = id_str.trim().parse::<u64>() {
                                        if !existing_clip_ids.contains(&id) {
                                            let entry = format!("{}\t{}\n", id, filename);
                                            if let Ok(mut f) = std::fs::OpenOptions::new()
                                                .create(true)
                                                .append(true)
                                                .open(&map_path)
                                            {
                                                let _ = f.write_all(entry.as_bytes());
                                                eprintln!(
                                                    "[save] Mapped ID {} to {}",
                                                    id, filename
                                                );
                                            }
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if found {
                            break;
                        }
                    }
                }
                produced.push(out);
            }
            Err(e) => eprintln!("[save] plate FAILED: {e}"),
        }
    }

    Ok(produced)
}

// ── slurp / grim ─────────────────────────────────────────────────────

pub fn slurp_region() -> Option<String> {
    let out = std::process::Command::new("slurp").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn grim_capture(region: Option<&str>) -> Result<PathBuf, String> {
    let dir = crate::libraries::process::mbs_subdir("mbs-annotator");
    let path = dir.join(format!(
        "shot_{}.png",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    ));
    let mut cmd = std::process::Command::new("grim");
    if let Some(r) = region {
        cmd.args(["-g", r]);
    }
    cmd.arg(&path);
    let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        return Err("grim failed".into());
    }
    Ok(path)
}

pub fn capture_fullscreen_to_temp() -> Result<PathBuf, String> {
    grim_capture(None)
}
