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
        tray_anim_width: f32,
    ) -> Element<'a, TopBarMessage> {
        let spacing_px = settings.capsule_spacing as f32;

        let mut left_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_left {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = match settings.slant_mode.as_str() {
                    "left" => (SlantSide::Left, SlantSide::Left),
                    "right" => (SlantSide::Right, SlantSide::Right),
                    _ => (SlantSide::Left, SlantSide::Left),
                };
                if id == "tray" && (tray_expanded || tray_anim_width > settings.tray_width as f32 + 0.5) {
                    left_row = left_row.push(CapsuleRenderer::render_tray_expanded(
                        tray_items, theme, settings, sl, sr, "left", tray_anim_width,
                        TopBarMessage::Action,
                    ));
                } else {
                    let cap = CapsuleRenderer::render(
                        id, telemetry, theme, settings, sl, sr, &settings.module_shape, "left", TopBarMessage::Action,
                    );
                    if id == "tray" {
                        left_row = left_row.push(
                            container(cap).width(Length::Fixed(settings.tray_width as f32))
                        );
                    } else {
                        left_row = left_row.push(cap);
                    }
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
                    if i == keystone_idx { (SlantSide::Left, SlantSide::Right) }
                    else if i < keystone_idx { (SlantSide::Left, SlantSide::Left) }
                    else { (SlantSide::Right, SlantSide::Right) }
                }
            };
            if id.as_str() == "tray" && (tray_expanded || tray_anim_width > settings.tray_width as f32 + 0.5) {
                center_row = center_row.push(CapsuleRenderer::render_tray_expanded(
                    tray_items, theme, settings, sl, sr, "center", tray_anim_width,
                        TopBarMessage::Action,
                ));
            } else {
                let cap = CapsuleRenderer::render(
                    id, telemetry, theme, settings, sl, sr, &settings.module_shape, "center", TopBarMessage::Action,
                );
                if id.as_str() == "tray" {
                    center_row = center_row.push(
                        container(cap).width(Length::Fixed(settings.tray_width as f32))
                    );
                } else {
                    center_row = center_row.push(cap);
                }
            }
        }

        let mut right_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = match settings.slant_mode.as_str() {
                    "left" => (SlantSide::Left, SlantSide::Left),
                    "right" => (SlantSide::Right, SlantSide::Right),
                    _ => (SlantSide::Right, SlantSide::Right),
                };
                if id == "tray" && (tray_expanded || tray_anim_width > settings.tray_width as f32 + 0.5) {
                    right_row = right_row.push(CapsuleRenderer::render_tray_expanded(
                        tray_items, theme, settings, sl, sr, "right", tray_anim_width,
                        TopBarMessage::Action,
                    ));
                } else {
                    let cap = CapsuleRenderer::render(
                        id, telemetry, theme, settings, sl, sr, &settings.module_shape, "right", TopBarMessage::Action,
                    );
                    if id == "tray" {
                        right_row = right_row.push(
                            container(cap).width(Length::Fixed(settings.tray_width as f32))
                        );
                    } else {
                        right_row = right_row.push(cap);
                    }
                }
            }
        }

        // Summary log every time tray state changes
        {
            use std::sync::atomic::{AtomicBool, Ordering};
            static LAST_EXPANDED: AtomicBool = AtomicBool::new(false);
            let was = LAST_EXPANDED.swap(tray_expanded, Ordering::Relaxed);
            if was != tray_expanded {
                let mut total = 0.0f32;
                let mut count = 0;
                for id in settings.bar_left.iter().chain(settings.bar_center.iter()).chain(settings.bar_right.iter()) {
                    if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                        if let Some(w) = super::capsules::lookup_width(id) {
                            total += w;
                            count += 1;
                        }
                    }
                }
                eprintln!("[bar-layout] {} visible capsules, sum={:.1}px", count, total);
            }
        }

        let bar_shape = match settings.bar_shape.as_str() {
            "hexagon" => ShapeType::Hexagon { cut: (theme.slant_width * 1.5) as u16 },
            "rounded" => ShapeType::Rounded(((theme.slant_width * 1.5) as u16).max(4)),
            "flat" => ShapeType::Flat,
            _ => ShapeType::Slant { left: SlantSide::Left, right: SlantSide::Right },
        };

        let bar_background = ShapeBox::new()
            .shape(bar_shape)
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .slant_width(theme.slant_width * 1.5);

        let content = row![
            left_row,
            iced::widget::horizontal_space(),
            center_row,
            iced::widget::horizontal_space(),
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
