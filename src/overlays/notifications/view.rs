//! Notification toast deck and history drawer rendering.

use iced::widget::{button, column, container, pin, row, scrollable, text, MouseArea, Stack};
use iced::{Alignment, Element, Length, Padding};

use super::ui::{NotifMessage, NotificationOverlay, NotificationToast};
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::{hex_to_color, ShellTheme};
use crate::libraries::style::shape_box::{AngledHexGeometry, ShapeBox, ShapeType, SlantSide};
use crate::libraries::style::TextExt;

// ── History row geometry (single source of truth) ─────────────────
// Used by both render_history() and scroll_offset_for_selected() so
// scroll always lands on the correct row regardless of text length.
// Icon size is dynamic — passed in from settings.launcher_icon_size
// so the history icons match the launcher.
pub(super) const HISTORY_ROW_PAD_X: f32 = 20.0;
pub(super) const HISTORY_ROW_PAD_Y: f32 = 12.0;
pub(super) const HISTORY_ROW_GAP: f32 = 10.0;
pub(super) const HISTORY_ROW_MIN_H: f32 = 72.0;

/// Hexagon wrap size around an icon. Must satisfy:
///   interior = hex_size - 2*cut ≥ icon_size
///   cut = 10% of hex_size  →  interior = 0.80 * hex_size
///   hex_size ≥ 1.25 * icon_size.
/// Use 1.40 for visible margin.
pub fn history_icon_hex_size(icon_size: f32) -> f32 {
    icon_size * 1.40
}

/// Compute the (shape, outer box size) for the toast icon wrapper.
/// `notif_shape` is the same field used for the notification card, so
/// the icon visually follows the card it lives in. `icon_size` is the
/// user-configured inner icon size (settings.notif_icon_size).
///
/// Hex uses at least 1.25× so octagon corner cuts do not clip the icon.
pub fn notif_icon_box_shape(notif_shape: &str, icon_size: f32) -> (ShapeType, f32) {
    let s = icon_size.clamp(16.0, 128.0);
    match notif_shape.to_lowercase().as_str() {
        "hex" | "hexagon" => {
            // 1.25 is the minimum to keep square icon corners clear of the
            // octagon cuts; 1.28 leaves a small margin for border & AA.
            let hex_size = s * 1.28;
            let min_cut = ((hex_size - s) as u16).saturating_add(1);
            let cut = ((hex_size * 0.25) as u16).max(min_cut).max(6);
            (ShapeType::Hexagon { cut }, hex_size)
        }
        "slant" => (
            ShapeType::Slant {
                left: SlantSide::Right,
                right: SlantSide::Right,
            },
            s + 12.0,
        ),
        "flat" => (ShapeType::Flat, s + 4.0),
        _ => {
            let radius = ((s * 0.15) as u16).max(4);
            (ShapeType::Rounded(radius), s + 8.0)
        }
    }
}

pub fn history_row_height(
    toast: &NotificationToast,
    row_w: f32,
    fs: f32,
    fs_sm: f32,
    icon_size: f32,
) -> f32 {
    let icon_hex_size = history_icon_hex_size(icon_size);
    let text_area_w = (row_w - icon_hex_size - 16.0 - HISTORY_ROW_PAD_X * 2.0).max(100.0);
    let char_w = (fs_sm * 0.55).max(6.0);
    let chars_per_line = (text_area_w / char_w).max(20.0) as usize;
    let body_len = toast.body.chars().count();
    let body_lines = body_len.div_ceil(chars_per_line).max(1);
    let header_h = fs * 1.2;
    let line_h = fs_sm * 1.2;
    let text_h =
        HISTORY_ROW_PAD_Y + header_h + 4.0 + (body_lines as f32) * line_h + HISTORY_ROW_PAD_Y;
    // Row must fit the icon hexagon + padding so it does not overflow.
    let icon_h = icon_hex_size + HISTORY_ROW_PAD_Y * 2.0;
    text_h.max(HISTORY_ROW_MIN_H).max(icon_h)
}

