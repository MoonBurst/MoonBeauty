use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};

pub struct Anim {
    pub w: f32,
    pub h: f32,
    pub slant: f32,
    pub cut: f32,
    pub opacity: f32,
}

/// Snapshot of animation state at the moment close was triggered.
#[derive(Debug, Clone, Copy)]
pub struct ClosingState {
    pub started: std::time::Instant,
    pub from_w: f32,
    pub from_h: f32,
    pub from_opacity: f32,
}

#[inline]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    let pause_ms: f32 = std::env::var("MBS_TOOLTIP_PAUSE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
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
    let cut = (w.min(h) * 0.15).max(0.0);
    Anim {
        w,
        h,
        slant: slant_h,
        cut,
        opacity,
    }
}

/// Interrupt-safe close: same choreography as `compute_close`, but
/// starts from the captured (from_w, from_h, from_opacity). Duration
/// scales with how much width still needs to shrink, so a tooltip
/// 30%-open closes proportionally faster than one at 100%.
pub fn compute_close_from(
    elapsed_ms: f32,
    from_w: f32,
    from_h: f32,
    from_opacity: f32,
    cap_w: f32,
    final_w: f32,
    slant_base: f32,
    bar_h: f32,
    base_ms: f32,
) -> Anim {
    let bar_h = bar_h.max(1.0);
    let slant_at = |h: f32| slant_base * (h / bar_h);
    let cap_core = (cap_w - slant_base).max(1.0);
    let from_slant = slant_at(from_h);
    let from_core = (from_w - from_slant).max(cap_core);

    let full_width_range = (final_w - cap_w).max(1.0);
    let remaining_width = (from_w - cap_w).max(0.0);
    let frac = (remaining_width / full_width_range).clamp(0.0, 1.0);
    // sqrt so a barely-open tooltip still gets a visible ~150ms close.
    let scale = frac.sqrt().max(0.15);

    let fade_ms = (base_ms * 0.4 * scale).max(1.0);
    let width_ms = (base_ms * 0.7 * scale).max(1.0);
    let height_ms = (base_ms * 0.8 * scale).max(1.0);

    let o_t = (elapsed_ms / fade_ms).clamp(0.0, 1.0);
    let opacity = from_opacity * (1.0 - o_t * o_t);

    let w_elapsed = (elapsed_ms - fade_ms).max(0.0);
    let w_t = (w_elapsed / width_ms).clamp(0.0, 1.0);
    let core = from_core + (cap_core - from_core) * w_t * w_t * w_t;

    let h_elapsed = (elapsed_ms - fade_ms - width_ms).max(0.0);
    let h_t = (h_elapsed / height_ms).clamp(0.0, 1.0);
    let h = from_h * (1.0 - h_t * h_t * h_t);

    let slant_h = slant_at(h);
    let w_final = core + slant_h;
    let cut = (w_final.min(h) * 0.15).max(0.0);

    Anim {
        w: w_final,
        h,
        slant: slant_h,
        cut,
        opacity,
    }
}

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
    let w_final = core + slant_h;
    let cut = (w_final.min(h) * 0.15).max(0.0);
    Anim {
        w: w_final,
        h,
        slant: slant_h,
        cut,
        opacity,
    }
}

pub struct ParallelogramBg {
    pub w: f32,
    pub h: f32,
    pub slant: f32,
    pub slant_right: bool,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
    /// Per-frame counter. Iced 0.14's reactive renderer skips repaints
    /// when the widget tree is identical. Bumping this every view() call
    /// changes the hidden marker text, forcing the tree to diff and the
    /// canvas to redraw. Without this, animated tooltips tick at ~20fps
    /// instead of 60fps because iced coalesces identical frames.
    pub frame: u64,
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
        let canvas_el: Element<'static, Message> = canvas::Canvas::new(inner)
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .into();
        // V228: force tree diff. Iced 0.14 skips repaints when widget trees are
        // identical (reactive rendering). Bumping a hidden text widget's content
        // every frame invalidates the enclosing Stack, forcing the canvas below
        // it to redraw. Without this, tick-only animation runs at ~20fps.
        // Force a tree diff using a zero-size Space widget. Varying its
        // width by a fraction of a pixel changes the layout tree, which
        // forces iced to redraw the canvas underneath. This bypasses
        // cosmic-text shaping entirely, saving 2-3ms per frame.
        let marker_w = (self.frame % 2) as f32;
        let marker_el: Element<'static, Message> = iced::widget::Space::new()
            .width(Length::Fixed(marker_w))
            .height(Length::Fixed(0.0))
            .into();
        iced::widget::Stack::new()
            .push(canvas_el)
            .push(marker_el)
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
    // Tracks last-rendered (bounds, w, h, slant). Redraws only when the
    // animation actually produces new geometry — not on every event.
    type State = Option<(Rectangle, f32, f32, f32)>;

