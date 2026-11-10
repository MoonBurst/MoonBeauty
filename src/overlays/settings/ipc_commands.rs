use super::*;
use iced::widget::column;

impl SettingsOverlay {
    pub(super) fn view_ipc_commands<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let fs_desc = scale_sz(0.72) as f32;
        let fs_cmd = scale_sz(0.75) as f32;
        let fs_head = scale_sz(0.85) as f32;

        // (title, [(description, command), ...])
        let sections: &[(&str, &[(&str, &str)])] = &[
            (
                "MUSIC",
                &[
                    (
                        "Cycle through active players (play, pause, next player, play, ...)",
                        "moonbeauty --music-cycle",
                    ),
                    (
                        "Play/pause the currently active player",
                        "moonbeauty --music-play-pause",
                    ),
                    (
                        "Skip to the next track on the active player",
                        "moonbeauty --music-next",
                    ),
                    (
                        "Go to the previous track on the active player",
                        "moonbeauty --music-previous",
                    ),
                ],
            ),
            (
                "OVERLAYS — each command toggles. Both --<target> and --toggle <target> work.",
                &[
                    ("Application launcher", "moonbeauty --launcher"),
                    ("Clipboard manager", "moonbeauty --clipboard"),
                    ("Notification history", "moonbeauty --history"),
                    ("Settings", "moonbeauty --settings"),
                    ("Calculator & units", "moonbeauty --calc"),
                    ("Password store", "moonbeauty --pass"),
                    ("Dictionary", "moonbeauty --dict"),
                    ("Unicode glyphs", "moonbeauty --unicode"),
                    ("Tasks & todos", "moonbeauty --todo"),
                    ("Reminders & alarms", "moonbeauty --reminders"),
                    ("Dice & RNG", "moonbeauty --rng"),
                    ("Web search", "moonbeauty --web"),
                    ("Email client", "moonbeauty --email"),
                    ("Among Us tracker", "moonbeauty --amogus"),
                    (
                        "Lock screen (must use --toggle; bare --lockscreen starts the coordinator)",
                        "moonbeauty --toggle lockscreen",
                    ),
                ],
            ),
            (
                "NOTIFICATIONS",
                &[
                    (
                        "Send a notification from a script or shell",
                        "moonbeauty --notify \"Title\" \"Body\"",
                    ),
                    (
                        "Focus the app that sent the newest toast",
                        "moonbeauty --activate-latest-notif",
                    ),
                    ("Dismiss the newest toast", "moonbeauty --dismiss-notif"),
                    (
                        "Toggle the notification history panel",
                        "moonbeauty --notif-history",
                    ),
                ],
            ),
            (
                "SCREENSHOTS",
                &[
                    (
                        "Capture a region, save to ~/Pictures/Screenshots",
                        "moonbeauty --screenshot",
                    ),
                    (
                        "Capture a region, open the annotator",
                        "moonbeauty --screenshot --annotate",
                    ),
                ],
            ),
            (
                "SYSTEM",
                &[
                    (
                        "Restart the bar (picks up new settings)",
                        "moonbeauty --restart-bar",
                    ),
                    ("Test IMAP/SMTP email connection", "moonbeauty --test-email"),
                    (
                        "Test lockscreen password authentication",
                        "moonbeauty --test-lock-auth",
                    ),
                ],
            ),
            (
                "SWAY KEYBIND EXAMPLES",
                &[
                    (
                        "Cycle music players with Super+M",
                        "bindsym $mod+m exec moonbeauty --music-cycle",
                    ),
                    (
                        "Play/pause with Super+Shift+M",
                        "bindsym $mod+Shift+m exec moonbeauty --music-play-pause",
                    ),
                    (
                        "Next track with Super+Shift+Right",
                        "bindsym $mod+Shift+Right exec moonbeauty --music-next",
                    ),
                    (
                        "Toggle notification history with Super+N",
                        "bindsym $mod+n exec moonbeauty --history",
                    ),
                    (
                        "Region screenshot with Print",
                        "bindsym Print exec moonbeauty --screenshot",
                    ),
                ],
            ),
        ];

        let mut col = column![].spacing(20).width(Length::Fill);

        col = col.push(
            container(
                text("Click 📋 to copy a command. Paste into a sway keybind, shell alias, or terminal.")
                    .size(fs_desc)
                    .color(theme.muted),
            )
            .padding([10, 14])
            .width(Length::Fill)
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(theme.surface_alt)),
                border: iced::Border {
                    color: theme.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        );

        for (title, entries) in sections {
            col = col.push(text(*title).size(fs_head).bold().color(theme.success));

            let mut section_col = column![].spacing(6).width(Length::Fill);
            for (desc, cmd) in entries.iter() {
                let cmd_clone = (*cmd).to_string();

                let desc_el = text(*desc).size(fs_desc).color(theme.muted);

                let cmd_el = text(*cmd)
                    .size(fs_cmd)
                    .font(iced::Font::MONOSPACE)
                    .color(theme.text)
                    .wrapping(iced::widget::text::Wrapping::None);

                let copy_btn = button(
                    container(text("📋").size(14))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .on_press(SettingsMessage::CopyIpc(cmd_clone))
                .padding(0)
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(32.0))
                .style(move |_t, status| button::Style {
                    background: Some(iced::Background::Color(match status {
                        button::Status::Hovered | button::Status::Pressed => theme.surface_alt,
                        _ => theme.background,
                    })),
                    border: iced::Border {
                        color: theme.text,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    text_color: theme.text,
                    ..Default::default()
                });

                let row = container(
                    row![
                        column![desc_el, cmd_el].spacing(2).width(Length::Fill),
                        copy_btn,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .width(Length::Fill),
                )
                .padding([6, 10])
                .width(Length::Fill)
                .style(move |_t| container::Style {
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });

                section_col = section_col.push(row);
            }
            col = col.push(section_col);
        }

        container(scrollable(col).height(Length::Fill).width(Length::Fill))
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