pub fn render_toasts<'a>(
    ov: &'a NotificationOverlay,
    theme: &'a ShellTheme,
    settings: &'a ShellSettings,
) -> Element<'a, NotifMessage> {
    let card_w = settings.notif_card_width as f32;
    let card_h = settings.notif_card_height as f32;

    let shape = match settings.notif_shape.to_lowercase().as_str() {
        "hex" | "hexagon" => ShapeType::Hexagon { cut: 14 },
        "slant" => ShapeType::Slant {
            left: SlantSide::Right,
            right: SlantSide::Right,
        },
        "flat" => ShapeType::Flat,
        _ => ShapeType::Rounded(10),
    };

    let from_right = settings.notif_corner.contains("right");

    let mut deck = Stack::new();
    let ordered: Vec<(usize, &NotificationToast)> = ov.active_toasts.iter().enumerate().collect();

    let strip_w =
        settings.notif_card_width as f32 + 2.0 * settings.notif_margin_x as f32 + 100.0;
    let strip_margin = settings.notif_margin_x as f32;

    // Iterate in reverse: newest pushed first (bottom z-order, tucked
    // behind/above), oldest pushed last (top z-order, front card in view).
    // As the oldest toast expires or is dismissed, newer toasts drop down
    // into the front slot.
    for (i, toast) in ordered.into_iter().rev() {
        let id = toast.id;
        let border_c = hex_to_color(&toast.border_color);

        let elapsed_ms = toast.created_at.elapsed().as_millis() as f32;
        let slide_ms: f32 = if settings.notif_slide_in_ms == 0 {
            250.0
        } else {
            settings.notif_slide_in_ms.max(50) as f32
        };
        let pause_ms = settings.notif_drop_pause_ms as f32;
        let drop_ms = 250.0_f32;
        let drop_dist = settings.notif_drop_distance_px as f32;
        let exit_ms = 300.0_f32;

        // Phase 1: Slide in horizontally from offscreen
        let slide_progress = (elapsed_ms / slide_ms).clamp(0.0, 1.0);
        let slide_ease = 1.0 - (1.0 - slide_progress).powi(3);

        // Phase 3: Slide away horizontally to the right on expiration / dismissal
        let exit_progress = toast
            .exit_started_at
            .map(|s| (s.elapsed().as_millis() as f32 / exit_ms).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let exit_ease = exit_progress * exit_progress * exit_progress;

        // Compute resting and offscreen coordinates without compressing the card width
        let (resting_x, offscreen_x) = if from_right {
            (strip_w - card_w - strip_margin, strip_w)
        } else {
            (strip_margin, -card_w)
        };

        // Interpolate position: offscreen -> resting -> offscreen (on exit)
        let enter_x = offscreen_x + (resting_x - offscreen_x) * slide_ease;
        let x_pos = enter_x + (offscreen_x - resting_x) * exit_ease;

        // Phase 2: Vertical drop down (starts after horizontal entry + pause)
        let drop_start = slide_ms + pause_ms;
        let drop_progress = ((elapsed_ms - drop_start) / drop_ms).clamp(0.0, 1.0);
        let drop_ease = 1.0 - (1.0 - drop_progress).powi(3);
        let drop_extra = (1.0 - drop_ease) * (-drop_dist);

        let peek = (settings.notif_stack_overlap as f32).abs().max(4.0);

        let shift_progress = toast
            .slot_shift_start
            .map(|s| (s.elapsed().as_millis() as f32 / 200.0).min(1.0))
            .unwrap_or(1.0);
        let shift_ease = 1.0 - (1.0 - shift_progress).powi(3);
        let shift_offset = (1.0 - shift_ease) * (-peek);
        let fixed_base = drop_dist + 4.0 * peek;
        let slot_y = fixed_base - (i as f32) * peek + shift_offset;
        let y_pos = (slot_y + drop_extra).max(0.0);

        let card_bg = ShapeBox::new()
            .shape(shape)
            .background(theme.background)
            .border(border_c, theme.border_width)
            .size(Length::Fixed(card_w), Length::Fixed(card_h));

        let toast_text_color = hex_to_color(&toast.text_color);
        let avatar: Element<'_, NotifMessage> = if let Some(ref path) = toast.avatar_path {
            let icon_size = (settings.notif_icon_size as f32).clamp(16.0, 128.0);
            let (box_shape, box_size) = notif_icon_box_shape(&settings.notif_shape, icon_size);
            let icon_element: Element<'_, NotifMessage> = if path.ends_with(".svg") {
                iced::widget::svg(iced::widget::svg::Handle::from_path(path.clone()))
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .into()
            } else {
                iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .content_fit(iced::ContentFit::Cover)
                    .into()
            };
            let mut shape_box = ShapeBox::new()
                .shape(box_shape)
                .background(theme.background)
                .border(theme.border, theme.control_border_width)
                .size(Length::Fixed(box_size), Length::Fixed(box_size));
            if matches!(box_shape, ShapeType::Slant { .. }) {
                shape_box = shape_box.slant_width(icon_size * 0.15);
            }
            Stack::new()
                .push(shape_box.view())
                .push(
                    container(icon_element)
                        .width(Length::Fixed(box_size))
                        .height(Length::Fixed(box_size))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .clip(true),
                )
                .into()
        } else {
            container(text("")).width(Length::Fixed(0.0)).into()
        };
        // Full-body activate area. Padding lives inside the MouseArea
        // (via the container), and both container and inner row are Fill height.
        let custom_actions: Vec<(&str, &str)> = toast
            .actions
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|pair| pair[0] != "default" && !pair[1].is_empty())
            .map(|pair| (pair[0].as_str(), pair[1].as_str()))
            .collect();
        let has_custom_actions = !custom_actions.is_empty();

        let toast_body = MouseArea::new(
            container(
                row![avatar, {
                    let summary_row = if toast.app_name.is_empty() {
                        column![text(&toast.summary)
                            .size((settings.notif_summary_size) as f32)
                            .bold()
                            .color(toast_text_color)
                            .width(Length::Fill)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph)]
                    } else {
                        column![
                            text(&toast.summary)
                                .size((settings.notif_summary_size) as f32)
                                .bold()
                                .color(toast_text_color)
                                .width(Length::Fill)
                                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                            text(format!("via {}", toast.app_name))
                                .size((settings.notif_summary_size.saturating_sub(2)) as f32)
                                .color(theme.focus)
                                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                        ]
                    };
                    let text_col = column![
                        summary_row.width(Length::Fill),
                        text(&toast.body)
                            .size((settings.notif_body_size) as f32)
                            .color(toast_text_color)
                            .width(Length::Fill)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    ];
                    text_col.width(Length::Fill).spacing(2)
                },]
                .spacing(12)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .padding(if has_custom_actions {
                Padding {
                    top: 10.0,
                    right: 14.0,
                    bottom: 4.0,
                    left: 14.0,
                }
            } else {
                Padding {
                    top: 14.0,
                    right: 14.0,
                    bottom: 14.0,
                    left: 14.0,
                }
            })
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .on_press(NotifMessage::ActivateToast(id));

        let left_col: Element<'_, NotifMessage> = if has_custom_actions {
            let mut actions_row = row![].spacing(6).align_y(Alignment::Center);
            let act_fs = (settings.notif_body_size.saturating_sub(4)).clamp(10, 14) as f32;
            for (action_key, action_label) in custom_actions.into_iter().take(4) {
                let key_owned = action_key.to_string();
                let label_owned = action_label.to_string();
                let bg_col = theme.surface_alt;
                let border_col = border_c;
                let fg_col = toast_text_color;
                let act_btn = button(
                    text(label_owned)
                        .size(act_fs)
                        .bold()
                        .wrapping(iced::widget::text::Wrapping::None),
                )
                .on_press(NotifMessage::InvokeToastAction(id, key_owned))
                .padding([3, 10])
                .style(move |_t, status| {
                    let (bg, tc) = match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            (border_col, iced::Color::BLACK)
                        }
                        _ => (bg_col, fg_col),
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        border: iced::Border {
                            color: border_col,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        text_color: tc,
                        ..Default::default()
                    }
                });
                actions_row = actions_row.push(act_btn);
            }
            column![
                toast_body,
                container(actions_row).padding(Padding {
                    top: 0.0,
                    right: 14.0,
                    bottom: 10.0,
                    left: 14.0,
                }),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            toast_body.into()
        };

        // Full-height close column.
        let close_btn = button(text("✕").size(12).color(theme.muted))
            .on_press(NotifMessage::DismissToast(id))
            .padding(0)
            .width(Length::Fixed(48.0))
            .height(Length::Fill)
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

        let content = row![left_col, close_btn]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill);

        let toast_card = container(Stack::new().push(card_bg.view()).push(content))
            .width(Length::Fixed(card_w))
            .height(Length::Fixed(card_h));

        // Pin widget directly positions the card without applying flex compression
        deck = deck.push(pin(toast_card).x(x_pos).y(y_pos));
    }

    // Clip container to ensure the card cleanly enters and exits across the strip boundary
    let clipped_strip = container(deck)
        .width(Length::Fixed(strip_w))
        .height(Length::Fill)
        .clip(true);

    container(clipped_strip)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(if from_right {
            iced::alignment::Horizontal::Right
        } else {
            iced::alignment::Horizontal::Left
        })
        .into()
}

