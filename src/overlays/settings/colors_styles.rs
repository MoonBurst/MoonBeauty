use super::*;
use iced::widget::column;

impl SettingsOverlay {
    pub(super) fn view_colors_styles<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let slots = [
            (
                "base00",
                "Primary Background (base00)",
                &settings.palette.base00,
            ),
            (
                "base01",
                "Surface Alternate (base01)",
                &settings.palette.base01,
            ),
            (
                "base02",
                "Selection / Row Highlight (base02)",
                &settings.palette.base02,
            ),
            (
                "base03",
                "Outer Bar & Card Border (base03)",
                &settings.palette.base03,
            ),
            (
                "base05",
                "Primary Accent / Text (base05)",
                &settings.palette.base05,
            ),
            (
                "base08",
                "Danger / Alert Red (base08)",
                &settings.palette.base08,
            ),
            (
                "base09",
                "Warning / Orange (base09)",
                &settings.palette.base09,
            ),
            (
                "base0c",
                "Success / Green (base0C)",
                &settings.palette.base0c,
            ),
            (
                "base0d",
                "Focus / Blue Accent (base0D)",
                &settings.palette.base0d,
            ),
        ];

        let mut col = column![
            text("🎨 PALETTE & SYSTEM ACCENT COLORS")
                .size((scale_sz(0.95)) as f32)
                .bold()
                .color(theme.success),
            text("Click on color swatches to apply presets, or type any valid hex color:")
                .size((scale_sz(0.72)) as f32)
                .color(theme.muted),
        ]
        .spacing(10);

        let preset_colors = [
            "#0f0f0f", "#003399", "#f7f700", "#ff5555", "#04f100", "#675ddb", "#fabd2f", "#38fedc",
            "#ffffff",
        ];

        for (slot, name, current_hex) in slots {
            let slot_clone = slot.to_string();
            let swatch_col = hex_to_color(current_hex);

            let swatch_box = container(text(""))
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(swatch_col)),
                    border: iced::Border {
                        color: Color::WHITE,
                        width: 1.5,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });

            let input = text_input("#ffffff", current_hex)
                .on_input(move |h| SettingsMessage::ColorChanged(slot_clone.clone(), h))
                .padding(5)
                .size((scale_sz(0.72)) as f32)
                .width(Length::Fixed(110.0));

            let mut swatches_row = row![swatch_box, input]
                .spacing(8)
                .align_y(Alignment::Center);

            for p_hex in preset_colors {
                let p_val = hex_to_color(p_hex);
                let p_str = p_hex.to_string();
                let s_clone = slot.to_string();
                let p_btn = button(
                    container(text(""))
                        .width(Length::Fixed(14.0))
                        .height(Length::Fixed(14.0))
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(p_val)),
                            border: iced::Border {
                                radius: 3.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                )
                .on_press(SettingsMessage::ColorChanged(s_clone, p_str))
                .padding(2)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });
                swatches_row = swatches_row.push(p_btn);
            }

            col = col.push(
                row![
                    swatches_row,
                    text(name)
                        .size((scale_sz(0.75)) as f32)
                        .bold()
                        .color(theme.text),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            );
        }

        col = col.push(
            column![
                row![
                    text("Window & Card Border Width")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.global_border_width as u32))
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    1..=10,
                    settings.global_border_width as u32,
                    SettingsMessage::BorderWidth
                )
                .width(Length::Fill),
            ]
            .spacing(3),
        );
        col = col.push(
            column![
                row![
                    text("Control / Button Border Width")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.control_border_width as u32))
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    1..=8,
                    settings.control_border_width as u32,
                    SettingsMessage::ControlBorderWidth
                )
                .width(Length::Fill),
            ]
            .spacing(3),
        );

        col.into()
    }
}
