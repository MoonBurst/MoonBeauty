use super::*;
use iced::widget::column;

impl SettingsOverlay {
    pub(super) fn view_notifications<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let (_, card_h) = settings.get_window_size("settings", 1260, 840);
        let mut pings_col = column![
            text("CUSTOM PINGS")
                .size((scale_sz(0.95)) as f32)
                .bold()
                .color(theme.success),
            row![
                text("ALL:")
                    .size((scale_sz(0.68)) as f32)
                    .bold()
                    .color(theme.text),
                text("Sound")
                    .size((scale_sz(0.68)) as f32)
                    .bold()
                    .color(theme.danger),
                text("TTS")
                    .size((scale_sz(0.68)) as f32)
                    .bold()
                    .color(theme.danger),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .width(Length::Fixed(270.0));

        let mut pings_list = column![].spacing(6);
        for (idx, p) in settings.friend_profiles.iter().enumerate() {
            let p_color = hex_to_color(&p.color);
            let p_name = p.summary.clone();
            let is_sound = p.sound;
            let is_tts = p.tts;
            let is_selected = idx == self.selected_profile;

            let sound_btn = button(text("Sound").size((scale_sz(0.6)) as f32).bold().color(
                if is_sound {
                    Color::BLACK
                } else {
                    Color::from_rgb8(255, 60, 60)
                },
            ))
            .on_press(SettingsMessage::ToggleProfileSound(idx))
            .padding([2, 6])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_sound {
                    Color::from_rgb8(0, 224, 0)
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_sound {
                        Color::from_rgb8(0, 224, 0)
                    } else {
                        Color::from_rgb8(60, 60, 80)
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            });

            let tts_btn = button(text("TTS").size((scale_sz(0.6)) as f32).bold().color(
                if is_tts {
                    Color::BLACK
                } else {
                    Color::from_rgb8(255, 60, 60)
                },
            ))
            .on_press(SettingsMessage::ToggleProfileTts(idx))
            .padding([2, 6])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_tts {
                    Color::from_rgb8(0, 224, 0)
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_tts {
                        Color::from_rgb8(0, 224, 0)
                    } else {
                        Color::from_rgb8(60, 60, 80)
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            });

            let swatch = container(text(""))
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(p_color)),
                    border: iced::Border {
                        color: Color::WHITE,
                        width: 1.0,
                        radius: 7.0.into(),
                    },
                    ..Default::default()
                });

