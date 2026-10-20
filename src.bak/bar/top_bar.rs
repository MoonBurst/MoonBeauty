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
                left_row = left_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    SlantSide::Left,
                    SlantSide::Left,
                    TopBarMessage::Action,
                ));
            }
        }

        let mut center_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_center {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                center_row = center_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    SlantSide::Left,
                    SlantSide::Right,
                    TopBarMessage::Action,
                ));
            }
        }

        let mut right_row = Row::new().spacing(spacing_px).align_y(Alignment::Center);
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                right_row = right_row.push(CapsuleRenderer::render(
                    id,
                    telemetry,
                    theme,
                    SlantSide::Right,
                    SlantSide::Right,
                    TopBarMessage::Action,
                ));
            }
        }

        let bar_background = ShapeBox::new()
            .shape(ShapeType::Slant {
                left: SlantSide::Left,
                right: SlantSide::Right,
            })
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
