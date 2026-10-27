//! Drag-to-resize canvas widget for tooltip dimensions in Bar Settings.
//!
//! Replaces the W/H sliders in the per-module row. Click anywhere in the
//! box to jump the size to that fraction of the allowed range, drag to
//! scrub, release to commit. Range maps linearly:
//!   x_norm ∈ [0,1] → w = min_w + x_norm·(max_w − min_w)

use super::settings::SettingsMessage;
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme, mouse};

pub struct SizeResizer {
    pub id: String,
    pub w: u32,
    pub h: u32,
    pub min_w: u32,
    pub min_h: u32,
    pub max_w: u32,
    pub max_h: u32,
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub muted: Color,
    pub display_w: f32,
    pub display_h: f32,
}

/// Public because `Program::State` for a public canvas impl leaks it
/// into the type signature. Also derives Debug/Clone/PartialEq/Copy per
/// the trait's documentation hints (needed for state comparisons in some
/// iced versions; harmless here).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DragState {
    pub dragging: bool,
}

impl SizeResizer {
    pub fn view(self) -> Element<'static, SettingsMessage> {
        let w = self.display_w;
        let h = self.display_h;
        Canvas::new(self).width(Length::Fixed(w)).height(Length::Fixed(h)).into()
    }
}

impl canvas::Program<SettingsMessage> for SizeResizer {
    type State = DragState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<iced::widget::Action<SettingsMessage>> {
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_in(bounds) {
                    state.dragging = true;
                    let x_norm = (p.x / bounds.width).clamp(0.0, 1.0);
                    let y_norm = (p.y / bounds.height).clamp(0.0, 1.0);
                    return Some(iced::widget::Action::publish(SettingsMessage::SizeDrag {
                        id: self.id.clone(),
                        x_norm,
                        y_norm,
                    }));
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                // Use absolute cursor position (not position_in) so drags
                // that go outside the widget still update clamped to bounds.
                if let Some(p) = cursor.position() {
                    let local_x = p.x - bounds.x;
                    let local_y = p.y - bounds.y;
                    let x_norm = (local_x / bounds.width).clamp(0.0, 1.0);
                    let y_norm = (local_y / bounds.height).clamp(0.0, 1.0);
                    return Some(iced::widget::Action::publish(SettingsMessage::SizeDrag {
                        id: self.id.clone(),
                        x_norm,
                        y_norm,
                    }));
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging => {
                    state.dragging = false;
                    return Some(iced::widget::Action::publish(SettingsMessage::SizeCommit {
                        id: self.id.clone(),
                    }));
                }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;

        // Dim track
        let track_bg = Color { a: 0.10, ..self.bg };
        frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), track_bg);
        frame.stroke(
            &Path::rectangle(Point::ORIGIN, Size::new(w - 1.0, h - 1.0)),
            Stroke::default().with_color(self.border).with_width(1.0),
        );

        // Current fraction of the absolute maximum. Because we map
        // x_norm -> [0, max_w] linearly, the fill ratio directly
        // corresponds to the tooltip's aspect ratio when the track
        // itself matches the aspect ratio of the maximum bounds.
        let w_frac = (self.w as f32 / self.max_w as f32).clamp(0.0, 1.0);
        let h_frac = (self.h as f32 / self.max_h as f32).clamp(0.0, 1.0);
        let handle_x = (w * w_frac).max(8.0).min(w - 8.0);
        let handle_y = (h * h_frac).max(8.0).min(h - 8.0);

        // Filled region from origin to handle
        let fill = Color { a: 0.45, ..self.fg };
        frame.fill_rectangle(Point::ORIGIN, Size::new(handle_x, handle_y), fill);

        // Handle
        let handle_r = 6.0;
        let circle = Path::circle(Point::new(handle_x, handle_y), handle_r);
        frame.fill(&circle, self.fg);
        frame.stroke(&circle, Stroke::default().with_color(self.bg).with_width(2.0));

        // W × H — center of box, high-contrast
        let label = format!("{} × {}", self.w, self.h);
        frame.fill_text(Text {
            content: label,
            position: Point::new(w / 2.0, h / 2.0),
            color: self.fg,
            size: iced::Pixels(13.0),
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });

        // Hint label bottom-right
        let hint = if state.dragging { "drag…" } else { "drag to resize" };
        frame.fill_text(Text {
            content: hint.to_string(),
            position: Point::new(w - 4.0, h - 2.0),
            color: Color { a: 0.55, ..self.muted },
            size: iced::Pixels(9.0),
            align_x: iced::alignment::Horizontal::Right.into(),
            align_y: iced::alignment::Vertical::Bottom,
            ..Text::default()
        });

        vec![frame.into_geometry()]
    }
}
