use iced::widget::{container, row, Row, Stack};
use iced::{Alignment, Element, Length};

use super::capsules::{CapsuleAction, CapsuleRenderer};
use crate::backend::TrayItem;
use crate::backend::UnifiedTelemetry;
use crate::config::settings::ShellSettings;
use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};

#[derive(Debug, Clone)]
pub enum TopBarMessage {
    Action(CapsuleAction),
    Noop,
}

pub struct TopBar;



impl TopBar {
    pub fn view<'a>(
        settings: &ShellSettings,
        theme: &ShellTheme,
        telemetry: &'a UnifiedTelemetry,
        tray_expanded: bool,
        tray_items: &'a [TrayItem],
    ) -> Element<'a, TopBarMessage> {
        let spacing_px = settings.capsule_spacing as f32;

        let mut left_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_left {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = match settings.slant_mode.as_str() {
                    "left" => (SlantSide::Left, SlantSide::Left),
                    "right" => (SlantSide::Right, SlantSide::Right),
                    _ => (SlantSide::Left, SlantSide::Left), // Slant towards center
                };
                if id == "tray" && tray_expanded {
                    left_row = left_row.push(CapsuleRenderer::render_tray_expanded(
                        tray_items, theme, sl, sr, "left", TopBarMessage::Action,
                    ));
                } else {
                    left_row = left_row.push(CapsuleRenderer::render(
                        id,
                        telemetry,
                        theme,
                        sl,
                        sr,
                        &settings.module_shape,
                        "left",
                        TopBarMessage::Action,
                    ));
                }
            }
        }

        let mut center_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        let visible_center: Vec<&String> = settings.bar_center.iter()
            .filter(|id| settings.visible_capsules.get(*id).copied().unwrap_or(true))
            .collect();
        let keystone_idx = visible_center.len() / 2;

        for (i, id) in visible_center.iter().enumerate() {
            let (sl, sr) = match settings.slant_mode.as_str() {
                "left" => (SlantSide::Left, SlantSide::Left),
                "right" => (SlantSide::Right, SlantSide::Right),
                _ => {
                    if i == keystone_idx {
                        (SlantSide::Left, SlantSide::Right) // Keystone /\
                    } else if i < keystone_idx {
                        (SlantSide::Left, SlantSide::Left) // Left of keystone //
                    } else {
                        (SlantSide::Right, SlantSide::Right) // Right of keystone \\
                    }
                }
            };
            if id.as_str() == "tray" && tray_expanded {
                center_row = center_row.push(CapsuleRenderer::render_tray_expanded(
                    tray_items, theme, sl, sr, "center", TopBarMessage::Action,
                ));
            } else {
                center_row = center_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    sl,
                    sr,
                    &settings.module_shape,
                    "center",
                    TopBarMessage::Action,
                ));
            }
        }

        let mut right_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = match settings.slant_mode.as_str() {
                    "left" => (SlantSide::Left, SlantSide::Left),
                    "right" => (SlantSide::Right, SlantSide::Right),
                    _ => (SlantSide::Right, SlantSide::Right), // Slant towards center
                };
                if id == "tray" && tray_expanded {
                    right_row = right_row.push(CapsuleRenderer::render_tray_expanded(
                        tray_items, theme, sl, sr, "right", TopBarMessage::Action,
                    ));
                } else {
                    right_row = right_row.push(CapsuleRenderer::render(
                        id,
                        telemetry,
                        theme,
                        sl,
                        sr,
                        &settings.module_shape,
                        "right",
                        TopBarMessage::Action,
                    ));
                }
            }
        }

        let bar_shape = match settings.bar_shape.as_str() {
            "hexagon" => ShapeType::Hexagon {
                cut: (theme.slant_width * 1.5) as u16,
            },
            "rounded" => ShapeType::Rounded(((theme.slant_width * 1.5) as u16).max(4)),
            "flat" => ShapeType::Flat,
            _ => ShapeType::Slant {
                left: SlantSide::Left,
                right: SlantSide::Right,
            },
        };

        let bar_background = ShapeBox::new()
            .shape(bar_shape)
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .slant_width(theme.slant_width * 1.5);

        let content = row![
            left_row,
            row![].width(Length::Fill),
            center_row,
            row![].width(Length::Fill),
            right_row,
        ]
        .padding([4, 16])
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fixed(settings.bar_height as f32));

        container(Stack::new().push(bar_background.view()).push(content))
            .width(Length::Fill)
            .height(Length::Fixed(settings.bar_height as f32))
            .into()
    }
}
