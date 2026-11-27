//! Shared tooltip rendering primitives.

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

#[derive(Debug, Clone)]
pub struct InBarTooltipConfig {
    pub id: String,
    pub w: u32,
    pub h: u32,
    pub cx: i32,
    pub sw: u32,
    pub cap_w: f32,
    pub grow_right: bool,
    pub slant_l: crate::libraries::style::shape_box::SlantSide,
    pub slant_r: crate::libraries::style::shape_box::SlantSide,
    pub drop_ms: f32,
    pub expand_ms: f32,
}

pub fn layer_anchor_and_margin_for(
    s: &crate::libraries::config::ShellSettings,
    w: u32,
    center_on_capsule: bool,
    cx: i32,
    sw: i32,
    cap_w: f32,
    bias_l: i32,
    bias_r: i32,
) -> (iced_layershell::reexport::Anchor, (i32, i32, i32, i32)) {
    use iced_layershell::reexport::Anchor;
    let grow_right = cx < sw / 2;

    let bar_h = s.bar_height as i32;
    let margin_top = (bar_h - s.control_border_width as i32 - 1).max(0);
    let half_cap = (cap_w / 2.0) as i32;

    let adjust_left = s.effective_tooltip_offset_left() + bias_l;
    let adjust_right = s.effective_tooltip_offset_right() + bias_r;

    if center_on_capsule {
        let margin_left = (cx - (w as i32) / 2).max(0);
        (Anchor::Top | Anchor::Left, (margin_top, 0, 0, margin_left))
    } else if grow_right {
        (
            Anchor::Top | Anchor::Left,
            (margin_top, 0, 0, (cx - half_cap + adjust_left).max(0)),
        )
    } else {
        (
            Anchor::Top | Anchor::Right,
            (margin_top, (sw - cx - half_cap + adjust_right).max(0), 0, 0),
        )
    }
}

pub fn truncate_proc_name(name: &str) -> String {
    let max = 22;
    if name.chars().count() > max {
        let mut s: String = name.chars().take(max - 3).collect();
        s.push_str("...");
        s
    } else {
        name.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn proc_kill_row<'a, M: Clone + 'a>(
    name: String,
    metric_str: String,
    metric_w: f32,
    on_kill: M,
    t: &crate::libraries::config::theme::ShellTheme,
    font_size: u16,
    alpha: f32,
    row_w: f32,
    row_h: f32,
    slant_right: bool,
    tooltip_slope: f32,
) -> Element<'a, M> {
    let apply = |c: Color| Color {
        a: c.a * alpha,
        ..c
    };
    let muted = apply(t.muted);
    let danger = apply(t.danger);
    let text_col = apply(t.text);
    let surface_alt = apply(t.surface_alt);

    let kill_w = 32.0;
    let kill_h = 24.0;
    let kill_border = t.control_border_width;
    let half_k = kill_border * 0.5;
    let x_slant = tooltip_slope * (kill_h - 2.0 * half_k);

    let kill_shape = KillButtonShape {
        w: kill_w,
        h: kill_h,
        slant: x_slant,
        slant_right,
        background: surface_alt,
        border_color: danger,
        border_width: kill_border,
        x_color: danger,
    };

    let kill_canvas: Element<'a, M> = canvas::Canvas::new(kill_shape)
        .width(Length::Fixed(kill_w))
        .height(Length::Fixed(kill_h))
        .into();

    let kill_btn = iced::widget::button(kill_canvas)
        .on_press(on_kill)
        .padding(0)
        .width(Length::Fixed(kill_w))
        .height(Length::Fixed(row_h))
        .style(|_t, _s| iced::widget::button::Style {
            background: None,
            ..Default::default()
        });

    let name_clip = container(
        iced::widget::text(name)
            .size((font_size.saturating_sub(4).max(9)) as f32)
            .color(text_col)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(Length::Fixed(row_h))
    .align_y(iced::Alignment::Center)
    .clip(true);

    let metric_container = container(
        iced::widget::text(metric_str)
            .size((font_size.saturating_sub(4).max(9)) as f32)
            .color(muted)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fixed(metric_w))
    .height(Length::Fixed(row_h))
    .align_y(iced::Alignment::Center)
    .padding([0.0, 6.0])
    .clip(true);

    row![name_clip, metric_container, kill_btn]
        .spacing(8.0)
        .align_y(iced::Alignment::Center)
        .width(Length::Fixed(row_w))
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn slanted_search_bar<'a, M: Clone + 'static>(
    query: &str,
    on_input: fn(String) -> M,
    t: &crate::libraries::config::theme::ShellTheme,
    control_border_width: f32,
    alpha: f32,
    y: f32,
    total_h: f32,
    slant_h: f32,
    slant_right: bool,
    safe: f32,
    w_f: f32,
    fs: u16,
    tooltip_slope: f32,
    gap: f32,
    frame_ms: u64,
) -> (Element<'a, M>, f32) {
    let search_h = 40.0;
    let y_mid_search = y + search_h * 0.5;
    let frac_s = (y_mid_search / total_h.max(1.0)).clamp(0.0, 1.0);
    let (lp_s, rp_s) = if slant_right {
        (slant_h * (1.0 - frac_s) + safe, slant_h * frac_s + safe)
    } else {
        (slant_h * frac_s + safe, slant_h * (1.0 - frac_s) + safe)
    };
    let search_w = (w_f - lp_s - rp_s).max(100.0);
    let half_s = control_border_width * 0.5;
    let search_slant = tooltip_slope * (search_h - 2.0 * half_s);

    let search_bg = crate::overlays::tooltip_engine::ParallelogramBg {
        w: search_w,
        h: search_h,
        slant: search_slant,
        slant_right,
        background: Color {
            a: t.surface_alt.a * alpha,
            ..t.surface_alt
        },
        border_color: Color {
            a: t.text.a * alpha,
            ..t.text
        },
        border_width: control_border_width,
        frame: frame_ms,
    };

    let input_text_col = Color { a: 1.0, ..t.text };
    let input_muted = Color { a: 1.0, ..t.muted };
    let selection_col = Color { a: 1.0, ..t.focus };

    let search_input = iced::widget::text_input("Search programs...", query)
        .on_input(on_input)
        .padding(iced::Padding {
            top: 8.0,
            bottom: 8.0,
            left: search_slant + 8.0,
            right: search_slant + 8.0,
        })
        .size(fs as f32)
        .width(Length::Fill)
        .style(move |_t, _s| iced::widget::text_input::Style {
            background: iced::Background::Color(Color::TRANSPARENT),
            border: iced::Border::default(),
            icon: input_muted,
            placeholder: input_muted,
            value: input_text_col,
            selection: selection_col,
        });

    let search_layer: Element<'a, M> = iced::widget::Stack::new()
        .push(search_bg.view())
        .push(
            container(search_input)
                .width(Length::Fixed(search_w))
                .height(Length::Fixed(search_h))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fixed(search_w))
        .height(Length::Fixed(search_h))
        .into();

    let row_el = slanted_row(
        search_layer,
        y_mid_search,
        total_h,
        slant_h,
        slant_right,
        safe,
        w_f,
        search_h,
    );

    (row_el, y + search_h + gap)
}
