use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Theme};

pub struct Anim { pub w: f32, pub h: f32, pub slant: f32, pub opacity: f32 }

#[inline]
fn smoothstep(t: f32) -> f32 { let t = t.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

pub fn compute(elapsed_ms: f32, cap_w: f32, final_w: f32, h_end: f32,
               slant_base: f32, bar_h: f32, drop_ms: f32, expand_ms: f32) -> Anim {
    let pause_ms: f32 = std::env::var("MBS_TOOLTIP_PAUSE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let bar_h = bar_h.max(1.0);
    let slant_at = |h: f32| slant_base * (h / bar_h);
    let cap_core = (cap_w - slant_base).max(1.0);
    let slant_end = slant_at(h_end);
    let final_core = (final_w - slant_end).max(cap_core);
    let h_t = (elapsed_ms / drop_ms.max(1.0)).clamp(0.0, 1.0);
    let h = smoothstep(h_t) * h_end;
    let slant_h = slant_at(h);
    let w_start = drop_ms + pause_ms;
    let w_t = ((elapsed_ms - w_start) / expand_ms.max(1.0)).clamp(0.0, 1.0);
    let core = cap_core + (final_core - cap_core) * smoothstep(w_t);
    let w = core + slant_h;
    let o_start = w_start + expand_ms;
    let opacity_ms = (drop_ms * 0.6).max(1.0);
    let o_t = ((elapsed_ms - o_start) / opacity_ms).clamp(0.0, 1.0);
    let opacity = o_t * (2.0 - o_t);
    Anim { w, h, slant: slant_h, opacity }
}

pub fn compute_close(elapsed_ms: f32, cap_w: f32, final_w: f32, h_end: f32,
                     slant_base: f32, bar_h: f32, base_ms: f32) -> Anim {
    let bar_h = bar_h.max(1.0);
    let slant_at = |h: f32| slant_base * (h / bar_h);
    let cap_core = (cap_w - slant_base).max(1.0);
    let slant_end = slant_at(h_end);
    let final_core = (final_w - slant_end).max(cap_core);
    let fade_ms = (base_ms * 0.4).max(1.0);
    let width_ms = (base_ms * 0.7).max(1.0);
    let height_ms = (base_ms * 0.8).max(1.0);
    let o_t = (elapsed_ms / fade_ms).clamp(0.0, 1.0);
    let opacity = 1.0 - o_t * o_t;
    let w_elapsed = (elapsed_ms - fade_ms).max(0.0);
    let w_t = (w_elapsed / width_ms).clamp(0.0, 1.0);
    let core = final_core + (cap_core - final_core) * w_t * w_t * w_t;
    let h_elapsed = (elapsed_ms - fade_ms - width_ms).max(0.0);
    let h_t = (h_elapsed / height_ms).clamp(0.0, 1.0);
    let h = h_end * (1.0 - h_t * h_t * h_t);
    let slant_h = slant_at(h);
    Anim { w: core + slant_h, h, slant: slant_h, opacity }
}

pub struct ParallelogramBg {
    pub w: f32, pub h: f32, pub slant: f32, pub slant_right: bool,
    pub background: Color, pub border_color: Color, pub border_width: f32,
}

impl ParallelogramBg {
    pub fn view<Message: 'static>(self) -> Element<'static, Message> {
        let w = self.w; let h = self.h;
        let inner = TooltipBg {
            w: self.w, h: self.h, slant: self.slant, slant_right: self.slant_right,
            background: self.background, border_color: self.border_color, border_width: self.border_width,
        };
        canvas::Canvas::new(inner).width(Length::Fixed(w)).height(Length::Fixed(h)).into()
    }
}

pub struct TooltipBg {
    pub w: f32, pub h: f32, pub slant: f32, pub slant_right: bool,
    pub background: Color, pub border_color: Color, pub border_width: f32,
}

impl<Message> canvas::Program<Message> for TooltipBg {
    type State = ();
    fn draw(&self, _s: &Self::State, renderer: &iced::Renderer, _t: &Theme,
            bounds: Rectangle, _c: iced::mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let half = self.border_width / 2.0;
        let w = self.w; let h = self.h; let sl = self.slant;
        let path = if self.slant_right {
            Path::new(|b| { b.move_to(Point::new(sl + half, half)); b.line_to(Point::new(w - half, half));
                b.line_to(Point::new(w - sl - half, h - half)); b.line_to(Point::new(half, h - half)); b.close(); })
        } else {
            Path::new(|b| { b.move_to(Point::new(half, half)); b.line_to(Point::new(w - sl - half, half));
                b.line_to(Point::new(w - half, h - half)); b.line_to(Point::new(sl + half, h - half)); b.close(); })
        };
        if self.background.a > 0.0 { frame.fill(&path, self.background); }
        if self.border_width > 0.0 && self.border_color.a > 0.0 {
            frame.stroke(&path, Stroke::default().with_color(self.border_color).with_width(self.border_width));
        }
        vec![frame.into_geometry()]
    }
}

pub fn clip_content<'a, Message: 'a>(content: Element<'a, Message>, anim_w: f32, anim_h: f32, grow_right: bool) -> Element<'a, Message> {
    let align = if grow_right { iced::alignment::Horizontal::Left } else { iced::alignment::Horizontal::Right };
    iced::widget::container(content).width(Length::Fixed(anim_w)).height(Length::Fixed(anim_h))
        .align_x(align).align_y(iced::alignment::Vertical::Top).clip(true).into()
}

pub fn align_to_side<'a, Message: 'a>(el: Element<'a, Message>, grow_right: bool) -> Element<'a, Message> {
    let align = if grow_right { iced::alignment::Horizontal::Left } else { iced::alignment::Horizontal::Right };
    iced::widget::container(el).width(Length::Fill).height(Length::Fill)
        .align_x(align).align_y(iced::alignment::Vertical::Top).into()
}
