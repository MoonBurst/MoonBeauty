use iced::widget::canvas::{Canvas, Frame, Geometry, Program};
use iced::widget::Stack;
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;
use iced_layershell::to_layer_message;
use std::path::PathBuf;

#[to_layer_message]
#[derive(Clone)]
pub enum Message {
    Press(Point),
    Drag(Point),
    Release,
    Confirm,
    Cancel,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Message")
    }
}


pub struct SelectorApp {
    handle: iced::widget::image::Handle,
    img_w: u32,
    img_h: u32,
    rgba: Vec<u8>,
    start: Option<Point>,
    current: Option<Point>,
    out_path: PathBuf,
}

impl Default for SelectorApp {
    fn default() -> Self {
        Self {
            handle: iced::widget::image::Handle::from_bytes(Vec::<u8>::new()),
            img_w: 1,
            img_h: 1,
            rgba: vec![0, 0, 0, 0],
            start: None,
            current: None,
            out_path: PathBuf::new(),
        }
    }
}


impl SelectorApp {
    fn selection(&self) -> Option<(f32, f32, f32, f32)> {
        let (s, c) = (self.start?, self.current?);
        let x0 = s.x.min(c.x);
        let y0 = s.y.min(c.y);
        let x1 = s.x.max(c.x);
        let y1 = s.y.max(c.y);
        if x1 - x0 < 4.0 || y1 - y0 < 4.0 {
            return None;
        }
        Some((x0, y0, x1 - x0, y1 - y0))
    }
}

struct SelectorCanvas {
    start: Option<Point>,
    current: Option<Point>,
}

impl Program<Message> for SelectorCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: iced::widget::canvas::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> (iced::widget::canvas::event::Status, Option<Message>) {
        use iced::mouse;
        use iced::widget::canvas::event::Status;
        match event {
            iced::widget::canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_in(bounds) {
                    return (Status::Captured, Some(Message::Press(p)));
                }
            }
            iced::widget::canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if self.start.is_some() {
                    if let Some(p) = cursor.position_in(bounds) {
                        return (Status::Captured, Some(Message::Drag(p)));
                    }
                }
            }
            iced::widget::canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if self.start.is_some() => {
                    return (Status::Captured, Some(Message::Release));
                }
            _ => {}
        }
        (Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let sel = if let (Some(s), Some(c)) = (self.start, self.current) {
            let x0 = s.x.min(c.x);
            let y0 = s.y.min(c.y);
            let x1 = s.x.max(c.x);
            let y1 = s.y.max(c.y);
            Some((x0, y0, x1 - x0, y1 - y0))
        } else {
            None
        };

        let dim = Color::from_rgba(0.0, 0.0, 0.0, 0.55);

        match sel {
            Some((sx, sy, sw, sh)) if sw > 1.0 && sh > 1.0 => {
                frame.fill_rectangle(Point::ORIGIN, Size::new(bounds.width, sy), dim);
                frame.fill_rectangle(
                    Point::new(0.0, sy + sh),
                    Size::new(bounds.width, (bounds.height - sy - sh).max(0.0)),
                    dim,
                );
                frame.fill_rectangle(Point::new(0.0, sy), Size::new(sx, sh), dim);
                frame.fill_rectangle(
                    Point::new(sx + sw, sy),
                    Size::new((bounds.width - sx - sw).max(0.0), sh),
                    dim,
                );
                let border = Color::WHITE;
                let b = 1.5;
                frame.fill_rectangle(Point::new(sx, sy), Size::new(sw, b), border);
                frame.fill_rectangle(Point::new(sx, sy + sh - b), Size::new(sw, b), border);
                frame.fill_rectangle(Point::new(sx, sy), Size::new(b, sh), border);
                frame.fill_rectangle(Point::new(sx + sw - b, sy), Size::new(b, sh), border);
            }
            _ => {
                frame.fill_rectangle(Point::ORIGIN, bounds.size(), dim);
            }
        }

        vec![frame.into_geometry()]
    }
}

impl Application for SelectorApp {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = SelectorApp;

    fn new(flags: Self::Flags) -> (Self, iced::Task<Message>) {
        (flags, iced::Task::none())
    }

    fn namespace(&self) -> String {
        "mbs-freeze-selector".to_string()
    }

    fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Press(p) => {
                self.start = Some(p);
                self.current = Some(p);
            }
            Message::Drag(p) => {
                if self.start.is_some() {
                    self.current = Some(p);
                }
            }
            Message::Release => {
                // Auto-confirm on mouse release (matches slurp UX). If the
                // selection is too small, stay open so the user can drag again.
                if self.selection().is_some() {
                    return self.update(Message::Confirm);
                }
            }
            Message::Confirm => {
                if let Some((x, y, w, h)) = self.selection() {
                    let src = match image::RgbaImage::from_raw(self.img_w, self.img_h, self.rgba.clone()) {
                        Some(i) => i,
                        None => {
                            eprintln!("[selector] invalid rgba buffer");
                            std::process::exit(1);
                        }
                    };
                    let xi = (x as u32).min(self.img_w.saturating_sub(1));
                    let yi = (y as u32).min(self.img_h.saturating_sub(1));
                    let wi = (w as u32).min(self.img_w - xi).max(1);
                    let hi = (h as u32).min(self.img_h - yi).max(1);
                    let cropped = image::imageops::crop_imm(&src, xi, yi, wi, hi).to_image();
                    if let Err(e) = cropped.save(&self.out_path) {
                        eprintln!("[selector] save failed: {e}");
                        std::process::exit(1);
                    }
                    std::process::exit(0);
                } else {
                    eprintln!("[selector] no selection");
                }
            }
            Message::Cancel => {
                std::process::exit(130);
            }
            _ => {}  // to_layer_message injected variants — ignore
        }
        iced::Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let img = iced::widget::image(self.handle.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(iced::ContentFit::Fill);

        let canvas = Canvas::new(SelectorCanvas {
            start: self.start,
            current: self.current,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        Stack::new().push(img).push(canvas).into()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::keyboard::on_key_press(|key, _mods| match key {
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => Some(Message::Confirm),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => Some(Message::Cancel),
            _ => None,
        })
    }


}
pub fn run(input_png: PathBuf, output_png: PathBuf, binded_output: Option<String>) -> Result<(), String> {
    let bytes = std::fs::read(&input_png).map_err(|e| format!("read input: {e}"))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("decode: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let rgba_raw = rgba.into_raw();
    let handle = iced::widget::image::Handle::from_bytes(bytes);

    let app = SelectorApp {
        handle,
        img_w: w,
        img_h: h,
        rgba: rgba_raw,
        start: None,
        current: None,
        out_path: output_png,
    };

    let layer = LayerShellSettings {
        size: Some((0, 0)),
        exclusive_zone: -1,
        anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
        layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::Exclusive,
        binded_output_name: binded_output,
        ..Default::default()
    };

    let settings = Settings {
        flags: app,
        layer_settings: layer,
        ..Default::default()
    };

    <SelectorApp as Application>::run(settings).map_err(|e| format!("run: {e:?}"))
}
