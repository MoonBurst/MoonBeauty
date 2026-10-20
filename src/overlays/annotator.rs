//! Annotator overlay.
//! - Watermark: LSB steganography — 1 bit per RGB channel, imperceptible.
//! - Reveal: decodes LSBs in-memory, shows recovered text as a UI overlay.
//!   No image capture, no temp files, no ImageMagick for reveal.
//! - Save: composes annotations via ImageMagick, then LSB-embeds watermark
//!   per recipient, writes `annotated_STAMP_{name}.png`.
//! - Enter: save (annotated+watermarked) then close. Always.

use iced::widget::canvas::{self, Canvas, Frame, Geometry};
use iced::widget::{button, container, text, text_input, Column, Row, Stack};
use iced::{event, mouse, Alignment, Color, Element, Length, Point, Rectangle, Size};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool { Rect, Arrow, Pen, Redact }
impl Tool {
    fn glyph(self) -> &'static str {
        match self { Self::Rect=>"▭", Self::Arrow=>"↗", Self::Pen=>"✎", Self::Redact=>"█" }
    }
    pub fn all() -> [Self; 4] { [Self::Rect, Self::Arrow, Self::Pen, Self::Redact] }
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub tool: Tool,
    pub start: Point,
    pub end: Point,
    pub points: Vec<Point>,
    pub color: Color,
    pub width: f32,
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
}

pub const PALETTE: [Color; 8] = [
    Color::from_rgb(1.0, 0.27, 0.23), Color::from_rgb(1.0, 0.62, 0.04),
    Color::from_rgb(1.0, 0.84, 0.04), Color::from_rgb(0.20, 0.84, 0.29),
    Color::from_rgb(0.04, 0.52, 1.0), Color::from_rgb(0.37, 0.36, 0.90),
    Color::from_rgb(1.0, 1.0, 1.0), Color::from_rgb(0.11, 0.11, 0.12),
];

pub struct AnnotatorOverlay {
    pub handle: Option<iced::widget::image::Handle>,
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

impl AnnotatorOverlay {
    pub fn new() -> Self {
        Self {
            handle: None,
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
                self.status = format!("{}×{} · draw · Enter saves & closes", img.width(), img.height());
            }
            Err(e) => self.status = format!("decode failed: {e}"),
        }
    }

    pub fn update(&mut self, msg: AnnotatorMessage) {
        match &msg {
            AnnotatorMessage::Pressed(p) | AnnotatorMessage::Moved(p) | AnnotatorMessage::Released(p) => {
                eprintln!("[mbs-child] annotator.update: mouse {:?} at ({:.0},{:.0})",
                    if matches!(msg, AnnotatorMessage::Pressed(_)) { "PRESS" }
                    else if matches!(msg, AnnotatorMessage::Moved(_)) { "MOVE" }
                    else { "RELEASE" },
                    p.x, p.y);
            }
            AnnotatorMessage::SetTool(t) => eprintln!("[mbs-child] annotator.update: SetTool {:?}", t),
            _ => {}
        }
        match msg {
            AnnotatorMessage::Pressed(p) if !self.reveal_mode => self.on_press(p),
            AnnotatorMessage::Moved(p) if !self.reveal_mode => self.on_move(p),
            AnnotatorMessage::Released(p) if !self.reveal_mode => self.on_release(p),
            AnnotatorMessage::SetTool(t) => self.tool = t,
            AnnotatorMessage::SetColor(c) => self.color = c,
            AnnotatorMessage::Undo => { self.annotations.pop(); }
            AnnotatorMessage::ClearAll => self.annotations.clear(),
            AnnotatorMessage::WatermarkInputChanged(s) => self.watermark_input = s,
            _ => {}
        }
    }

