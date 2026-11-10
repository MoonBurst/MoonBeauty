use super::*;
use iced::widget::column;

impl SettingsOverlay {
    pub(super) fn view_bar_layout<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let all_capsules = [
            "calendar", "music", "alarm", "weather", "unified", "notify", "audio", "clock", "mic",
            "custom", "net", "cpu", "gpu", "ram", "battery", "tray",
        ];

        let mk_toggle = move |label: &'static str, active: bool, msg: SettingsMessage| {
            let bg = if active {
                theme.success
            } else {
                theme.background
            };
            let fg = if active { Color::BLACK } else { theme.text };
            let border = if active { theme.success } else { theme.text };
            button(text(label).size((scale_sz(0.7)) as f32).bold().color(fg))
                .on_press(msg)
                .padding([4, 10])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        color: border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
        };

        // ── Monitor picker ────────────────────────────────
        let monitors = ["All", "DP-1", "DP-2", "HDMI-A-1", "HDMI-A-2"];
        let mut mon_row: iced::widget::Row<'a, SettingsMessage> = iced::widget::Row::new()
            .spacing(8)
            .align_y(Alignment::Center)
            .push(
                text("Bar Monitor:")
                    .size((scale_sz(0.85)) as f32)
                    .bold()
                    .color(theme.text),
            );
        for m in monitors {
            let is_sel = settings.bar_monitor == m;
            let m_str = m.to_string();
            let btn = button(
                text(m)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(if is_sel { Color::BLACK } else { theme.text }),
            )
            .on_press(SettingsMessage::SelectBarMonitor(m_str))
            .padding([3, 10])
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
            mon_row = mon_row.push(btn);
        }

        let bg_bar_row: Element<'a, SettingsMessage> = row![
            text("Background Bar Shape:")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.text),
            mk_toggle(
                "SLANT",
                settings.bar_shape == "slant",
                SettingsMessage::SelectBarShape("slant".into())
            ),
            mk_toggle(
                "HEXAGON",
                settings.bar_shape == "hexagon",
                SettingsMessage::SelectBarShape("hexagon".into())
            ),
            mk_toggle(
                "ROUNDED",
                settings.bar_shape == "rounded",
                SettingsMessage::SelectBarShape("rounded".into())
            ),
            mk_toggle(
                "FLAT",
                settings.bar_shape == "flat",
                SettingsMessage::SelectBarShape("flat".into())
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

        let module_slants_row: Element<'a, SettingsMessage> = row![
            text("Module Slants:")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.text),
            mk_toggle(
                "LEFT",
                settings.slant_mode == "left",
                SettingsMessage::SelectSlantMode("left".into())
            ),
            mk_toggle(
                "CENTER",
                settings.slant_mode == "center"
                    || settings.slant_mode == "symmetric"
                    || settings.slant_mode == "sym",
                SettingsMessage::SelectSlantMode("center".into())
            ),
            mk_toggle(
                "RIGHT",
                settings.slant_mode == "right",
                SettingsMessage::SelectSlantMode("right".into())
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

        let module_shape_row: Element<'a, SettingsMessage> = row![
            text("Module Shape:")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.text),
            mk_toggle(
                "SLANT",
                settings.module_shape == "slant",
                SettingsMessage::SelectModuleShape("slant".into())
            ),
            mk_toggle(
                "HEXAGON",
                settings.module_shape == "hexagon",
                SettingsMessage::SelectModuleShape("hexagon".into())
            ),
            mk_toggle(
                "ROUNDED",
                settings.module_shape == "rounded",
                SettingsMessage::SelectModuleShape("rounded".into())
            ),
            mk_toggle(
                "FLAT",
                settings.module_shape == "flat",
                SettingsMessage::SelectModuleShape("flat".into())
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

        let top_sliders: Element<'a, SettingsMessage> = column![
            row![
                column![
                    row![
                        text("Top Bar Height")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.bar_height))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(28..=90, settings.bar_height, SettingsMessage::BarHeight)
                        .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Global Font Size")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.global_font_size))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        8..=32,
                        settings.global_font_size,
                        SettingsMessage::GlobalFontSize
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
            ]
            .spacing(20)
            .width(Length::Fill),
            row![
                column![
                    row![
                        text("Capsule Spacing")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.capsule_spacing))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        -50..=40,
                        settings.capsule_spacing,
                        SettingsMessage::CapsuleSpacing
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Capsule Slant Width")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.slant_width as u32))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        0..=40,
                        settings.slant_width as u32,
                        SettingsMessage::SlantWidth
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Capsule Internal Padding")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.capsule_internal_padding))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        0..=60,
                        settings.capsule_internal_padding,
                        SettingsMessage::CapsuleInternalPadding
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
            ]
            .spacing(20)
            .width(Length::Fill),
        ]
        .spacing(12)
        .width(Length::Fill)
        .into();

        // Finding 77: per-slot offset columns. Slider shows the
        // effective value (auto or override); label marks "(auto)"
        // when the field is None; reset button clears the override.
        let offset_left_col: Element<'a, SettingsMessage> = {
            let is_auto = settings.tooltip_offset_left.is_none();
            let eff = settings.effective_tooltip_offset_left();
            column![
                row![
                    text("Tooltip Left Offset")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(if is_auto {
                        format!("{}px (auto)", eff)
                    } else {
                        format!("{}px", eff)
                    })
                    .size((scale_sz(0.8)) as f32)
                    .bold()
                    .color(theme.text),
                    button(text(if is_auto { " " } else { "↺" }).size(11))
                        .on_press_maybe(if is_auto {
                            None
                        } else {
                            Some(SettingsMessage::TooltipOffsetLeftReset)
                        })
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: if is_auto {
                                None
                            } else {
                                Some(iced::Background::Color(theme.surface_alt))
                            },
                            border: if is_auto {
                                iced::Border::default()
                            } else {
                                iced::Border {
                                    color: theme.warning,
                                    width: 1.0,
                                    radius: 3.0.into(),
                                }
                            },
                            text_color: if is_auto { theme.muted } else { theme.warning },
                            ..Default::default()
                        }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                slider(-50..=100, eff, SettingsMessage::TooltipOffsetLeft).width(Length::Fill),
            ]
            .spacing(3)
            .width(Length::FillPortion(1))
            .into()
        };
        let offset_right_col: Element<'a, SettingsMessage> = {
            let is_auto = settings.tooltip_offset_right.is_none();
            let eff = settings.effective_tooltip_offset_right();
            column![
                row![
                    text("Tooltip Right Offset")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(if is_auto {
                        format!("{}px (auto)", eff)
                    } else {
                        format!("{}px", eff)
                    })
                    .size((scale_sz(0.8)) as f32)
                    .bold()
                    .color(theme.text),
                    button(text(if is_auto { " " } else { "↺" }).size(11))
                        .on_press_maybe(if is_auto {
                            None
                        } else {
                            Some(SettingsMessage::TooltipOffsetRightReset)
                        })
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: if is_auto {
                                None
                            } else {
                                Some(iced::Background::Color(theme.surface_alt))
                            },
                            border: if is_auto {
                                iced::Border::default()
                            } else {
                                iced::Border {
                                    color: theme.warning,
                                    width: 1.0,
                                    radius: 3.0.into(),
                                }
                            },
                            text_color: if is_auto { theme.muted } else { theme.warning },
                            ..Default::default()
                        }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                slider(-50..=100, eff, SettingsMessage::TooltipOffsetRight).width(Length::Fill),
            ]
            .spacing(3)
            .width(Length::FillPortion(1))
            .into()
        };
        let anim_sliders: Element<'a, SettingsMessage> = column![
            row![
                column![
                    row![
                        text("Tooltip Drop Speed")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}ms", settings.tooltip_drop_ms))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        5..=100,
                        (settings.tooltip_drop_ms / 10).clamp(5, 100),
                        |v: u32| SettingsMessage::TooltipDropMs(v * 10)
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Tooltip Expand Speed")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}ms", settings.tooltip_expand_ms))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        5..=100,
                        (settings.tooltip_expand_ms / 10).clamp(5, 100),
                        |v: u32| SettingsMessage::TooltipExpandMs(v * 10)
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
            ]
            .spacing(20)
            .width(Length::Fill),
            row![
                column![
                    row![
                        text("Tooltip Pause Time")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}ms", settings.tooltip_pause_ms))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        0..=100,
                        (settings.tooltip_pause_ms / 10).min(100),
                        |v: u32| SettingsMessage::TooltipPauseMs(v * 10)
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Tooltip Text Size")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.tooltip_font_size))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        8..=28,
                        settings.tooltip_font_size,
                        SettingsMessage::TooltipFontSize
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                offset_left_col,
                offset_right_col,
            ]
            .spacing(20)
            .width(Length::Fill),
        ]
        .spacing(12)
        .width(Length::Fill)
        .into();

        let render_row = |id_str: &str, is_visible: bool| -> Element<'a, SettingsMessage> {
            let id = id_str.to_string();
            let (w_val, h_val) = settings.tooltip_size(id_str);
            let _id_w = id_str.to_string();
            let _id_h = id_str.to_string();
            let id_reset = id_str.to_string();
            let has_override = settings.tooltip_sizes.contains_key(id_str);

            let up = button(text("▲").size((scale_sz(0.6)) as f32).color(theme.text))
                .on_press(SettingsMessage::MoveCapsuleUp(id.clone()))
                .padding([2, 6])
                .style(move |_t, _s| button::Style {
                    background: None,
                    border: iced::Border::default(),
                    ..Default::default()
                });
            let down = button(text("▼").size((scale_sz(0.6)) as f32).color(theme.text))
                .on_press(SettingsMessage::MoveCapsuleDown(id.clone()))
                .padding([2, 6])
                .style(move |_t, _s| button::Style {
                    background: None,
                    border: iced::Border::default(),
                    ..Default::default()
                });

            let (min_w, min_h): (u32, u32) = min_size_for(id_str);

            let size_controls = if id_str == "tray" {
                row![
                    text("W:").size((scale_sz(0.6)) as f32).color(theme.muted),
                    slider(30..=400, settings.tray_width, SettingsMessage::TrayWidth)
                        .width(Length::Fixed(80.0)),
                    text(format!("{}", settings.tray_width))
                        .size((scale_sz(0.6)) as f32)
                        .color(theme.text)
                        .width(Length::Fixed(25.0)),
                    text("I:").size((scale_sz(0.6)) as f32).color(theme.muted),
                    slider(
                        12..=48,
                        settings.tray_icon_size,
                        SettingsMessage::TrayIconSize
                    )
                    .width(Length::Fixed(80.0)),
                    text(format!("{}px", settings.tray_icon_size))
                        .size((scale_sz(0.6)) as f32)
                        .color(theme.text)
                        .width(Length::Fixed(25.0)),
                    text("T:").size((scale_sz(0.6)) as f32).color(theme.muted),
                    slider(
                        0..=30,
                        settings.tray_timeout_sec,
                        SettingsMessage::TrayTimeout
                    )
                    .width(Length::Fixed(80.0)),
                    text(format!("{}s", settings.tray_timeout_sec))
                        .size((scale_sz(0.6)) as f32)
                        .color(theme.text)
                        .width(Length::Fixed(25.0)),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
            } else {
                let resizer = crate::overlays::size_resizer::SizeResizer {
                    id: id_str.to_string(),
                    w: w_val,
                    h: h_val,
                    min_w,
                    min_h,
                    max_w: 1200,
                    max_h: 1000,
                    bg: theme.background,
                    fg: theme.text,
                    border: theme.border,
                    muted: theme.muted,
                    display_w: 180.0,
                    display_h: 150.0,
                };
                row![
                    resizer.view(),
                    button(
                        text(if has_override { "×" } else { "·" })
                            .size((scale_sz(0.7)) as f32)
                            .color(if has_override {
                                theme.danger
                            } else {
                                theme.muted
                            })
                    )
                    .on_press(SettingsMessage::ResetTooltipSize(id_reset))
                    .padding([2, 6])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border {
                            color: if has_override {
                                theme.danger
                            } else {
                                theme.border
                            },
                            width: 1.0,
                            radius: 3.0.into()
                        },
                        ..Default::default()
                    }),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            };

            let btn_text = if is_visible { "HIDE" } else { "SHOW" };
            let btn_bg = if is_visible {
                theme.danger
            } else {
                theme.success
            };
            let toggle_btn = button(
                text(btn_text)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(Color::BLACK),
            )
            .on_press(SettingsMessage::ToggleCapsuleVisible(id.clone()))
            .padding([4, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(btn_bg)),
                border: iced::Border {
                    color: btn_bg,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            let mk_move = |lbl: &'static str, target: &'static str| {
                button(text(lbl).size((scale_sz(0.55)) as f32).color(theme.text))
                    .on_press(SettingsMessage::MoveCapsuleTo(id.clone(), target.into()))
                    .padding([2, 4])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border {
                            color: theme.text,
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..Default::default()
                    })
            };
            let controls = row![
                mk_move("L", "left"),
                mk_move("C", "center"),
                mk_move("R", "right"),
                toggle_btn
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            let base_row = row![
                row![up, down].spacing(4),
                text(id_str.to_uppercase())
                    .size((scale_sz(0.7)) as f32)
                    .bold()
                    .color(theme.text)
                    .width(Length::Fixed(110.0)),
                size_controls,
                row![].width(Length::Fill),
                controls
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            let mut content_col = column![base_row].spacing(8).width(Length::Fill);

            if id_str == "alarm" {
                let audio_ui = column![
                    row![
                        text("Sound:")
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.muted)
                            .width(Length::Fixed(55.0)),
                        text_input(
                            "filename.mp3",
                            settings.alarm_sound.as_deref().unwrap_or("")
                        )
                        .on_input(SettingsMessage::AlarmSoundChanged)
                        .padding(4)
                        .size((scale_sz(0.7)) as f32)
                        .width(Length::Fill),
                        button(
                            text("Import")
                                .size((scale_sz(0.65)) as f32)
                                .bold()
                                .color(theme.text)
                        )
                        .on_press(SettingsMessage::ImportAlarmSound)
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.background)),
                            border: iced::Border {
                                color: theme.text,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                        button(
                            text("Test")
                                .size((scale_sz(0.65)) as f32)
                                .bold()
                                .color(theme.text)
                        )
                        .on_press(SettingsMessage::TestAlarmSound)
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.background)),
                            border: iced::Border {
                                color: theme.text,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Duration:")
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.muted)
                            .width(Length::Fixed(55.0)),
                        slider(
                            1..=60,
                            settings.alarm_sound_duration_sec,
                            SettingsMessage::AlarmSoundDurationChanged
                        )
                        .width(Length::Fill),
                        text(format!("{}s", settings.alarm_sound_duration_sec))
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(25.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Volume:")
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.muted)
                            .width(Length::Fixed(55.0)),
                        slider(
                            0..=100,
                            settings.alarm_sound_volume,
                            SettingsMessage::AlarmSoundVolumeChanged
                        )
                        .width(Length::Fill),
                        text(format!("{}%", settings.alarm_sound_volume))
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(25.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6)
                .width(Length::Fill);

                content_col = content_col.push(audio_ui);
            }

            container(content_col)
                .padding([8, 10])
                .width(Length::Fill)
                .style(move |_t| container::Style {
                    border: iced::Border {
                        color: theme.text,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        };

        let section_header = |title: &'static str, color: Color| {
            container(row![
                text(title)
                    .size((scale_sz(0.85)) as f32)
                    .bold()
                    .color(color),
                row![].width(Length::Fill),
            ])
            .padding([4, 12])
            .style(move |_t| container::Style {
                border: iced::Border {
                    color,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .width(Length::Fill)
        };

        let mut sections: iced::widget::Column<'a, SettingsMessage> =
            iced::widget::Column::new().spacing(16).width(Length::Fill);

        sections = sections.push(section_header("LEFT BAR SECTION", theme.success));
        for id in &settings.bar_left {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                sections = sections.push(render_row(id, true));
            }
        }
        sections = sections.push(section_header("CENTER BAR SECTION", theme.success));
        for id in &settings.bar_center {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                sections = sections.push(render_row(id, true));
            }
        }
        sections = sections.push(section_header("RIGHT BAR SECTION", theme.success));
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                sections = sections.push(render_row(id, true));
            }
        }
        sections = sections.push(section_header("HIDDEN MODULES", theme.danger));
        for id_str in all_capsules {
            let id = id_str.to_string();
            let is_vis = settings.visible_capsules.get(&id).copied().unwrap_or(true);
            let in_left = settings.bar_left.contains(&id);
            let in_center = settings.bar_center.contains(&id);
            let in_right = settings.bar_right.contains(&id);
            if !is_vis || (!in_left && !in_center && !in_right) {
                sections = sections.push(render_row(id_str, false));
            }
        }

        container(
            scrollable(
                column![
                    text("BAR GEOMETRY, CAPSULE SHAPES & LAYOUT")
                        .size((scale_sz(1.0)) as f32)
                        .bold()
                        .color(theme.success),
                    mon_row,
                    bg_bar_row,
                    module_slants_row,
                    module_shape_row,
                    top_sliders,
                    anim_sliders,
                    sections,
                ]
                .spacing(16)
                .width(Length::Fill),
            )
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
        .into()
    }
}
