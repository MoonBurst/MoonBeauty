//! Per-monitor freeze-frame selector.
//! Each fragment displays a local crop of the full desktop image.
//! Fragments coordinate cross-monitor drags via a shared state file.

use iced::widget::canvas::{Canvas, Frame, Geometry, Program};
use iced::widget::Stack;
use iced::{Color, Element, Length, Point, Rectangle, Size, Task, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::to_layer_message;
use std::io::Write;
use std::path::PathBuf;

#[to_layer_message]
#[derive(Clone)]
pub enum Message {
    Tick,
    Press(Point),
    Drag(Point),
    Release(Point),
    Escape,
}

// Manual Debug impl because the macro injects variants that don't derive Debug
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Message")
    }
}

pub fn read_state_file(path: &std::path::Path) -> (String, i32, i32, i32, i32) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return ("idle".into(), 0, 0, 0, 0);
    };
    let mut it = content.lines();
    let state = it.next().unwrap_or("idle").trim().to_string();
    let sx = it.next().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
    let sy = it.next().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
    let cx = it.next().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
    let cy = it.next().and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
    (state, sx, sy, cx, cy)
}

pub fn write_state_file(path: &std::path::Path, state: &str, sx: i32, sy: i32, cx: i32, cy: i32) {
    let tmp = path.with_extension("tmp");
    if let Ok(mut f) = std::fs::File::create(&tmp) {
        let _ = write!(f, "{}\n{}\n{}\n{}\n{}\n", state, sx, sy, cx, cy);
        let _ = f.sync_all();
    }
    let _ = std::fs::rename(&tmp, path);
}

pub struct FragmentApp {
    monitor: String,
    gx: i32,
    gy: i32,
    handle: iced::widget::image::Handle,
    state_file: PathBuf,
    is_initiator: bool,
    render_rect: Option<Rectangle>,
}

impl Default for FragmentApp {
    fn default() -> Self {
        Self {
            monitor: String::new(),
            gx: 0,
            gy: 0,
            handle: iced::widget::image::Handle::from_bytes(Vec::<u8>::new()),
            state_file: PathBuf::new(),
            is_initiator: false,
            render_rect: None,
        }
    }
}

#[cfg(any())] impl Application for FragmentApp {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = FragmentApp;

    fn new(flags: Self::Flags) -> (Self, Task<Message>) {
        (flags, Task::none())
    }

    fn namespace(&self) -> String {
        format!("mbs-freeze-frag-{}", self.monitor)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                let (state, sx, sy, cx, cy) = read_state_file(&self.state_file);
                match state.as_str() {
                    "done" => std::process::exit(0),
                    "cancelled" => std::process::exit(130),
                    "dragging" => {
                        let lx0 = (sx - self.gx) as f32;
                        let ly0 = (sy - self.gy) as f32;
                        let lx1 = (cx - self.gx) as f32;
                        let ly1 = (cy - self.gy) as f32;
                        self.render_rect = Some(Rectangle {
                            x: lx0.min(lx1),
                            y: ly0.min(ly1),
                            width: (lx1 - lx0).abs(),
                            height: (ly1 - ly0).abs(),
                        });
                    }
                    _ => {
                        self.render_rect = None;
                    }
                }
                Task::none()
            }
            Message::Press(local) => {
                let (state, _, _, _, _) = read_state_file(&self.state_file);
                if state == "idle" {
                    self.is_initiator = true;
                    let gx = self.gx + local.x as i32;
                    let gy = self.gy + local.y as i32;
                    write_state_file(&self.state_file, "dragging", gx, gy, gx, gy);
                }
                Task::none()
            }
            Message::Drag(local) => {
                if !self.is_initiator {
                    let (state, _, _, _, _) = read_state_file(&self.state_file);
                    if state == "dragging" {
                        // We picked up the pointer mid-drag from another monitor.
                        self.is_initiator = true;
                    } else {
                        return Task::none();
                    }
                }
                let (state, sx, sy, _, _) = read_state_file(&self.state_file);
                if state == "dragging" {
                    let gx = self.gx + local.x as i32;
                    let gy = self.gy + local.y as i32;
                    write_state_file(&self.state_file, "dragging", sx, sy, gx, gy);
                }
                Task::none()
            }
            Message::Release(local) => {
                let (state, sx, sy, _, _) = read_state_file(&self.state_file);
                if state == "dragging" {
                    let gx = self.gx + local.x as i32;
                    let gy = self.gy + local.y as i32;
                    write_state_file(&self.state_file, "done", sx, sy, gx, gy);
                    std::process::exit(0);
                }
                Task::none()
            }
            Message::Escape => {
                let (state, _, _, _, _) = read_state_file(&self.state_file);
                if state != "done" {
                    write_state_file(&self.state_file, "cancelled", 0, 0, 0, 0);
                }
                std::process::exit(130);
            }
            _ => Task::none(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let img = iced::widget::image(self.handle.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(iced::ContentFit::Fill);

        let canvas = Canvas::new(FragmentCanvas {
            rect: self.render_rect,
        })
        .width(Length::Fill)
        .height(Length::Fill);

        Stack::new().push(img).push(canvas).into()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick),
            iced::keyboard::on_key_press(|key, modifiers| match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                    Some(Message::Escape)
                }
                _ => None,
            }),
        ])
    }
}

