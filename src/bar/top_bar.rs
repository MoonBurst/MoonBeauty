use iced::widget::{container, row, Row, Stack};
use iced::{Alignment, Element, Length};

use super::capsules::{CapsuleAction, CapsuleRenderer};
use crate::bar::tray::TrayItem;
use crate::libraries::backend::UnifiedTelemetry;
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};

#[derive(Debug, Clone)]
pub enum TopBarMessage {
    Action(CapsuleAction),
    Noop,
}

pub struct TopBar;

#[allow(clippy::too_many_arguments)]
fn render_capsule_slot<'a>(
    id: &str,
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
    settings: &ShellSettings,
    sl: SlantSide,
    sr: SlantSide,
    region: &str,
    tray_expanded: bool,
    tray_items: &'a [TrayItem],
    tray_anim_width: f32,
) -> Element<'a, TopBarMessage> {
    if id == "tray" && (tray_expanded || tray_anim_width > settings.tray_width as f32 + 0.5) {
        CapsuleRenderer::render_tray_expanded(
            tray_items, theme, settings, sl, sr, region, tray_anim_width, TopBarMessage::Action,
        )
    } else {
        let cap = CapsuleRenderer::render(
            id, telemetry, theme, settings, sl, sr, &settings.module_shape, region, TopBarMessage::Action,
        );
        if id == "tray" {
            container(cap).width(Length::Fixed(settings.tray_width as f32)).into()
        } else {
            cap
        }
    }
}

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
                left_row = left_row.push(render_capsule_slot(id, telemetry, theme, settings, sl, sr, "left", tray_expanded, tray_items, tray_anim_width));
            }
        }

        let mut center_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        let visible_center: Vec<&String> = settings
            .bar_center
            .iter()
            .filter(|id| settings.visible_capsules.get(*id).copied().unwrap_or(true))
            .collect();
        let keystone_idx = visible_center.len() / 2;

        for (i, id) in visible_center.iter().enumerate() {
            let (sl, sr) = match settings.slant_mode.as_str() {
                "left" => (SlantSide::Left, SlantSide::Left),
                "right" => (SlantSide::Right, SlantSide::Right),
                _ => {
                    if i == keystone_idx {
                        (SlantSide::Left, SlantSide::Right)
                    } else if i < keystone_idx {
                        (SlantSide::Left, SlantSide::Left)
                    } else {
                        (SlantSide::Right, SlantSide::Right)
                    }
                }
            };
            center_row = center_row.push(render_capsule_slot(id, telemetry, theme, settings, sl, sr, "center", tray_expanded, tray_items, tray_anim_width));
        }

        let mut right_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = match settings.slant_mode.as_str() {
                    "left" => (SlantSide::Left, SlantSide::Left),
                    "right" => (SlantSide::Right, SlantSide::Right),
                    _ => (SlantSide::Right, SlantSide::Right),
                };
                right_row = right_row.push(render_capsule_slot(id, telemetry, theme, settings, sl, sr, "right", tray_expanded, tray_items, tray_anim_width));
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
                for id in settings
                    .bar_left
                    .iter()
                    .chain(settings.bar_center.iter())
                    .chain(settings.bar_right.iter())
                {
                    if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                        if let Some(w) = super::capsules::lookup_width(id) {
                            total += w;
                            count += 1;
                        }
                    }
                }
                eprintln!(
                    "[bar-layout] {} visible capsules, sum={:.1}px",
                    count, total
                );
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
            .slant_width(theme.slant_width * 1.5)
            .size(Length::Fill, Length::Fixed(settings.bar_height as f32));

        let content = row![
            left_row,
            iced::widget::space::horizontal(),
            center_row,
            iced::widget::space::horizontal(),
            right_row,
        ]
        .padding([4, 24])
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fixed(settings.bar_height as f32));

        container(Stack::new().push(bar_background.view()).push(content))
            .width(Length::Fill)
            .height(Length::Fixed(settings.bar_height as f32))
            .into()
    }
}