    fn update(
        &self,
        state: &mut Self::State,
        _event: &iced::Event,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        let key = (bounds, self.w, self.h, self.slant);
        if *state != Some(key) {
            *state = Some(key);
            return Some(iced::widget::Action::request_redraw());
        }
        None
    }
    fn draw(
        &self,
        _s: &Self::State,
        renderer: &iced::Renderer,
        _t: &Theme,
        bounds: Rectangle,
        _c: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if std::env::var_os("MBS_TICK_DIAG").is_some() {
            use std::sync::{Mutex, OnceLock};
            static LAST: OnceLock<Mutex<Option<std::time::Instant>>> = OnceLock::new();
            let m = LAST.get_or_init(|| Mutex::new(None));
            let mut g = m.lock().unwrap();
            let now = std::time::Instant::now();
            if let Some(prev) = *g {
                let us = now.duration_since(prev).as_micros() as f64;
                eprintln!("[tick-diag] dt_ms={:.2}", us / 1000.0);
            }
            *g = Some(now);
        }

        // Use bounds, not self.w/self.h. When the parent clips the canvas
        // (e.g., a fixed-height container), bounds is smaller than self.h
        // and self.w. Drawing at self.w/self.h would overshoot the visible area.
        let bw = self.border_width;
        let half = bw / 2.0;
        let w = bounds.width;
        let h = bounds.height;
        // Scale slant so slope matches when the canvas is clipped shorter.
        let sl = self.slant * (h / self.h.max(1.0));
        // Keep the bottom border + fill safely inside the visible canvas.
        let border_inset = bw.max(4.0);

        // Fill path — bottom at h - border_inset, not self.h.
        let path = if self.slant_right {
            Path::new(|b| {
                b.move_to(Point::new(sl + half, half));
                b.line_to(Point::new(w - half, half));
                b.line_to(Point::new(w - sl - half, h - border_inset));
                b.line_to(Point::new(half, h - border_inset));
                b.close();
            })
        } else {
            Path::new(|b| {
                b.move_to(Point::new(half, half));
                b.line_to(Point::new(w - sl - half, half));
                b.line_to(Point::new(w - half, h - border_inset));
                b.line_to(Point::new(sl + half, h - border_inset));
                b.close();
            })
        };
        if self.background.a > 0.0 {
            frame.fill(&path, self.background);
        }

        if bw > 0.0 && self.border_color.a > 0.0 {
            // Parallelogram corner x-coords:
            //   slant_right=true  (\ \): top-left=sl+half, top-right=w-half, bot-left=half, bot-right=w-sl-half
            //   slant_right=false (/ /): top-left=half, top-right=w-sl-half, bot-left=sl+half, bot-right=w-half
            let (top_left_x, top_right_x, bot_left_x, bot_right_x) = if self.slant_right {
                (sl + half, w - half, half, w - sl - half)
            } else {
                (half, w - sl - half, sl + half, w - half)
            };

            // Top border — spans the parallelogram's top edge exactly
            let top_w = (top_right_x - top_left_x).max(1.0);
            frame.fill_rectangle(
                Point::new(top_left_x, half),
                Size::new(top_w, bw),
                self.border_color,
            );

            // Bottom border — spans the parallelogram's bottom edge exactly
            let bot_w = (bot_right_x - bot_left_x).max(1.0);
            frame.fill_rectangle(
                Point::new(bot_left_x, h - border_inset - bw),
                Size::new(bot_w, bw),
                self.border_color,
            );

            // Diagonal sides — stroked, connecting the corners above
            let stroke = Stroke::default()
                .with_color(self.border_color)
                .with_width(bw);
            if self.slant_right {
                let left_edge = Path::new(|b| {
                    b.move_to(Point::new(sl + half, half));
                    b.line_to(Point::new(half, h - border_inset));
                });
                let right_edge = Path::new(|b| {
                    b.move_to(Point::new(w - half, half));
                    b.line_to(Point::new(w - sl - half, h - border_inset));
                });
                frame.stroke(&left_edge, stroke);
                frame.stroke(&right_edge, stroke);
            } else {
                let left_edge = Path::new(|b| {
                    b.move_to(Point::new(half, half));
                    b.line_to(Point::new(sl + half, h - border_inset));
                });
                let right_edge = Path::new(|b| {
                    b.move_to(Point::new(w - sl - half, half));
                    b.line_to(Point::new(w - half, h - border_inset));
                });
                frame.stroke(&left_edge, stroke);
                frame.stroke(&right_edge, stroke);
            }
        }
        vec![frame.into_geometry()]
    }
}

pub fn clip_content<'a, Message: 'a>(
    content: Element<'a, Message>,
    anim_w: f32,
    anim_h: f32,
    align: iced::alignment::Horizontal,
) -> Element<'a, Message> {
    iced::widget::container(content)
        .width(Length::Fixed(anim_w))
        .height(Length::Fixed(anim_h))
        .align_x(align)
        .align_y(iced::alignment::Vertical::Top)
        .clip(true)
        .into()
}

pub fn align_to_side<'a, Message: 'a>(
    el: Element<'a, Message>,
    align: iced::alignment::Horizontal,
) -> Element<'a, Message> {
    iced::widget::container(el)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(align)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

