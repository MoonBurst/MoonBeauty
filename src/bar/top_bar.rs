use iced::widget::{container, row, Row, Stack};
use iced::{Alignment, Element, Length};

use super::capsules::{CapsuleAction, CapsuleRenderer};
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

fn slant_for(settings: &ShellSettings, id: &str, section: &str) -> (SlantSide, SlantSide) {
    if let Some(custom) = settings.capsule_slants.get(id) {
        match custom.as_str() {
            "left" => return (SlantSide::Left, SlantSide::Left),
            "right" => return (SlantSide::Right, SlantSide::Right),
            "center" => return (SlantSide::Left, SlantSide::Right),
            _ => {}
        }
    }
    match settings.slant_mode.as_str() {
        "all-left" => (SlantSide::Left, SlantSide::Left),
        "all-right" => (SlantSide::Right, SlantSide::Right),
        _ => match section {
            "left" => (SlantSide::Left, SlantSide::Left),
            "right" => (SlantSide::Right, SlantSide::Right),
            "center" => (SlantSide::Left, SlantSide::Right),
            _ => (SlantSide::Left, SlantSide::Left),
        },
    }
}

impl TopBar {
    pub fn view<'a>(
        settings: &ShellSettings,
        theme: &ShellTheme,
        telemetry: &'a UnifiedTelemetry,
    ) -> Element<'a, TopBarMessage> {
        let spacing_px = settings.capsule_spacing as f32;

        let mut left_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_left {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = slant_for(settings, id, "left");
                left_row = left_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    sl,
                    sr,
                    TopBarMessage::Action,
                ));
            }
        }

        let mut center_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_center {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = slant_for(settings, id, "center");
                center_row = center_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    sl,
                    sr,
                    TopBarMessage::Action,
                ));
            }
        }

        let mut right_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                let (sl, sr) = slant_for(settings, id, "right");
                right_row = right_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    sl,
                    sr,
                    TopBarMessage::Action,
                ));
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
