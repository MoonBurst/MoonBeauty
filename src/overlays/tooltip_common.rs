//! Shared tooltip rendering primitives.
//!
//! Extracted from bar/weather/tooltip.rs, overlays/system_tooltip.rs,
//! and overlays/info_tooltip.rs. Those three files each independently
//! defined `slanted_row`, and two of them each defined `KillButtonShape`
//! (struct + impl). Every tooltip layout tweak required editing every
//! copy. Consolidating here makes those changes one-file again.
//!
//! Extraction is behavior-preserving: the bodies are byte-identical to
//! the originals (verified programmatically before extraction).

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{container, row};
use iced::{Color, Element, Length, Point, Rectangle, Theme};

pub fn slanted_row<'a, M: 'a>(
    inner: Element<'a, M>,
    y_mid: f32,
    total_h: f32,
    slant: f32,
    slant_right: bool,
    safe: f32,
    total_w: f32,
    row_h: f32,
) -> Element<'a, M> {
    let frac = (y_mid / total_h.max(1.0)).clamp(0.0, 1.0);
    let (lp, rp) = if slant_right {
        (slant * (1.0 - frac) + safe, slant * frac + safe)
    } else {
        (slant * frac + safe, slant * (1.0 - frac) + safe)
    };
    let inner_w = (total_w - lp - rp).max(10.0);

    let inner_clipped = container(inner)
        .width(Length::Fixed(inner_w))
        .height(Length::Fixed(row_h))
        .clip(true);

    row![
        iced::widget::container(iced::widget::Space::new()).width(Length::Fixed(lp)),
        inner_clipped,
        iced::widget::container(iced::widget::Space::new()).width(Length::Fixed(rp)),
    ]
    .height(Length::Fixed(row_h))
    .width(Length::Fixed(total_w))
    .into()
}

pub fn octagon_row<'a, M: 'a>(
    inner: Element<'a, M>,
    y_mid: f32,
    total_h: f32,
    cut: f32,
    safe: f32,
    total_w: f32,
    row_h: f32,
) -> Element<'a, M> {
    let half_row = row_h * 0.5;
    let y_top = (y_mid - half_row).max(0.0);
    let y_bot = (y_mid + half_row).min(total_h);

    // Inset at row boundary: 0 in the middle band, rises linearly from 0
    // to `cut` as you approach the top or bottom edge.
    let inset_at = |y: f32| -> f32 {
        if cut <= 0.0 {
            return 0.0;
        }
        if y < cut {
            cut - y
        } else if y > total_h - cut {
            y - (total_h - cut)
        } else {
            0.0
        }
    };

    // Take the max of top/bottom insets so a row that straddles the
    // diagonal doesn't poke out on one corner.
    let inset = inset_at(y_top).max(inset_at(y_bot));
    let lp = inset + safe;
    let rp = inset + safe;
    let inner_w = (total_w - lp - rp).max(10.0);

    let inner_clipped = container(inner)
        .width(Length::Fixed(inner_w))
        .height(Length::Fixed(row_h))
        .clip(true);

    row![
        iced::widget::container(iced::widget::Space::new()).width(Length::Fixed(lp)),
        inner_clipped,
        iced::widget::container(iced::widget::Space::new()).width(Length::Fixed(rp)),
    ]
    .height(Length::Fixed(row_h))
    .width(Length::Fixed(total_w))
    .into()
}

pub struct KillButtonShape {
    pub w: f32,
    pub h: f32,
    pub slant: f32,
    pub slant_right: bool,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub x_color: Color,
}

impl<Message> canvas::Program<Message> for KillButtonShape {
    type State = ();
    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let half = self.border_width / 2.0;
        let w = self.w;
        let h = self.h;
        let sl = self.slant;

        let path = if self.slant_right {
            Path::new(|b| {
                b.move_to(Point::new(sl + half, half));
                b.line_to(Point::new(w - half, half));
                b.line_to(Point::new(w - sl - half, h - half));
                b.line_to(Point::new(half, h - half));
                b.close();
            })
        } else {
            Path::new(|b| {
                b.move_to(Point::new(half, half));
                b.line_to(Point::new(w - sl - half, half));
                b.line_to(Point::new(w - half, h - half));
                b.line_to(Point::new(sl + half, h - half));
                b.close();
            })
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

        let cx = w / 2.0;
        let cy = h / 2.0;
        let r_x = (w * 0.26).max(3.0);
        let r_y = (h * 0.26).max(3.0);
        let stroke_w = (h * 0.09).max(1.5);

        let inner_h = (h - 2.0 * half).max(1.0);
        let sl_sign = if self.slant_right { 1.0 } else { -1.0 };
        let delta = sl_sign * sl * r_y / inner_h;

        let tl = Point::new(cx - r_x + delta, cy - r_y);
        let tr = Point::new(cx + r_x + delta, cy - r_y);
        let bl = Point::new(cx - r_x - delta, cy + r_y);
        let br = Point::new(cx + r_x - delta, cy + r_y);

        let x_stroke = Stroke::default()
            .with_color(self.x_color)
            .with_width(stroke_w)
            .with_line_cap(canvas::LineCap::Round);
        let diag_a = Path::new(|b| {
            b.move_to(tl);
            b.line_to(br);
        });
        let diag_b = Path::new(|b| {
            b.move_to(bl);
            b.line_to(tr);
        });
        frame.stroke(&diag_a, x_stroke);
        frame.stroke(&diag_b, x_stroke);

        vec![frame.into_geometry()]
    }
}