            let capsule_content = row![
                swatch,
                text(p_name)
                    .size((scale_sz(0.75)) as f32)
                    .bold()
                    .color(if is_selected {
                        theme.text
                    } else {
                        Color::from_rgb8(200, 200, 200)
                    })
                    .width(Length::Fill),
                sound_btn,
                tts_btn,
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            let card_btn = button(capsule_content)
                .on_press(SettingsMessage::SelectProfile(idx))
                .padding([4, 10])
                .width(Length::Fill)
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(if is_selected {
                        Color::from_rgb8(34, 38, 50)
                    } else {
                        Color::from_rgb8(18, 20, 26)
                    })),
                    border: iced::Border {
                        color: p_color,
                        width: if is_selected { 2.0 } else { 1.5 },
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                });

            pings_list = pings_list.push(card_btn);
        }

        let add_btn = button(
            text("+ Add User / Word")
                .size((scale_sz(0.72)) as f32)
                .bold()
                .color(theme.success)
                .align_x(Alignment::Center),
        )
        .on_press(SettingsMessage::AddNewProfile)
        .padding([6, 12])
        .width(Length::Fill)
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
            border: iced::Border {
                color: theme.success,
                width: 1.5,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        pings_col = pings_col.push(scrollable(pings_list).height(Length::Fixed(560.0)));
        pings_col = pings_col.push(add_btn);

        let active_pill = button(
            text(if settings.notifications_enabled {
                "ON"
            } else {
                "OFF"
            })
            .size((scale_sz(0.7)) as f32)
            .bold()
            .color(Color::BLACK),
        )
        .on_press(SettingsMessage::ToggleNotifications)
        .padding([4, 14])
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(if settings.notifications_enabled {
                theme.success
            } else {
                theme.danger
            })),
            border: iced::Border {
                color: if settings.notifications_enabled {
                    theme.success
                } else {
                    theme.danger
                },
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

        let test_btn = button(
            text("Test Notification")
                .size((scale_sz(0.72)) as f32)
                .bold()
                .color(theme.text),
        )
        .on_press(SettingsMessage::TestNotification)
        .padding([6, 16])
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb8(20, 22, 28))),
            border: iced::Border {
                color: theme.text,
                width: 1.5,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        let screens = ["Auto (Primary)", "HDMI-A-2", "DP-2", "DP-1"];
        let mut scr_row = row![text("Screen:")
            .size((scale_sz(0.78)) as f32)
            .bold()
            .color(theme.text)]
        .spacing(6)
        .align_y(Alignment::Center);
        for s in screens {
            let is_s = self.selected_screen == s;
            let s_str = s.to_string();
            let scr_btn = button(text(s).size((scale_sz(0.65)) as f32).bold().color(if is_s {
                Color::BLACK
            } else {
                theme.text
            }))
            .on_press(SettingsMessage::SelectNotifScreen(s_str))
            .padding([3, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_s {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_s { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            scr_row = scr_row.push(scr_btn);
        }

        let corners = [
            ("top-right", "Top-R"),
            ("top-left", "Top-L"),
            ("bottom-right", "Bot-R"),
            ("bottom-left", "Bot-L"),
        ];
        let mut corner_row = row![text("Corner:")
            .size((scale_sz(0.78)) as f32)
            .bold()
            .color(theme.text)]
        .spacing(6)
        .align_y(Alignment::Center);
        for (c_id, c_lbl) in corners {
            let is_c = settings.notif_corner == c_id;
            let c_str = c_id.to_string();
            let c_btn = button(
                text(c_lbl)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(if is_c { Color::BLACK } else { theme.text }),
            )
            .on_press(SettingsMessage::SelectNotifCorner(c_str))
            .padding([3, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_c {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_c { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            corner_row = corner_row.push(c_btn);
        }

        let shapes = [
            ("round", "Round"),
            ("hex", "Hex"),
            ("slant", "Slant"),
            ("flat", "Flat"),
        ];
        let mut shape_row = row![text("Shape:")
            .size((scale_sz(0.78)) as f32)
            .bold()
            .color(theme.text)]
        .spacing(6)
        .align_y(Alignment::Center);
        for (s_id, s_lbl) in shapes {
            let is_sh = settings.notif_shape == s_id;
            let sh_str = s_id.to_string();
            let sh_btn = button(
                text(s_lbl)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(if is_sh { Color::BLACK } else { theme.text }),
            )
            .on_press(SettingsMessage::SelectNotifShape(sh_str))
            .padding([3, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_sh {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_sh { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            shape_row = shape_row.push(sh_btn);
        }

        let col_left_sliders = column![
            column![
                row![
                    text("Hold Duration")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}s", settings.notif_hold_duration_sec))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(1..=20, settings.notif_hold_duration_sec as u32, |v| {
                    SettingsMessage::NotifHoldDuration(v as u64)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Baseline Y Offset")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_baseline_y))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    20..=800,
                    settings.notif_baseline_y,
                    SettingsMessage::NotifBaselineY
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Card Stack Overlap")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_stack_overlap))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    -100..=100,
                    settings.notif_stack_overlap,
                    SettingsMessage::NotifStackOverlap
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Notification Card Height")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_card_height))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    90..=240,
                    settings.notif_card_height,
                    SettingsMessage::NotifCardHeight
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Drop Pause")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}ms", settings.notif_drop_pause_ms))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    0..=400,
                    settings.notif_drop_pause_ms,
                    SettingsMessage::NotifDropPause
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Summary Font Size")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_summary_size))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(10..=36, settings.notif_summary_size as u32, |v| {
                    SettingsMessage::NotifSummarySize(v as u16)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .width(Length::FillPortion(1));

        let col_right_sliders = column![
            column![
                row![
                    text("Sound Volume")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}%", settings.notif_volume))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(0..=100, settings.notif_volume, SettingsMessage::NotifVolume)
                    .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Margin X Offset")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_margin_x))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    0..=150,
                    settings.notif_margin_x,
                    SettingsMessage::NotifMarginX
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Notification Card Width")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_card_width))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    250..=650,
                    settings.notif_card_width,
                    SettingsMessage::NotifCardWidth
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Slide-In Time")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}ms", settings.notif_slide_in_ms))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    50..=600,
                    settings.notif_slide_in_ms,
                    SettingsMessage::NotifSlideIn
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Drop Travel Distance")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_drop_distance_px))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    10..=400,
                    settings.notif_drop_distance_px,
                    SettingsMessage::NotifDropDistance
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Body Font Size")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_body_size))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(8..=32, settings.notif_body_size as u32, |v| {
                    SettingsMessage::NotifBodySize(v as u16)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Notification Icon Size")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_icon_size))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    24..=96,
                    settings.notif_icon_size,
                    SettingsMessage::NotifIconSize
                )
                .width(Length::Fill),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .width(Length::FillPortion(1));

        let profile_inspector: Element<'a, SettingsMessage> = if let Some(p) =
            settings.friend_profiles.get(self.selected_profile)
        {
            let sel_idx = self.selected_profile;
            let p_summary = p.summary.clone();
            let p_sound = p.sound_path.clone().unwrap_or_default();
            let p_tts = p
                .default_readout
                .clone()
                .unwrap_or_else(|| "Message from {name}".into());

            let colors = [
                "#38FEDC", "#758593", "#8be9fd", "#ED54BA", "#F5F557", "#132ED1", "#C51111",
                "#EF7D0D", "#117F2D", "#D6E0F0", "#FFFFFF",
            ];
            let mut swatch_row = row![text("Tag Color:")
                .size((scale_sz(0.72)) as f32)
                .bold()
                .color(theme.text)]
            .spacing(6)
            .align_y(Alignment::Center);
            for c_hex in colors {
                let c_val = hex_to_color(c_hex);
                let hex_str = c_hex.to_string();
                let is_active = p.color.eq_ignore_ascii_case(c_hex);
                let s_btn = button(
                    container(text(""))
                        .width(Length::Fixed(16.0))
                        .height(Length::Fixed(16.0))
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(c_val)),
                            border: iced::Border {
                                color: if is_active {
                                    Color::WHITE
                                } else {
                                    Color::TRANSPARENT
                                },
                                width: if is_active { 2.0 } else { 0.0 },
                                radius: 8.0.into(),
                            },
                            ..Default::default()
                        }),
                )
                .on_press(SettingsMessage::ProfileColorChanged(sel_idx, hex_str))
                .padding(2)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });

                swatch_row = swatch_row.push(s_btn);
            }

            let mut rules_col = column![row![
                text(format!("BODY RULES ({})", p.content_rules.len()))
                    .size((scale_sz(0.72)) as f32)
                    .bold()
                    .color(theme.success),
                row![].width(Length::Fill),
                button(
                    text("+ Add Body Rule")
                        .size((scale_sz(0.68)) as f32)
                        .bold()
                        .color(theme.success)
                )
                .on_press(SettingsMessage::AddContentRule(sel_idx))
                .padding([3, 8])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgb8(20, 25, 20))),
                    border: iced::Border {
                        color: theme.success,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                }),
            ]
            .align_y(Alignment::Center),]
            .spacing(6);

            for (r_idx, r) in p.content_rules.iter().enumerate() {
                let pat = r.pattern.clone();
                let rd = r.readout.clone();
                rules_col = rules_col.push(
                    row![
                        text("When contains:")
                            .size((scale_sz(0.65)) as f32)
                            .color(theme.muted),
                        text_input("pattern", &pat)
                            .on_input(move |v| SettingsMessage::RulePatternChanged(
                                sel_idx, r_idx, v
                            ))
                            .padding(4)
                            .size((scale_sz(0.68)) as f32)
                            .width(Length::Fixed(140.0)),
                        text("TTS:")
                            .size((scale_sz(0.65)) as f32)
                            .color(theme.muted),
                        text_input("readout", &rd)
                            .on_input(move |v| SettingsMessage::RuleReadoutChanged(
                                sel_idx, r_idx, v
                            ))
                            .padding(4)
                            .size((scale_sz(0.68)) as f32)
                            .width(Length::Fill),
                        button(text("✕").size(10).color(theme.danger))
                            .on_press(SettingsMessage::DeleteContentRule(sel_idx, r_idx))
                            .padding([3, 6])
                            .style(|_t, _s| button::Style {
                                background: None,
                                ..Default::default()
                            }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }

            column![
                row![
                    text(format!("CONFIGURING PING: {}", p.summary.to_uppercase()))
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.success),
                    row![].width(Length::Fill),
                    button(
                        text("Delete Rule")
                            .size((scale_sz(0.68)) as f32)
                            .bold()
                            .color(theme.danger)
                    )
                    .on_press(SettingsMessage::DeleteProfile(sel_idx))
                    .padding([3, 10])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb8(30, 15, 15))),
                        border: iced::Border {
                            color: theme.danger,
                            width: 1.0,
                            radius: 4.0.into()
                        },
                        ..Default::default()
                    }),
                ]
                .align_y(Alignment::Center),
                row![
                    text("Match Tag:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    text_input("Name/Tag", &p_summary)
                        .on_input(move |v| SettingsMessage::ProfileSummaryChanged(sel_idx, v))
                        .padding(5)
                        .size((scale_sz(0.72)) as f32),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Base Sound:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    text_input("Sound Path (opt)", &p_sound)
                        .on_input(move |v| SettingsMessage::ProfileSoundPathChanged(sel_idx, v))
                        .padding(5)
                        .size((scale_sz(0.72)) as f32),
                    button(text("▶").size((scale_sz(0.7)) as f32).color(theme.success))
                        .on_press(SettingsMessage::TestProfileSound(sel_idx))
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
                            border: iced::Border {
                                color: theme.success,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                    button(text("✕").size(10).color(theme.danger))
                        .on_press(SettingsMessage::ProfileSoundPathChanged(sel_idx, "".into()))
                        .padding([4, 8])
                        .style(|_t, _s| button::Style {
                            background: None,
                            ..Default::default()
                        }),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Default TTS:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    text_input("Message from {name}", &p_tts)
                        .on_input(move |v| SettingsMessage::ProfileTtsTemplateChanged(sel_idx, v))
                        .padding(5)
                        .size((scale_sz(0.72)) as f32),
                    button(text("▶").size((scale_sz(0.7)) as f32).color(theme.success))
                        .on_press(SettingsMessage::TestProfileTts(sel_idx))
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
                            border: iced::Border {
                                color: theme.success,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Avatar Image:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    button(
                        container(
                            text("📁 Pick Image & Auto-Crop")
                                .size((scale_sz(0.72)) as f32)
                                .color(Color::BLACK)
                        )
                        .padding([5, 10])
                    )
                    .on_press(SettingsMessage::PickAvatar(sel_idx))
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.text)),
                        border: iced::Border {
                            color: theme.text,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                swatch_row,
                rules_col,
            ]
            .spacing(8)
            .into()
        } else {
            column![].into()
        };

        let controls_col = column![
            row![
                text("NOTIFICATION CONTROLS")
                    .size((scale_sz(0.95)) as f32)
                    .bold()
                    .color(theme.success),
                row![].width(Length::Fill),
                test_btn,
            ]
            .align_y(Alignment::Center),
            row![
                text("Notifications Active")
                    .size((scale_sz(0.8)) as f32)
                    .bold()
                    .color(theme.text),
                row![].width(Length::Fill),
                active_pill,
            ]
            .align_y(Alignment::Center),
            scr_row,
            corner_row,
            shape_row,
            row![col_left_sliders, col_right_sliders].spacing(16),
            column![
                text("Excluded Apps (comma separated):")
                    .size((scale_sz(0.72)) as f32)
                    .bold()
                    .color(theme.text),
                text_input("greenclip, copyq", &settings.notif_excluded)
                    .on_input(SettingsMessage::NotifExcludedChanged)
                    .padding(6)
                    .size((scale_sz(0.75)) as f32),
            ]
            .spacing(4),
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(theme.border)),
                    ..Default::default()
                }),
            profile_inspector,
        ]
        .spacing(10)
        .width(Length::Fill);

        let right_scrollable = scrollable(container(controls_col).padding(Padding {
            top: 0.0,
            right: 28.0,
            bottom: 12.0,
            left: 0.0,
        }))
        .height(Length::Fixed(card_h - 180.0))
        .width(Length::Fill);

        row![pings_col, right_scrollable].spacing(24).into()
    }
}