struct FragmentCanvas {
    rect: Option<Rectangle>,
}

impl Program<Message> for FragmentCanvas {
    type State = ();

    fn update(
        &self,
        _s: &mut (),
        event: &iced::Event,
        _bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        use iced::mouse;
        use iced::event::Status;

        // Use unclamped cursor position so we get motion events even when the
        // cursor leaves this surface (which happens during a Wayland implicit
        // grab when dragging from one monitor to another).
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            )) => {
                if let Some(p) = cursor.position() {
                    return Some(iced::widget::Action::publish(Message::Press(p)));
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(p) = cursor.position() {
                    return Some(iced::widget::Action::publish(Message::Drag(p)));
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left,
            )) => {
                if let Some(p) = cursor.position() {
                    return Some(iced::widget::Action::publish(Message::Release(p)));
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        _s: &(),
        renderer: &iced::Renderer,
        _t: &Theme,
        bounds: Rectangle,
        _c: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let dim = Color::from_rgba(0.0, 0.0, 0.0, 0.55);

        match self.rect {
            Some(r) if r.width > 1.0 && r.height > 1.0 => {
                let sx = r.x.max(0.0);
                let sy = r.y.max(0.0);
                let ex = (r.x + r.width).min(bounds.width);
                let ey = (r.y + r.height).min(bounds.height);

                if sy > 0.0 {
                    frame.fill_rectangle(Point::ORIGIN, Size::new(bounds.width, sy), dim);
                }
                if ey < bounds.height {
                    frame.fill_rectangle(
                        Point::new(0.0, ey),
                        Size::new(bounds.width, bounds.height - ey),
                        dim,
                    );
                }
                if sy < ey {
                    if sx > 0.0 {
                        frame.fill_rectangle(Point::new(0.0, sy), Size::new(sx, ey - sy), dim);
                    }
                    if ex < bounds.width {
                        frame.fill_rectangle(
                            Point::new(ex, sy),
                            Size::new(bounds.width - ex, ey - sy),
                            dim,
                        );
                    }
                }

                let b = 1.5;
                let border = Color::WHITE;
                let w = (ex - sx).max(0.0);
                let h = (ey - sy).max(0.0);
                if w > 0.0 && h > 0.0 {
                    frame.fill_rectangle(Point::new(sx, sy), Size::new(w, b), border);
                    frame.fill_rectangle(Point::new(sx, ey - b), Size::new(w, b), border);
                    frame.fill_rectangle(Point::new(sx, sy), Size::new(b, h), border);
                    frame.fill_rectangle(Point::new(ex - b, sy), Size::new(b, h), border);
                    
                    // Live Dimensions HUD
                    let dim_text = format!("{} x {}", w as u32, h as u32);
                    frame.fill_text(iced::widget::canvas::Text {
                        content: dim_text,
                        position: Point::new(ex + 8.0, ey + 8.0),
                        color: Color::WHITE,
                        size: iced::Pixels(16.0),
                        ..Default::default()
                    });
                }
            }
            _ => {
                frame.fill_rectangle(Point::ORIGIN, bounds.size(), dim);
            }
        }

        vec![frame.into_geometry()]
    }
}

pub fn run(
    monitor: String,
    gx: i32,
    gy: i32,
    local_png: PathBuf,
    state_file: PathBuf,
) -> Result<(), String> {
    eprintln!("[fragment DISABLED during iced 0.14 migration]");
    Err("DISABLED during iced 0.14 migration".into())
}
