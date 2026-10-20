use iced::mouse::Cursor;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeType {
    Rounded(u16),
    Flat,
    Slant { left: SlantSide, right: SlantSide },
    Hexagon { cut: u16 },
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
        Canvas::new(self)
            .width(w)
            .height(h)
            .into()
    }
}

impl<Message> canvas::Program<Message> for ShapeBox {
    type State = ();

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
                let x1 = if left == SlantSide::Right { sw + half } else { half };
                let x2 = if left == SlantSide::Left { sw + half } else { half };
                let x3 = if right == SlantSide::Left { w - sw - half } else { w - half };
                let x4 = if right == SlantSide::Right { w - sw - half } else { w - half };

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