    fn on_press(&mut self, p: Point) {
        eprintln!("[mbs-child] on_press ({:.0},{:.0}) tool={:?}", p.x, p.y, self.tool);
        eprintln!("[mbs-child] on_press at ({:.0},{:.0}) — starting {:?}", p.x, p.y, self.tool);
        self.drawing = true;
        self.in_progress = Some(Annotation {
            tool: self.tool, start: p, end: p,
            points: if self.tool == Tool::Pen { vec![p] } else { Vec::new() },
            color: self.color, width: self.stroke_width,
        });
    }
    fn on_move(&mut self, p: Point) {
        if !self.drawing { return; }
        if let Some(ref mut a) = self.in_progress {
            if a.tool == Tool::Pen { a.points.push(p); } else { a.end = p; }
        }
    }
    fn on_release(&mut self, p: Point) {
        eprintln!("[mbs-child] on_release ({:.0},{:.0}) in_progress={}", p.x, p.y, self.in_progress.is_some());
        eprintln!("[mbs-child] on_release at ({:.0},{:.0}) — in_progress={}",
            p.x, p.y, self.in_progress.is_some());
        self.drawing = false;
        if let Some(mut a) = self.in_progress.take() {
            if a.tool != Tool::Pen { a.end = p; }
            let ok = match a.tool {
                Tool::Pen => a.points.len() > 2,
                _ => (a.end.x - a.start.x).abs() > 0.5 || (a.end.y - a.start.y).abs() > 0.5,
            };
            if ok {
                eprintln!("[mbs-child] annotation ACCEPTED: {} total", self.annotations.len() + 1);
                self.annotations.push(a);
            } else {
                eprintln!("[mbs-child] annotation REJECTED (too small: dx={:.2} dy={:.2})",
                    (a.end.x - a.start.x).abs(), (a.end.y - a.start.y).abs());
            }
        }
    }

            pub fn view<'a>(&'a self) -> Element<'a, AnnotatorMessage> {
        if self.handle.is_none() {
            return container(
                text(self.status.clone()).size(18).color(Color::WHITE)
            )
            .width(Length::Fill).height(Length::Fill)
            .center_x(Length::Fill).center_y(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.05, 0.02, 0.02))),
                ..Default::default()
            }).into();
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
            annotations: if self.reveal_mode { Vec::new() } else { self.annotations.clone() },
            in_progress: if self.reveal_mode { None } else { self.in_progress.clone() },
            reveal_mode: self.reveal_mode,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        let reveal_banner: Element<'a, AnnotatorMessage> = if self.reveal_mode && self.reveal_text.is_some() {
            let txt = self.reveal_text.clone().unwrap_or_default();
            container(
                text(txt).size(56).color(Color::from_rgb(0.2, 1.0, 0.3)).font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..Default::default()
                })
            )
            .width(Length::Fill)
            .padding([32, 20])
            .center_x(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.88))),
                border: iced::Border { color: Color::WHITE, width: 2.0, radius: 8.0.into() },
                ..Default::default()
            })
            .into()
        } else if self.reveal_mode {
            // Plate case: amplified image is the reveal. No banner.
            container(text("")).width(Length::Fill).height(Length::Fixed(0.0)).into()
        } else {
            container(text("")).width(Length::Fill).height(Length::Fixed(0.0)).into()
        };

        let canvas_stack = container(
            Stack::new()
                .push(image_widget)
                .push(canvas)
                .push(
                    container(reveal_banner)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_y(Alignment::End)
                        .padding(iced::Padding {
                            top: 0.0,
                            right: 0.0,
                            bottom: 40.0,
                            left: 0.0,
                        })
                )
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
                        border: iced::Border { color: Color::WHITE, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    })
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
                            color: if active { Color::WHITE } else { Color::from_rgb(0.3, 0.3, 0.4) },
                            width: if active { 2.5 } else { 1.0 },
                            radius: 12.0.into(),
                        },
                        ..Default::default()
                    })
            );
        }

        let wm_input = text_input("recipient(s) for LSB watermark", &self.watermark_input)
            .on_input(AnnotatorMessage::WatermarkInputChanged)
            .on_submit(AnnotatorMessage::Save)
            .padding([6, 10])
            .size(13)
            .width(Length::Fixed(220.0));

        let reveal_label = if self.reveal_mode { "◀ Exit Reveal" } else { "👁 Reveal" };
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
                border: iced::Border { color: Color::WHITE, width: 1.0, radius: 4.0.into() },
                ..Default::default()
            });

        let mk = |lbl: &'static str, msg: AnnotatorMessage| {
            button(text(lbl).size(13))
                .on_press(msg)
                .padding([6, 12])
                .style(|_t, _s| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(0.15, 0.15, 0.20, 0.9))),
                    text_color: Color::WHITE,
                    border: iced::Border { color: Color::WHITE, width: 1.0, radius: 4.0.into() },
                    ..Default::default()
                })
        };

        let toolbar = container(
            Column::new()
                .push(Row::new()
                    .push(text("Tools:").size(13).color(Color::WHITE))
                    .push(tool_row)
                    .push(text(" ").size(13))
                    .push(color_row)
                    .spacing(10).align_y(Alignment::Center))
                .push(Row::new()
                    .push(text("Watermark:").size(13).color(Color::WHITE))
                    .push(wm_input)
                    .push(reveal_btn)
                    .push(mk("Undo", AnnotatorMessage::Undo))
                    .push(mk("Clear", AnnotatorMessage::ClearAll))
                    .push(mk("Cancel", AnnotatorMessage::Cancel))
                    .spacing(8).align_y(Alignment::Center))
                .spacing(6)
        )
        .width(Length::Fill)
        .padding([10, 12])
        .style(|_t| container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.05, 0.05, 0.08))),
            ..Default::default()
        });

        let status_bar = container(
            text(self.status.clone()).size(12).color(Color::from_rgb(0.7, 0.7, 0.75))
        )
        .width(Length::Fill)
        .padding([6, 14])
        .style(|_t| container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.05, 0.05, 0.08))),
            ..Default::default()
        });

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
}

