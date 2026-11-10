use super::*;
use iced::widget::column;

impl SettingsOverlay {
    pub(super) fn view_hardware<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let mut gpu_col = column![text("🎮 GPU ALERT THRESHOLDS & MONITORING")
            .size((scale_sz(0.95)) as f32)
            .bold()
            .color(theme.success),]
        .spacing(12)
        .width(Length::Fill);

        gpu_col = gpu_col.push(
            text("CPU & RAM WARNINGS")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.success),
        );

        gpu_col = gpu_col.push(
            row![
                column![
                    row![
                        text("CPU Temp Warning (°C)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}°C", settings.cpu_temp_warn))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        40..=95,
                        settings.cpu_temp_warn,
                        SettingsMessage::CpuTempWarn
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("CPU Temp Danger (°C)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}°C", settings.cpu_temp_danger))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        50..=105,
                        settings.cpu_temp_danger,
                        SettingsMessage::CpuTempDanger
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
        );

        gpu_col = gpu_col.push(
            row![
                column![
                    row![
                        text("CPU Usage Warning (%)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}%", settings.cpu_usage_warn))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        10..=99,
                        settings.cpu_usage_warn,
                        SettingsMessage::CpuUsageWarn
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("CPU Usage Danger (%)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}%", settings.cpu_usage_danger))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        20..=100,
                        settings.cpu_usage_danger,
                        SettingsMessage::CpuUsageDanger
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
        );

        gpu_col = gpu_col.push(
            row![
                column![
                    row![
                        text("RAM Remaining Warning (GiB)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{} GiB", settings.ram_warn_gib))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(1..=32, settings.ram_warn_gib, SettingsMessage::RamWarnGib)
                        .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("RAM Remaining Danger (GiB)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{} GiB", settings.ram_danger_gib))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        1..=32,
                        settings.ram_danger_gib,
                        SettingsMessage::RamDangerGib
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
        );

        gpu_col = gpu_col.push(
            text("Active GPU (shown in top bar)")
                .size((scale_sz(0.8)) as f32)
                .bold()
                .color(theme.muted),
        );

        let mut selector_row: iced::widget::Row<'a, SettingsMessage> = iced::widget::Row::new()
            .spacing(8)
            .align_y(Alignment::Center);

        if self.detected_gpus.is_empty() {
            selector_row = selector_row.push(
                text("No GPUs detected")
                    .size((scale_sz(0.72)) as f32)
                    .color(theme.muted),
            );
        } else {
            for gpu in &self.detected_gpus {
                let is_sel = settings.gpu_selected_id == gpu.card_id;
                let card_id = gpu.card_id.clone();
                let label = gpu.name.clone();
                let btn = button(
                    text(label)
                        .size((scale_sz(0.68)) as f32)
                        .bold()
                        .color(if is_sel { Color::BLACK } else { theme.text }),
                )
                .on_press(SettingsMessage::SelectGpuId(card_id))
                .padding([4, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(if is_sel {
                        theme.text
                    } else {
                        Color::TRANSPARENT
                    })),
                    border: iced::Border {
                        color: if is_sel { theme.text } else { theme.border },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });
                selector_row = selector_row.push(btn);
            }
        }
        gpu_col = gpu_col.push(selector_row);

        for idx in 0usize..2 {
            let th = if idx == 0 {
                &settings.gpu0
            } else {
                &settings.gpu1
            };
            let gpu_name = self.detected_gpus.get(idx).map(|g| g.name.clone());
            let section_label = match gpu_name {
                Some(n) => format!("── GPU {}: {} ──", idx, n),
                None => format!("── GPU {} (not detected) ──", idx),
            };

            gpu_col = gpu_col.push(
                text(section_label)
                    .size((scale_sz(0.85)) as f32)
                    .bold()
                    .color(theme.success),
            );

            let tw_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuTempWarn
            } else {
                SettingsMessage::Gpu1TempWarn
            };
            let td_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuTempDanger
            } else {
                SettingsMessage::Gpu1TempDanger
            };
            let vw_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuVramWarn
            } else {
                SettingsMessage::Gpu1VramWarn
            };
            let vd_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuVramDanger
            } else {
                SettingsMessage::Gpu1VramDanger
            };

            gpu_col = gpu_col.push(
                row![
                    column![
                        row![
                            text("Temp Warning (°C)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{}°C", th.temp_warn))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(40..=95, th.temp_warn, tw_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                    column![
                        row![
                            text("Temp Danger (°C)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{}°C", th.temp_danger))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(50..=105, th.temp_danger, td_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                ]
                .spacing(16),
            );

            gpu_col = gpu_col.push(
                row![
                    column![
                        row![
                            text("Free VRAM Warning (GiB)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{} GiB", th.vram_warn))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(1..=16, th.vram_warn, vw_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                    column![
                        row![
                            text("Free VRAM Danger (GiB)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{} GiB", th.vram_danger))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(0..=8, th.vram_danger, vd_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                ]
                .spacing(16),
            );
        }

        gpu_col = gpu_col.push(
            column![
                row![
                    text("Hardware Polling Interval (ms)")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}ms", settings.hardware_poll_interval_ms))
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(500..=5000, settings.hardware_poll_interval_ms as u32, |v| {
                    SettingsMessage::HardwarePollInterval(v as u64)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
        );

        let body: Element<'a, SettingsMessage> = container(
            scrollable(container(gpu_col).padding(iced::Padding {
                top: 0.0,
                right: 24.0,
                bottom: 0.0,
                left: 0.0,
            }))
            .height(Length::Fill)
            .width(Length::Fill),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: (settings.overlay_cut as f32) + 16.0,
            bottom: (settings.overlay_cut as f32) + 16.0,
            left: (settings.overlay_cut as f32) + 16.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into();
        body
    }
}
