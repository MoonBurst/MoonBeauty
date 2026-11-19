use iced::mouse::Cursor;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};

#[derive(Debug, Clone, Copy)]
pub struct AngledHexGeometry {
    pub dx: f32,
    pub dy: f32,
}

impl AngledHexGeometry {
    pub fn new(win_w: f32, win_h: f32, cut: f32, angle_deg: f32) -> Self {
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        Self {
            dx: (cut * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0),
            dy: (cut * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0),
        }
    }

    pub fn inset_at(self, y: f32, pad: f32) -> f32 {
        if self.dy > y {
            (self.dx * (1.0 - (y / self.dy))).max(0.0) + pad
        } else {
            pad
        }
    }

    pub fn standard_insets(self) -> (f32, f32, f32) {
        let header = self.inset_at(20.0, 16.0);
        let footer = self.inset_at(20.0, 16.0);
        let body = header.min(footer) + 4.0;
        (header, body, footer)
    }

    pub fn search_list_insets(self) -> (f32, f32, f32, f32) {
        let search = self.inset_at(18.0, 16.0);
        let list_top = self.inset_at(72.0, 16.0);
        let list_bottom = self.inset_at(45.0, 16.0);
        let list_side = list_top.max(list_bottom).max(20.0);
        (search, list_top, list_bottom, list_side)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeType {
    Rounded(u16),
    Flat,
    Slant { left: SlantSide, right: SlantSide },
    Hexagon { cut: u16 },
    AngledHexagon { cut: f32, angle_deg: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlantSide {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct ShapeBox {
    pub shape: ShapeType,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub slant_width: f32,
    pub width: Length,
    pub height: Length,
}

impl Default for ShapeBox {
    fn default() -> Self {
        Self {
            shape: ShapeType::Slant {
                left: SlantSide::Left,
                right: SlantSide::Right,
            },
            background: Color::from_rgb8(15, 15, 15),
            border_color: Color::from_rgb8(0, 51, 153),
            border_width: 2.0,
            slant_width: 12.0,
            width: Length::Fill,
            height: Length::Fill,
        }
    }
}

impl ShapeBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shape(mut self, shape: ShapeType) -> Self {
        self.shape = shape;
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn border(mut self, color: Color, width: f32) -> Self {
        self.border_color = color;
        self.border_width = width;
        self
    }

    pub fn slant_width(mut self, width: f32) -> Self {
        self.slant_width = width;
        self
    }

    pub fn size(mut self, width: Length, height: Length) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn view<Message: 'static>(self) -> Element<'static, Message> {
        let w = self.width;
        let h = self.height;
        Canvas::new(self).width(w).height(h).into()
    }
}

impl<Message> canvas::Program<Message> for ShapeBox {
    // Tracks the last-rendered bounds and shape params. We only request a
    // redraw when one of them changes — NOT on every event. The old code
    // requested redraw unconditionally, which combined with the 50 ms IPC
    // poll (20 events/sec) to force a full-bar repaint at 60 fps idle.
    type State = Option<(Rectangle, ShapeType, Color, Color, f32, f32)>;

    fn update(
        &self,
        state: &mut Self::State,
        _event: &iced::Event,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        let key = (
            bounds,
            self.shape,
            self.background,
            self.border_color,
            self.border_width,
            self.slant_width,
        );
        if *state != Some(key) {
            *state = Some(key);
            return Some(iced::widget::Action::request_redraw());
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let half = self.border_width / 2.0;

        if w <= half * 2.0 || h <= half * 2.0 {
            return vec![frame.into_geometry()];
        }

        let path = match self.shape {
            ShapeType::Flat => Path::rectangle(
                Point::new(half, half),
                Size::new(w - self.border_width, h - self.border_width),
            ),
            ShapeType::Rounded(r) => {
                let radius = (r as f32).min(w / 2.0).min(h / 2.0);
                Path::rounded_rectangle(
                    Point::new(half, half),
                    Size::new(w - self.border_width, h - self.border_width),
                    radius.into(),
                )
            }
            ShapeType::Slant { left, right } => {
                let sw = self.slant_width.min(w / 2.0);
                let x1 = if left == SlantSide::Right {
                    sw + half
                } else {
                    half
                };
                let x2 = if left == SlantSide::Left {
                    sw + half
                } else {
                    half
                };
                let x3 = if right == SlantSide::Left {
                    w - sw - half
                } else {
                    w - half
                };
                let x4 = if right == SlantSide::Right {
                    w - sw - half
                } else {
                    w - half
                };

                Path::new(|builder| {
                    builder.move_to(Point::new(x1, half));
                    builder.line_to(Point::new(x3, half));
                    builder.line_to(Point::new(x4, h - half));
                    builder.line_to(Point::new(x2, h - half));
                    builder.close();
                })
            }
            ShapeType::Hexagon { cut } => {
                let c = (cut as f32).min(w * 0.35).min(h * 0.45);
                Path::new(|builder| {
                    builder.move_to(Point::new(c + half, half));
                    builder.line_to(Point::new(w - c - half, half));
                    builder.line_to(Point::new(w - half, c + half));
                    builder.line_to(Point::new(w - half, h - c - half));
                    builder.line_to(Point::new(w - c - half, h - half));
                    builder.line_to(Point::new(c + half, h - half));
                    builder.line_to(Point::new(half, h - c - half));
                    builder.line_to(Point::new(half, c + half));
                    builder.close();
                })
            }
            ShapeType::AngledHexagon { cut, angle_deg } => {
                let AngledHexGeometry { dx, dy } = AngledHexGeometry::new(w, h, cut, angle_deg);

                Path::new(|builder| {
                    builder.move_to(Point::new(dx + half, half));
                    builder.line_to(Point::new(w - dx - half, half));
                    builder.line_to(Point::new(w - half, dy + half));
                    builder.line_to(Point::new(w - half, h - dy - half));
                    builder.line_to(Point::new(w - dx - half, h - half));
                    builder.line_to(Point::new(dx + half, h - half));
                    builder.line_to(Point::new(half, h - dy - half));
                    builder.line_to(Point::new(half, dy + half));
                    builder.close();
                })
            }
        };

        if self.background.a > 0.0 {
            frame.fill(&path, self.background);
        }

        if self.border_width > 0.0 && self.border_color.a > 0.0 {
            frame.stroke(
                &path,
                Stroke::default()
                    .with_color(self.border_color)
                    .with_width(self.border_width),
            );
        }

        vec![frame.into_geometry()]
    }
}