pub fn render_history<'a>(
    ov: &'a NotificationOverlay,
    theme: &'a ShellTheme,
    settings: &ShellSettings,
) -> Element<'a, NotifMessage> {
    let (win_w, win_h) = settings.get_window_size("history", 960, 720);

    let cut_f = theme.overlay_cut;
    let angle_deg = theme.overlay_angle;
    let pad_top = 20.0_f32;
    let pad_bottom = 20.0_f32;
    let (header_inset, body_inset, footer_inset) =
        AngledHexGeometry::new(win_w, win_h, cut_f, angle_deg).standard_insets();

    let fs = theme.font_size_overlay as f32;
    let fs_sm = fs * 0.85;
    let gap = 10.0;
    let row_w = win_w - body_inset * 2.0;

    let mut list_col = column![].spacing(gap);

    if ov.history.is_empty() {
        list_col = list_col.push(
            container(
                text("No notifications in history.")
                    .size(fs)
                    .color(theme.muted),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        );
    } else {
        // Render in reverse so newest is at the bottom.
        // Compute approximate row height from body text so ShapeBox
        // and content are both fixed at the same size (Fill+Shrink
        // collapses to 0 in iced 0.14).
        for toast in ov.history.iter().rev() {
            let id = toast.id;
            let log_now = ov.should_log_icon(id);
            let is_sel = ov.selected_history_id == Some(id);

            // Icon size matches the launcher slider setting.
            // Hexagon sized large enough that its cut corners can't
            // clip the icon's corners (see history_icon_hex_size).
            let icon_size = settings.launcher_icon_size as f32;
            let icon_hex_size = history_icon_hex_size(icon_size);
            let hist_min_cut = ((icon_hex_size - icon_size) as u16).saturating_add(1);
            let icon_hex_cut: u16 = ((icon_hex_size * 0.25) as u16).max(hist_min_cut).max(8);

            let inner_icon: Element<'a, NotifMessage> = if let Some(ref path) = toast.avatar_path {
                if std::path::Path::new(path).exists() {
                    crate::dbg_log!("[notif-render] id={} icon=OK {}", id, path);
                    if path.ends_with(".svg") {
                        iced::widget::svg(iced::widget::svg::Handle::from_path(path.clone()))
                            .width(Length::Fixed(icon_size))
                            .height(Length::Fixed(icon_size))
                            .into()
                    } else {
                        iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                            .width(Length::Fixed(icon_size))
                            .height(Length::Fixed(icon_size))
                            .content_fit(iced::ContentFit::Contain)
                            .into()
                    }
                } else {
                    if log_now {
                        crate::dbg_log!("[notif-render] id={} icon=MISSING {}", id, path);
                    }
                    container(text("🔔").size(icon_size * 0.7))
                        .width(Length::Fixed(icon_size))
                        .height(Length::Fixed(icon_size))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .into()
                }
            } else {
                if log_now {
                    crate::dbg_log!("[notif-render] id={} icon=NONE", id);
                }
                container(text("🔔").size(icon_size * 0.7))
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            };

            let icon_hex_bg = ShapeBox::new()
                .shape(ShapeType::Hexagon { cut: icon_hex_cut })
                .background(theme.background)
                .border(theme.border, theme.control_border_width)
                .size(Length::Fixed(icon_hex_size), Length::Fixed(icon_hex_size));

            let icon_clipped = container(inner_icon)
                .width(Length::Fixed(icon_size))
                .height(Length::Fixed(icon_size))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .clip(true);

            let icon_elem: Element<'a, NotifMessage> = Stack::new()
                .push(icon_hex_bg.view())
                .push(
                    container(icon_clipped)
                        .width(Length::Fixed(icon_hex_size))
                        .height(Length::Fixed(icon_hex_size))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .into();

            let time_str = toast.timestamp.clone();
            let app_str = toast.app_name.clone();
            let summary = toast.summary.clone();
            let body = toast.body.clone();

            // Delegate to shared helper — must match scroll_offset_for_selected.
            let row_h = history_row_height(toast, row_w, fs, fs_sm, icon_size);

            let border_col = if is_sel { theme.success } else { theme.border };
            let border_w = if is_sel {
                theme.control_border_width * 2.0
            } else {
                theme.control_border_width
            };

            let row_bg = ShapeBox::new()
                .shape(ShapeType::Hexagon { cut: 16 })
                .background(if is_sel {
                    theme.surface_alt
                } else {
                    theme.background
                })
                .border(border_col, border_w)
                .size(Length::Fixed(row_w), Length::Fixed(row_h));

            let text_col = column![
                row![
                    text(summary).size(fs).bold().color(theme.text),
                    iced::widget::space::horizontal(),
                    text(format!("{} • {}", app_str, time_str))
                        .size(fs_sm)
                        .color(theme.muted),
                ]
                .align_y(iced::alignment::Vertical::Center),
                text(body)
                    .size(fs_sm)
                    .color(theme.text)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            ]
            .spacing(4)
            .width(Length::Fill);

            let inner_row = row![icon_elem, container(text_col).width(Length::Fill),]
                .spacing(16)
                .align_y(iced::alignment::Vertical::Top);

            let row_content = container(inner_row)
                .width(Length::Fixed(row_w))
                .height(Length::Fixed(row_h))
                .padding(iced::Padding {
                    top: 12.0,
                    right: 20.0,
                    bottom: 12.0,
                    left: 20.0,
                })
                .align_y(iced::alignment::Vertical::Top);

            let stack = Stack::new().push(row_bg.view()).push(row_content);

            let btn = button(stack)
                .on_press(NotifMessage::HistoryDeleteSelected)
                .padding(0)
                .width(Length::Fixed(row_w))
                .height(Length::Fixed(row_h))
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });

            list_col = list_col.push(btn);
        }
    }

    let pos_label = if let Some(id) = ov.selected_history_id {
        if let Some(i) = ov.history.iter().position(|t| t.id == id) {
            // Render order is reversed: position in list (1-indexed, top-down)
            // = history.len() - i
            format!("{} / {}", ov.history.len() - i, ov.history.len())
        } else {
            format!("{} items", ov.history.len())
        }
    } else if !ov.history.is_empty() {
        format!("{} items", ov.history.len())
    } else {
        String::new()
    };

    let header_row = container(
        row![
            text("📜 NOTIFICATION HISTORY")
                .size(fs)
                .bold()
                .color(theme.text),
            iced::widget::space::horizontal(),
            text(pos_label).size(fs_sm).bold().color(theme.muted),
            button(text("Clear All").size(fs_sm).bold().color(theme.danger))
                .on_press(NotifMessage::ClearHistory)
                .padding([6, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.background)),
                    border: iced::Border {
                        color: theme.danger,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                }),
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: 0.0,
        right: header_inset,
        bottom: 0.0,
        left: header_inset,
    });

    let list_container = container(
        scrollable(list_col)
            .id(iced::widget::Id::new("history_list"))
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(Padding {
        top: 0.0,
        right: body_inset,
        bottom: 0.0,
        left: body_inset,
    });

    let footer_row = container(
        text("↑/↓ Navigate  •  Enter to jump  •  Delete to remove  •  ESC to dismiss")
            .size(fs_sm)
            .color(theme.muted)
            .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding {
        top: 0.0,
        right: footer_inset,
        bottom: 0.0,
        left: footer_inset,
    });

    let card_bg = ShapeBox::new()
        .shape(ShapeType::AngledHexagon {
            cut: cut_f,
            angle_deg,
        })
        .background(theme.background)
        .border(theme.border, theme.border_width)
        .size(Length::Fixed(win_w), Length::Fixed(win_h));

    let card_content = container(
        column![header_row, list_container, footer_row]
            .spacing(14)
            .padding(Padding {
                top: pad_top,
                right: 0.0,
                bottom: pad_bottom,
                left: 0.0,
            }),
    )
    .width(Length::Fixed(win_w))
    .height(Length::Fixed(win_h))
    .style(|_t| iced::widget::container::Style::default());

    let card = Stack::new().push(card_bg.view()).push(card_content);

    container(card)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_t| iced::widget::container::Style::default())
        .into()
}
