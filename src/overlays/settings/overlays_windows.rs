use super::*;
use iced::widget::column;

impl SettingsOverlay {
    pub(super) fn view_overlays_windows<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };

        if matches!(self.browse_mode, Some(BrowseMode::MusicDir)) {
            if let Some(browser) = self.view_in_app_browser(settings, theme) {
                return browser;
            }
        }
        let mut col = column![
            text("🪟 OVERLAY WINDOW SIZING & GEOMETRY")
                .size(base_sz as f32)
                .bold()
                .color(theme.success),
            text("Global dimensions apply to all overlays (unless individually overridden).")
                .size(scale_sz(0.82) as f32)
                .color(theme.text),
        ]
        .spacing(8);

        col = col.push(
            container(
                column![
                    row![
                        text("Overlay Window Font Size")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.overlay_font_size))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(11..=32, settings.overlay_font_size as u32, |v| {
                        SettingsMessage::OverlayFontSize(v as u16)
                    })
                    .width(Length::Fill),
                ]
                .spacing(6),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        col = col.push(
            container(
                column![
                    text("⬡ Global Overlay Hexagon Geometry")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    row![
                        text("Cut:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        slider(0..=200, settings.overlay_cut, SettingsMessage::OverlayCut)
                            .width(Length::Fixed(180.0)),
                        text(format!("{}px", settings.overlay_cut))
                            .size(scale_sz(0.85) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(62.0)),
                        text("Angle:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        slider(
                            5..=85,
                            settings.overlay_angle,
                            SettingsMessage::OverlayAngle
                        )
                        .width(Length::Fixed(180.0)),
                        text(format!("{}°", settings.overlay_angle))
                            .size(scale_sz(0.85) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(48.0)),
                        button(text("30°").size(scale_sz(0.85) as f32).bold().color(
                            if settings.overlay_angle == 30 {
                                theme.success
                            } else {
                                theme.text
                            }
                        ))
                        .on_press(SettingsMessage::OverlayAngle(30))
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: if settings.overlay_angle == 30 {
                                    theme.focus
                                } else {
                                    theme.border
                                },
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                        button(text("45°").size(scale_sz(0.85) as f32).bold().color(
                            if settings.overlay_angle == 45 {
                                theme.success
                            } else {
                                theme.text
                            }
                        ))
                        .on_press(SettingsMessage::OverlayAngle(45))
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: if settings.overlay_angle == 45 {
                                    theme.focus
                                } else {
                                    theme.border
                                },
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                        button(text("60°").size(scale_sz(0.85) as f32).bold().color(
                            if settings.overlay_angle == 60 {
                                theme.success
                            } else {
                                theme.text
                            }
                        ))
                        .on_press(SettingsMessage::OverlayAngle(60))
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: if settings.overlay_angle == 60 {
                                    theme.focus
                                } else {
                                    theme.border
                                },
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        col = col.push(
            container(
                column![
                    text("📁 Attachment Download Directory")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    text("Leave blank for default (~/Downloads).")
                        .size(scale_sz(0.85) as f32)
                        .color(theme.text),
                    text_input("Default (~/Downloads)", &settings.attachment_download_dir)
                        .on_input(SettingsMessage::AttachmentDownloadDirChanged)
                        .padding(8)
                        .size(scale_sz(0.85) as f32)
                        .width(Length::Fill),
                ]
                .spacing(8),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        let mut music_dirs_col = column![
            row![
                text("🎵 Music Library Folders")
                    .size(scale_sz(0.8) as f32)
                    .bold()
                    .color(theme.success),
                row![].width(Length::Fill),
                button(text("📁 Browse…").size(scale_sz(0.85) as f32).bold().color(theme.background))
                    .on_press(SettingsMessage::PickMusicDir)
                    .padding([3, 8])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.text)),
                        border: iced::Border { color: theme.text, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    }),
                button(text("+ Add").size(scale_sz(0.85) as f32).bold().color(theme.success))
                    .on_press(SettingsMessage::AddMusicDir)
                    .padding([3, 8])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.surface_alt)),
                        border: iced::Border { color: theme.success, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    }),
                button(text("↻ Rescan").size(scale_sz(0.85) as f32).bold().color(theme.text))
                    .on_press(SettingsMessage::RescanMusicLibrary)
                    .padding([3, 8])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.surface_alt)),
                        border: iced::Border { color: theme.border, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    }),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
            text("Leave empty for default (~/Music). Supports ~/ paths; subdirectories are scanned recursively.")
                .size(scale_sz(0.85) as f32)
                .color(theme.text),
        ]
        .spacing(8);

        if settings.music_dirs.is_empty() {
            music_dirs_col = music_dirs_col.push(text("Using default: ~/Music").size(scale_sz(0.85) as f32).color(theme.text));
        } else {
            for (idx, dir) in settings.music_dirs.iter().enumerate() {
                music_dirs_col = music_dirs_col.push(
                    row![
                        text_input("/path/to/music", dir)
                            .on_input(move |v| SettingsMessage::MusicDirChanged(idx, v))
                            .padding(6)
                            .size(scale_sz(0.85) as f32)
                            .width(Length::Fill),
                        button(text("✕").size(scale_sz(0.85) as f32).bold().color(theme.danger))
                            .on_press(SettingsMessage::RemoveMusicDir(idx))
                            .padding([4, 8])
                            .style(move |_t, _s| button::Style {
                                background: Some(iced::Background::Color(theme.surface_alt)),
                                border: iced::Border { color: theme.danger, width: 1.0, radius: 4.0.into() },
                                ..Default::default()
                            }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
        }

        col = col.push(
            container(music_dirs_col)
                .padding([10, 14])
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                    border: iced::Border { color: theme.border, width: 1.5, radius: 6.0.into() },
                    ..Default::default()
                }),
        );

        col = col.push(
            container(
                column![
                    text("⬜ Global Overlay Dimensions")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    row![
                        text("Width:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        slider(
                            400..=2000,
                            settings.overlay_width,
                            SettingsMessage::OverlayWidth
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.overlay_width))
                            .size(scale_sz(0.85) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(62.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Height:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        slider(
                            250..=1500,
                            settings.overlay_height,
                            SettingsMessage::OverlayHeight
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.overlay_height))
                            .size(scale_sz(0.85) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(62.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Icon Size:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        slider(
                            48..=160,
                            settings.launcher_icon_size,
                            SettingsMessage::LauncherIconSize
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.launcher_icon_size))
                            .size(scale_sz(0.85) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(62.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Clipboard Thumb:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        slider(
                            32..=160,
                            settings.clipboard_thumbnail_size,
                            SettingsMessage::ClipboardThumbnailSize
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.clipboard_thumbnail_size))
                            .size(scale_sz(0.85) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(62.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        // ── Screenshot Save Format ─────────────────────────────
        let fmt_btn =
            |fmt: ScreenshotSaveFormat, label: &'static str| -> Element<'a, SettingsMessage> {
                let is_sel = settings.screenshot_save_format == fmt;
                button(text(label).size(scale_sz(0.85) as f32).bold().color(if is_sel {
                    theme.background
                } else {
                    theme.text
                }))
                .on_press(SettingsMessage::ScreenshotSaveFormat(fmt))
                .padding([3, 8])
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
                })
                .into()
            };

        col = col.push(
            container(
                column![
                    text("🖼 Screenshot Save Format")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    text("PNG saves in ~0.5s for 4K. JXL is 4-20x slower but 4-5x smaller.")
                        .size(scale_sz(0.85) as f32)
                        .color(theme.text),
                    row![
                        text("Format:").size(scale_sz(0.85) as f32).bold().color(theme.text),
                        fmt_btn(ScreenshotSaveFormat::Png, "PNG (fast, large)"),
                        fmt_btn(ScreenshotSaveFormat::JxlFast, "JXL (balanced)"),
                        fmt_btn(ScreenshotSaveFormat::JxlBest, "JXL (smallest)"),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        container(scrollable(col).height(Length::Fill))
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