fn compute_display_rect(bounds: Rectangle, image: Size) -> Option<Rectangle> {
    if image.width <= 0.0 || image.height <= 0.0 { return None; }
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
                fn draw(&self, _s: &(), renderer: &iced::Renderer, _t: &iced::Theme, bounds: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let dr_opt = compute_display_rect(bounds, self.image_size);

        if let Some(dr) = dr_opt {

            if !self.reveal_mode {
                let scale = dr.width / self.image_size.width;
                let to_disp = |p: Point| Point::new(dr.x + p.x * scale, dr.y + p.y * scale);
                for ann in &self.annotations {
                    draw_ann(&mut frame, ann, &to_disp, scale);
                }
                if let Some(a) = &self.in_progress {
                    draw_ann(&mut frame, a, &to_disp, scale);
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn update(&self, _s: &mut (), event: canvas::Event, bounds: Rectangle, cursor: mouse::Cursor)
        -> (event::Status, Option<AnnotatorMessage>)
    {
        if self.reveal_mode { return (event::Status::Ignored, None); }
        let Some(dr) = compute_display_rect(bounds, self.image_size) else {
            return (event::Status::Ignored, None);
        };
        let scale = (dr.width / self.image_size.width).max(0.0001);
        let to_img = |p: Point| Point::new((p.x - dr.x) / scale, (p.y - dr.y) / scale);
        let clamp_img = |p: Point| Point::new(
            p.x.clamp(0.0, self.image_size.width),
            p.y.clamp(0.0, self.image_size.height),
        );
        let inside_img = |p: Point| {
            p.x >= 0.0 && p.x <= self.image_size.width &&
            p.y >= 0.0 && p.y <= self.image_size.height
        };
        if let canvas::Event::Mouse(m) = event {
            match m {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    eprintln!("[mbs-child] CANVAS RAW PRESS pos={:?} bounds={:?}",
                        cursor.position(), bounds);
                    if let Some(p) = cursor.position_in(bounds) {
                        let img = to_img(p);
                        if inside_img(img) {
                            return (event::Status::Captured, Some(AnnotatorMessage::Pressed(img)));
                        }
                        eprintln!("[mbs-child] press OUTSIDE image: img=({:.0},{:.0}) dr=({:.0},{:.0} {:.0}x{:.0})",
                            img.x, img.y, dr.x, dr.y, dr.width, dr.height);
                        return (event::Status::Ignored, None);
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if let Some(p) = cursor.position_in(bounds) {
                        {
                            use std::sync::atomic::{AtomicU32, Ordering};
                            static M: AtomicU32 = AtomicU32::new(0);
                            let n = M.fetch_add(1, Ordering::Relaxed);
                            if n < 5000 {
                                eprintln!("[mbs-child] CANVAS MOVE #{} raw=({:.0},{:.0})", n, p.x, p.y);
                            }
                        }
                        return (event::Status::Ignored, Some(AnnotatorMessage::Moved(clamp_img(to_img(p)))));
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if let Some(p) = cursor.position_in(bounds) {
                        return (event::Status::Captured, Some(AnnotatorMessage::Released(clamp_img(to_img(p)))));
                    }
                }
                _ => {}
            }
        }
        (event::Status::Ignored, None)
    }
}

fn draw_ann<F: Fn(Point) -> Point>(frame: &mut Frame, a: &Annotation, to_disp: &F, scale: f32) {
    let sp = to_disp;
    let sw = (a.width * scale).max(3.0);
    let r = a.color;
    match a.tool {
        Tool::Rect => {
            let p1 = sp(a.start); let p2 = sp(a.end);
            let x = p1.x.min(p2.x); let y = p1.y.min(p2.y);
            let w = (p2.x - p1.x).abs().max(6.0);
            let h = (p2.y - p1.y).abs().max(6.0);
            eprintln!("[mbs-child] RECT pixel=({:.0},{:.0}) {:.0}x{:.0} rgba=({:.2},{:.2},{:.2},{:.2})",
                x, y, w, h, r.r, r.g, r.b, r.a);
            // Solid fill
            frame.fill_rectangle(Point::new(x, y), Size::new(w, h),
                Color::from_rgba(r.r, r.g, r.b, 0.55));
            // Border
            frame.fill_rectangle(Point::new(x, y), Size::new(w, sw), r);
            frame.fill_rectangle(Point::new(x, y + h - sw), Size::new(w, sw), r);
            frame.fill_rectangle(Point::new(x, y), Size::new(sw, h), r);
            frame.fill_rectangle(Point::new(x + w - sw, y), Size::new(sw, h), r);
        }
        Tool::Redact => {
            let p1 = sp(a.start); let p2 = sp(a.end);
            let x = p1.x.min(p2.x); let y = p1.y.min(p2.y);
            let w = (p2.x - p1.x).abs().max(4.0);
            let h = (p2.y - p1.y).abs().max(4.0);
            frame.fill_rectangle(Point::new(x, y), Size::new(w, h), Color::BLACK);
        }
        Tool::Arrow => {
            let p1 = sp(a.start); let p2 = sp(a.end);
            let dx = p2.x - p1.x; let dy = p2.y - p1.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1.0 { return; }
            let steps = (len / 2.0).ceil() as i32;
            for step in 0..=steps {
                let f = step as f32 / steps as f32;
                frame.fill_rectangle(
                    Point::new(p1.x + dx * f - sw / 2.0, p1.y + dy * f - sw / 2.0),
                    Size::new(sw, sw),
                    r,
                );
            }
            let hs = sw * 3.0;
            frame.fill_rectangle(Point::new(p2.x - hs / 2.0, p2.y - hs / 2.0),
                Size::new(hs, hs), r);
        }
        Tool::Pen => {
            if a.points.len() < 2 { return; }
            for p in &a.points {
                let dp = sp(*p);
                frame.fill_rectangle(
                    Point::new(dp.x - sw / 2.0, dp.y - sw / 2.0),
                    Size::new(sw, sw),
                    r,
                );
            }
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
    if std::process::Command::new("magick").arg("--version").output()
        .map(|o| o.status.success()).unwrap_or(false) { Some("magick") }
    else if std::process::Command::new("convert").arg("--version").output()
        .map(|o| o.status.success()).unwrap_or(false) { Some("convert") }
    else { None }
}

pub fn compose_annotations(source: &Path, annotations: &[Annotation], out: &Path) -> Result<(), String> {
    let Some(bin) = magick_or_convert() else { return Err("ImageMagick not installed".into()); };
    let mut args: Vec<String> = vec![source.to_str().unwrap_or("").into()];
    let color_arg = |c: Color| format!("rgba({},{},{},{})",
        (c.r*255.0) as u8, (c.g*255.0) as u8, (c.b*255.0) as u8, c.a);

    for a in annotations {
        match a.tool {
            Tool::Rect => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                let (lx, ty) = (x1.min(x2), y1.min(y2));
                let (rx, by) = (x1.max(x2), y1.max(y2));
                args.extend([
                    "-stroke".into(), color_arg(a.color),
                    "-strokewidth".into(), format!("{}", a.width as i32),
                    "-fill".into(), "none".into(),
                    "-draw".into(), format!("rectangle {},{} {},{}", lx, ty, rx, by),
                ]);
            }
            Tool::Redact => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                let (lx, ty) = (x1.min(x2), y1.min(y2));
                let (rx, by) = (x1.max(x2), y1.max(y2));
                args.extend(["-fill".into(), "black".into(), "-stroke".into(), "none".into(),
                    "-draw".into(), format!("rectangle {},{} {},{}", lx, ty, rx, by)]);
            }
            Tool::Arrow => {
                let (x1, y1) = (a.start.x as i32, a.start.y as i32);
                let (x2, y2) = (a.end.x as i32, a.end.y as i32);
                args.extend([
                    "-stroke".into(), color_arg(a.color),
                    "-strokewidth".into(), format!("{}", a.width as i32),
                    "-draw".into(), format!("line {},{} {},{}", x1, y1, x2, y2),
                ]);
                let ang = ((y2-y1) as f32).atan2((x2-x1) as f32);
                let hl = (a.width * 4.0).max(12.0);
                let p1x = (x2 as f32 - hl * (ang-0.5).cos()) as i32;
                let p1y = (y2 as f32 - hl * (ang-0.5).sin()) as i32;
                let p2x = (x2 as f32 - hl * (ang+0.5).cos()) as i32;
                let p2y = (y2 as f32 - hl * (ang+0.5).sin()) as i32;
                args.extend(["-fill".into(), color_arg(a.color), "-stroke".into(), "none".into(),
                    "-draw".into(), format!("polygon {},{} {},{} {},{}", x2, y2, p1x, p1y, p2x, p2y)]);
            }
            Tool::Pen => {
                if a.points.len() < 2 { continue; }
                let pts: Vec<String> = a.points.iter()
                    .map(|p| format!("{},{}", p.x as i32, p.y as i32)).collect();
                args.extend([
                    "-stroke".into(), color_arg(a.color),
                    "-strokewidth".into(), format!("{}", a.width as i32),
                    "-fill".into(), "none".into(),
                    "-draw".into(), format!("polyline {}", pts.join(" ")),
                ]);
            }
        }
    }
    args.push(out.to_str().unwrap_or("").into());
    let ok = std::process::Command::new(bin).args(&args).status()
        .map(|s| s.success()).unwrap_or(false);
    if !ok { return Err(format!("{bin} failed")); }
    Ok(())
}

pub fn save_annotated_image(
    source: PathBuf, annotations: Vec<Annotation>, watermark: Option<String>,
) -> Result<Vec<PathBuf>, String> {
    let save_dir = dirs::picture_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Screenshots");
    std::fs::create_dir_all(&save_dir).map_err(|e| format!("mkdir: {e}"))?;
    
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let time_str = chrono::Local::now().format("%H-%M").to_string();

    let composed_bytes = if annotations.is_empty() {
        std::fs::read(&source).map_err(|e| format!("read source: {e}"))?
    } else {
        let composed_tmp = std::env::temp_dir().join(format!("mbs-composed-{}.png", stamp));
        compose_annotations(&source, &annotations, &composed_tmp)?;
        let bytes = std::fs::read(&composed_tmp).map_err(|e| format!("read composed: {e}"))?;
        let _ = std::fs::remove_file(&composed_tmp);
        bytes
    };

    let mut produced = Vec::new();
    
    let names: Vec<String> = watermark
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let final_names = if names.is_empty() {
        vec!["Screenshot".to_string()]
    } else {
        names
    };

    for (i, name) in final_names.iter().enumerate() {
        // Stagger multiple saves so cliphist/wl-copy have time to register them
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        let safe_name: String = name.chars()
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        
        let out = save_dir.join(format!("{} {} {}.png", safe_name, date_str, time_str));

        match crate::overlays::plate_watermark::add_plate_watermark(&composed_bytes, name) {
            Ok(wm_bytes) => {
                std::fs::write(&out, &wm_bytes).map_err(|e| format!("write: {e}"))?;
                eprintln!("[save] plate OK, {} bytes -> {:?}", wm_bytes.len(), out);
                
                let filename = out.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                // Get existing IDs to find the new one
                let mut existing_clip_ids = std::collections::HashSet::new();
                if let Ok(list_out) = std::process::Command::new("cliphist").args(["list"]).output() {
                    for line in String::from_utf8_lossy(&list_out.stdout).lines() {
                        if let Some((id_str, _)) = line.split_once('\t') {
                            if let Ok(id) = id_str.parse::<u64>() { existing_clip_ids.insert(id); }
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

                    let cache_dir = dirs::cache_dir().unwrap_or_else(|| std::env::temp_dir());
                    let _ = std::fs::create_dir_all(&cache_dir);
                    let map_path = cache_dir.join("mbs-clip-names.txt");

                    let mut found = false;
                    for _ in 0..20 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if let Ok(list_out) = std::process::Command::new("cliphist").args(["list"]).output() {
                            let text = String::from_utf8_lossy(&list_out.stdout);
                            for line in text.lines() {
                                if let Some((id_str, _)) = line.split_once('\t') {
                                    if let Ok(id) = id_str.trim().parse::<u64>() {
                                        if !existing_clip_ids.contains(&id) {
                                            let entry = format!("{}\t{}\n", id, filename);
                                            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&map_path) {
                                                let _ = f.write_all(entry.as_bytes());
                                                eprintln!("[save] Mapped ID {} to {}", id, filename);
                                            }
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if found { break; }
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
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

pub fn grim_capture(region: Option<&str>) -> Result<PathBuf, String> {
    let rt = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let dir = PathBuf::from(rt).join("mbs-annotator");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    let path = dir.join(format!("shot_{}.png",
        chrono::Local::now().format("%Y%m%d_%H%M%S")));
    let mut cmd = std::process::Command::new("grim");
    if let Some(r) = region { cmd.args(["-g", r]); }
    cmd.arg(&path);
    let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
    if !ok { return Err("grim failed".into()); }
    Ok(path)
}

pub fn capture_fullscreen_to_temp() -> Result<PathBuf, String> {
    grim_capture(None)
}










