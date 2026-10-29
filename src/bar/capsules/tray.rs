use iced::widget::{button, container, text, tooltip, MouseArea, Row, Stack};
use iced::{Alignment, Color, Element, Length};
use std::sync::{Mutex, OnceLock};

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    theme: &ShellTheme,
    region: &str,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let arrow = match region {
        "left" => "\u{25B6}",
        "right" => "\u{25C0}",
        _ => "\u{25BC}",
    };
    let elem = label::<Message>(&[(arrow.to_string(), theme.text)], theme.font_size_normal);
    (elem, CapsuleAction::ToggleTrayExpanded, theme.text)
}

/// Tray bias — the arrow (▼/◀/▶) is a single character centered
/// inside a `settings.tray_width`-wide pill. The default +32 global
/// offset would place the tooltip's left edge 32px from the pill's
/// container left. But the arrow's visual center is at
/// `pill_width/2`, so the tooltip appears shifted right by
/// `pill_width/2 - 32`. For typical `tray_width = 60`, that's a
/// +~-2 drift — subtle but every tray tooltip accumulates it.
///
/// The bias pulls the tooltip inward on the grow_right side so its
/// left edge aligns with the arrow's left edge, and pulls inward on
/// the grow_left side for the same reason (arrow's right edge).
pub(super) fn tooltip_bias(theme: &ShellTheme) -> (i32, i32) {
    // Arrow glyph width ~ theme.font_size_normal * 0.6 (ASCII-ish).
    let arrow_w = theme.font_size_normal as f32 * 0.6;
    // Default user offset (32) already shifts 32px inward.
    // Actual pill padding + slant ≈ (slant + 12). Difference is what
    // we need to correct. Rounded to i32 for the caller.
    let pad = theme.slant_width + 12.0;
    let bias = (pad - arrow_w / 2.0 - 8.0).round() as i32;
    (bias, bias)
}

static SVG_HANDLE_CACHE: OnceLock<
    Mutex<std::collections::HashMap<String, iced::widget::svg::Handle>>,
> = OnceLock::new();

fn cached_svg_handle(path: &str) -> iced::widget::svg::Handle {
    let cache = SVG_HANDLE_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut g = cache.lock().unwrap();
    if let Some(h) = g.get(path) {
        return h.clone();
    }
    let h = iced::widget::svg::Handle::from_path(path);
    g.insert(path.to_string(), h.clone());
    h
}

pub(super) fn render_expanded<'a, Message: 'static + Clone>(
    items: &[crate::bar::tray::TrayItem],
    theme: &ShellTheme,
    settings: &ShellSettings,
    slant_left: SlantSide,
    slant_right: SlantSide,
    region: &str,
    display_width: f32,
    on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
) -> Element<'a, Message> {
    let icon_size = settings.tray_icon_size as f32;
    let icon_pad = 4.0_f32;
    let icon_slot = icon_size + icon_pad * 2.0;
    let close_slot = 24.0_f32;
    let spacing = 4.0_f32;

    let shown = items.len().min(8);
    let empty_w = if items.is_empty() { 150.0 } else { 0.0 };
    let content_w = close_slot
        + if shown > 0 { spacing } else { 0.0 }
        + (icon_slot + spacing) * shown as f32
        + empty_w;
    let _full_min_width = content_w + theme.slant_width * 2.0 + 32.0;
    let bounding_w = display_width + theme.slant_width * 2.0;
    let show_icons = display_width > settings.tray_width as f32 + 3.0;

    let hover_bg = theme.surface_alt;
    let fg = theme.text;

    let close_arrow = match region {
        "left" => "◀",
        "right" => "▶",
        _ => "▲",
    };
    let close_btn = button(text(close_arrow).size(16).color(theme.text))
        .on_press(on_action(CapsuleAction::ToggleTrayExpanded))
        .padding(0)
        .width(Length::Fixed(close_slot))
        .height(Length::Fixed(close_slot))
        .style(move |_t, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(iced::Background::Color(hover_bg))
                }
                _ => None,
            };
            button::Style {
                background: bg,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                text_color: fg,
                ..Default::default()
            }
        });

    let mut icons_row = Row::new().spacing(spacing).align_y(Alignment::Center);
    for item in items.iter().take(shown) {
        let icon_elem: Element<'a, Message> = if let Some(p) = &item.icon_path {
            let exists = std::path::Path::new(p).exists();
            if exists && p.ends_with(".svg") {
                iced::widget::svg(cached_svg_handle(p))
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .into()
            } else if exists {
                iced::widget::image(iced::widget::image::Handle::from_path(p.clone()))
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .into()
            } else {
                let initial = item.title.chars().next()
                    .or_else(|| item.id.chars().next())
                    .or_else(|| item.tooltip.chars().next())
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                text(initial).size(icon_size * 0.7).color(theme.text).into()
            }
        } else {
            let initial = item.title.chars().next()
                .or_else(|| item.id.chars().next())
                .or_else(|| item.tooltip.chars().next())
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            text(initial).size(icon_size * 0.7).color(theme.text).into()
        };

        let activate_item = item.clone();
        let ctx_item = item.clone();

        let icon_box = container(icon_elem)
            .padding(icon_pad)
            .center_x(Length::Fixed(icon_slot))
            .center_y(Length::Fixed(icon_slot));

        let clickable = MouseArea::new(icon_box)
            .on_press(on_action(CapsuleAction::TrayActivate(activate_item)))
            .on_right_press(on_action(CapsuleAction::TrayContextMenu(ctx_item)));

        let interactive: Element<'a, Message> = if !item.tooltip.is_empty() {
            let tt_text = item.tooltip.clone();
            let tt_bg = theme.surface_alt;
            let tt_fg = theme.text;
            let tt_border = theme.border;
            tooltip(
                clickable,
                container(text(tt_text).size(11).color(tt_fg))
                    .padding(6)
                    .style(move |_t| container::Style {
                        background: Some(iced::Background::Color(tt_bg)),
                        border: iced::Border {
                            color: tt_border,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
                tooltip::Position::Bottom,
            )
            .into()
        } else {
            clickable.into()
        };

        icons_row = icons_row.push(interactive);
    }

    let pad_x = theme.slant_width.round() as u16;
    let mut content = if region == "right" {
        let mut r = Row::new().spacing(spacing).align_y(Alignment::Center);
        if show_icons {
            r = r.push(icons_row);
        }
        r = r.push(close_btn);
        r
    } else {
        let mut r = Row::new().spacing(spacing).align_y(Alignment::Center);
        r = r.push(close_btn);
        if show_icons {
            r = r.push(icons_row);
        }
        r
    }
    .padding([0, pad_x]);
    if items.is_empty() {
        content = content.push(
            container(text("Empty").size(11).color(theme.muted))
                .width(Length::Fixed(bounding_w - pad_x as f32 * 2.0))
                .center_x(Length::Fill)
                .clip(true),
        );
    }

    let shape_type = ShapeType::Slant { left: slant_left, right: slant_right };

    let background_shape = ShapeBox::new()
        .shape(shape_type)
        .background(theme.background)
        .border(theme.text, theme.control_border_width)
        .slant_width(theme.slant_width)
        .size(Length::Fixed(bounding_w), Length::Fill);

    container(
        Stack::new().push(background_shape.view()).push(
            container(content)
                .width(Length::Fixed(bounding_w))
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        ),
    )
    .width(Length::Fixed(bounding_w))
    .clip(true)
    .into()
}
