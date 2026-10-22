use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Theme};

/// QML-ported animation state (3 stages, sequential)
pub struct Anim {
    pub w: f32,
    pub h: f32,
    pub slant: f32,
    pub opacity: f32,
}

pub fn compute(
    elapsed_ms: f32,
    cap_w: f32,
    final_w: f32,
    h_end: f32,
    slant_base: f32,
    bar_h: f32,
    drop_ms: f32,
    expand_ms: f32,
) -> Anim {
    // Matches QML: no pause by default (override via env for testing)
    let pause_ms: f32 = std::env::var("MBS_TOOLTIP_PAUSE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    let bar_h = bar_h.max(1.0);
    let slant_at = |h: f32| slant_base * (h / bar_h);

    let cap_core = (cap_w - slant_base).max(1.0);
    let slant_end = slant_at(h_end);
    let final_core = (final_w - slant_end).max(cap_core);

    // ── Stage 1: Height (OutCubic) — 0..drop_ms ──────────────────
    let h_t = (elapsed_ms / drop_ms.max(1.0)).clamp(0.0, 1.0);
    let h_ease = 1.0 - (1.0 - h_t).powi(3);
    let h = h_ease * h_end;
    let slant_h = slant_at(h);

    // ── Stage 2: Width (OutCubic) — after drop + pause ───────────
    let w_start = drop_ms + pause_ms;
    let w_t = ((elapsed_ms - w_start) / expand_ms.max(1.0)).clamp(0.0, 1.0);
    let w_ease = 1.0 - (1.0 - w_t).powi(3);
    let core = cap_core + (final_core - cap_core) * w_ease;
    let w = core + slant_h;

    // ── Stage 3: Opacity (OutQuad) — 0.6 × drop_ms, after width ──
    let o_start = w_start + expand_ms;
    let opacity_ms = (drop_ms * 0.6).max(1.0);
    let o_t = ((elapsed_ms - o_start) / opacity_ms).clamp(0.0, 1.0);
    let opacity = o_t * (2.0 - o_t);

    Anim { w, h, slant: slant_h, opacity }
}

/// QML-ported close sequence (3 stages, sequential, In-easing).
/// Stage A: opacity 1→0 (InQuad, 0.4·T)
/// Stage B: core shrinks to cap_core (InCubic, 0.7·T)
/// Stage C: height collapses to 0 (InCubic, 0.8·T)
pub fn compute_close(
    elapsed_ms: f32,
    cap_w: f32,
    final_w: f32,
    h_end: f32,
    slant_base: f32,
    bar_h: f32,
    base_ms: f32,
) -> Anim {
    let bar_h = bar_h.max(1.0);
    let slant_at = |h: f32| slant_base * (h / bar_h);

    let cap_core = (cap_w - slant_base).max(1.0);
    let slant_end = slant_at(h_end);
    let final_core = (final_w - slant_end).max(cap_core);

    // QML: each stage duration = fraction of T
    let fade_ms = (base_ms * 0.4).max(1.0);
    let width_ms = (base_ms * 0.7).max(1.0);
    let height_ms = (base_ms * 0.8).max(1.0);

    // ── Stage A: opacity fade (InQuad) ────────────────────────────
    let o_t = (elapsed_ms / fade_ms).clamp(0.0, 1.0);
    let opacity = 1.0 - o_t * o_t;

    // ── Stage B: core shrink (InCubic) — starts after fade ────────
    let w_elapsed = (elapsed_ms - fade_ms).max(0.0);
    let w_t = (w_elapsed / width_ms).clamp(0.0, 1.0);
    let w_ease = w_t * w_t * w_t;
    let core = final_core + (cap_core - final_core) * w_ease;

    // ── Stage C: height collapse (InCubic) — starts after width ───
    let h_elapsed = (elapsed_ms - fade_ms - width_ms).max(0.0);
    let h_t = (h_elapsed / height_ms).clamp(0.0, 1.0);
    let h_ease = h_t * h_t * h_t;
    let h = h_end * (1.0 - h_ease);

    let slant_h = slant_at(h);
    let w = core + slant_h;

    Anim { w, h, slant: slant_h, opacity }
}

// Backward-compat tuple form (unused but keeps old call sites working)

// ── Canvas ──────────────────────────────────────────────────────

pub struct ParallelogramBg {
    pub w: f32,
    pub h: f32,
    pub slant: f32,
    pub slant_right: bool,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
}

impl ParallelogramBg {
    pub fn view<Message: 'static>(self) -> Element<'static, Message> {
        let w = self.w;
        let h = self.h;
        let inner = TooltipBg {
            w: self.w,
            h: self.h,
            slant: self.slant,
            slant_right: self.slant_right,
            background: self.background,
            border_color: self.border_color,
            border_width: self.border_width,
        };
        canvas::Canvas::new(inner)
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .into()
    }
}

pub struct TooltipBg {
    pub w: f32,
    pub h: f32,
    pub slant: f32,
    pub slant_right: bool,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
}

impl<Message> canvas::Program<Message> for TooltipBg {
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
        vec![frame.into_geometry()]
    }
}

pub fn clip_content<'a, Message: 'a>(
    content: Element<'a, Message>,
    anim_w: f32, anim_h: f32, grow_right: bool,
) -> Element<'a, Message> {
    let align = if grow_right {
        iced::alignment::Horizontal::Left
    } else {
        iced::alignment::Horizontal::Right
    };
    let c: iced::widget::Container<'a, Message> = iced::widget::container(content)
        .width(Length::Fixed(anim_w))
        .height(Length::Fixed(anim_h))
        .align_x(align)
        .align_y(iced::alignment::Vertical::Top)
        .clip(true);
    c.into()
}

pub fn align_to_side<'a, Message: 'a>(
    el: Element<'a, Message>, grow_right: bool,
) -> Element<'a, Message> {
    let align = if grow_right {
        iced::alignment::Horizontal::Left
    } else {
        iced::alignment::Horizontal::Right
    };
    let c: iced::widget::Container<'a, Message> = iced::widget::container(el)
        .width(Length::Fill).height(Length::Fill)
        .align_x(align).align_y(iced::alignment::Vertical::Top);
    c.into()
}