/// Detect the focused monitor's refresh rate in Hz.
///
/// Reads `swaymsg -t get_outputs -r` once and caches the result via
/// `OnceLock`. Falls back to 60Hz if detection fails. Override via
/// `MBS_REFRESH_HZ` env var if the compositor isn't sway or detection
/// misreads the current mode.
///
/// Used to size the animation tick interval. At 144Hz we want ~5ms
/// ticks (200fps source) to exceed display refresh with margin, not the
/// 16ms (60fps) that's appropriate on a 60Hz panel.
pub fn detect_refresh_hz() -> u32 {
    use std::sync::OnceLock;
    static CACHED: OnceLock<u32> = OnceLock::new();
    *CACHED.get_or_init(|| {
        if let Ok(v) = std::env::var("MBS_REFRESH_HZ") {
            if let Ok(hz) = v.parse::<u32>() {
                if (30..=480).contains(&hz) {
                    eprintln!("[refresh-detect] MBS_REFRESH_HZ override = {}Hz", hz);
                    return hz;
                }
            }
        }
        if let Ok(out) = std::process::Command::new("swaymsg")
            .args(["-t", "get_outputs", "-r"])
            .output()
        {
            if out.status.success() {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                    if let Some(arr) = v.as_array() {
                        for o in arr {
                            if o.get("focused").and_then(|f| f.as_bool()) == Some(true) {
                                if let Some(hz) = o
                                    .get("current_mode")
                                    .and_then(|m| m.get("refresh"))
                                    .and_then(|r| r.as_u64())
                                {
                                    let hz = ((hz as f64) / 1000.0).round() as u32;
                                    if (30..=480).contains(&hz) {
                                        eprintln!("[refresh-detect] focused monitor = {}Hz", hz);
                                        return hz;
                                    }
                                }
                            }
                        }
                        for o in arr {
                            if let Some(hz) = o
                                .get("current_mode")
                                .and_then(|m| m.get("refresh"))
                                .and_then(|r| r.as_u64())
                            {
                                let hz = ((hz as f64) / 1000.0).round() as u32;
                                if (30..=480).contains(&hz) {
                                    eprintln!("[refresh-detect] first monitor = {}Hz", hz);
                                    return hz;
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!("[refresh-detect] fallback = 60Hz");
        60
    })
}

// ── OctagonBg: same animation cadence as ParallelogramBg but with 8
// equal corner cuts (diamond-cornered rectangle). Used by the clock
// tooltip — it's the keystone bar module and deserves a distinct shape.

pub struct OctagonBg {
    pub w: f32,
    pub h: f32,
    pub cut: f32,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub frame: u64,
}

impl OctagonBg {
    pub fn view<Message: 'static>(self) -> Element<'static, Message> {
        let w = self.w;
        let h = self.h;
        let inner = OctagonCanvas {
            w: self.w,
            h: self.h,
            cut: self.cut,
            background: self.background,
            border_color: self.border_color,
            border_width: self.border_width,
        };
        let canvas_el: Element<'static, Message> = canvas::Canvas::new(inner)
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .into();
        let marker_w = (self.frame % 2) as f32;
        let marker_el: Element<'static, Message> = iced::widget::Space::new()
            .width(Length::Fixed(marker_w))
            .height(Length::Fixed(0.0))
            .into();
        iced::widget::Stack::new()
            .push(canvas_el)
            .push(marker_el)
            .into()
    }
}

pub struct OctagonCanvas {
    pub w: f32,
    pub h: f32,
    pub cut: f32,
    pub background: Color,
    pub border_color: Color,
    pub border_width: f32,
}

impl<Message> canvas::Program<Message> for OctagonCanvas {
    // (bounds, w, h, cut) — same redraw-on-change pattern as TooltipBg.
    type State = Option<(Rectangle, f32, f32, f32)>;

    fn update(
        &self,
        state: &mut Self::State,
        _event: &iced::Event,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<iced::widget::Action<Message>> {
        let key = (bounds, self.w, self.h, self.cut);
        if *state != Some(key) {
            *state = Some(key);
            return Some(iced::widget::Action::request_redraw());
        }
        None
    }

    fn draw(
        &self,
        _s: &Self::State,
        renderer: &iced::Renderer,
        _t: &Theme,
        bounds: Rectangle,
        _c: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let half = self.border_width / 2.0;

        if w <= 2.0 * half || h <= 2.0 * half {
            return vec![frame.into_geometry()];
        }

        // Clamp the cut so the two corner cuts never overlap the opposite
        // sides. 0.35/0.45 matches ShapeBox::Hexagon's own clamps.
        let cut = self.cut.min(w * 0.35).min(h * 0.45);

        let path = Path::new(|b| {
            b.move_to(Point::new(cut + half, half));
            b.line_to(Point::new(w - cut - half, half));
            b.line_to(Point::new(w - half, cut + half));
            b.line_to(Point::new(w - half, h - cut - half));
            b.line_to(Point::new(w - cut - half, h - half));
            b.line_to(Point::new(cut + half, h - half));
            b.line_to(Point::new(half, h - cut - half));
            b.line_to(Point::new(half, cut + half));
            b.close();
        });

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
