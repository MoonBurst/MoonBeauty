//! Two-phase tooltip animation.
//! Phase 1 (drop):   height grows H_START → H at the bar-module slant angle.
//! Phase 2 (expand): width grows W_START → W. Reveals content toward the
//!                   screen center. Phases never overlap — the drop
//!                   completes before the expand starts.

use iced::{Element, Length};

pub const DROP_MS: f32 = 220.0;
pub const EXPAND_MS: f32 = 260.0;
pub const H_START: f32 = 8.0;

pub struct Anim { pub w: f32, pub h: f32 }

pub fn compute(elapsed_ms: f32, w_start: f32, w_end: f32, h_end: f32) -> Anim {
    let drop_t = (elapsed_ms / DROP_MS).clamp(0.0, 1.0);
    let drop_ease = 1.0 - (1.0 - drop_t).powi(3);
    let h = H_START + (h_end - H_START) * drop_ease;

    let expand_t = ((elapsed_ms - DROP_MS) / EXPAND_MS).clamp(0.0, 1.0);
    let expand_ease = 1.0 - (1.0 - expand_t).powi(3);
    let w = w_start + (w_end - w_start) * expand_ease;

    Anim { w, h }
}

pub fn anchor_align(grow_right: bool) -> iced::alignment::Horizontal {
    if grow_right { iced::alignment::Horizontal::Left }
    else { iced::alignment::Horizontal::Right }
}

pub fn clip_content<'a, Message: 'a>(
    content: Element<'a, Message>,
    anim_w: f32, anim_h: f32, grow_right: bool,
) -> Element<'a, Message> {
    let align = anchor_align(grow_right);
    let wrapped: iced::widget::Column<'a, Message> = iced::widget::Column::with_children(vec![content]).align_x(align).width(Length::Fill);
    let c: iced::widget::Container<'a, Message> = iced::widget::container(wrapped)
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
    let align = anchor_align(grow_right);
    let c: iced::widget::Container<'a, Message> = iced::widget::container(el)
        .width(Length::Fill).height(Length::Fill)
        .align_x(align).align_y(iced::alignment::Vertical::Top);
    c.into()
}
