use super::*;

impl MoonBeautyShellApp {
    pub fn view(&self) -> Element<'_, Message> {
        if self.is_notif_surface {
            let is_right_corner = self.settings.notif_corner.contains("right");
            let is_top_corner = self.settings.notif_corner.contains("top");
            let pad_top = if is_top_corner {
                self.settings.notif_baseline_y as f32
            } else {
                0.0
            };
            let pad_bottom = if !is_top_corner {
                self.settings.notif_baseline_y as f32
            } else {
                0.0
            };
            let pad_right = if is_right_corner {
                self.settings.notif_margin_x.max(16) as f32
            } else {
                0.0
            };
            let pad_left = if !is_right_corner {
                self.settings.notif_margin_x.max(16) as f32
            } else {
                0.0
            };
            return container(
                self.notifs
                    .view_toasts(&self.theme, &self.settings, self.window_w)
                    .map(Message::Notif),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(if is_right_corner {
                iced::alignment::Horizontal::Right
            } else {
                iced::alignment::Horizontal::Left
            })
            .align_y(if is_top_corner {
                iced::alignment::Vertical::Top
            } else {
                iced::alignment::Vertical::Bottom
            })
            .padding(Padding {
                top: pad_top,
                right: pad_right,
                bottom: pad_bottom,
                left: pad_left,
            })
            .into();
        }
        let is_right_corner = self.settings.notif_corner.contains("right");
        let is_top_corner = self.settings.notif_corner.contains("top");

        let pad_top = if is_top_corner {
            self.settings.notif_baseline_y as f32
        } else {
            0.0
        };
        let pad_bottom = if !is_top_corner {
            self.settings.notif_baseline_y as f32
        } else {
            0.0
        };
        let pad_right = if is_right_corner {
            self.settings.notif_margin_x.max(16) as f32
        } else {
            0.0
        };
        let pad_left = if !is_right_corner {
            self.settings.notif_margin_x.max(16) as f32
        } else {
            0.0
        };

        let notifs_view = container(
            self.notifs
                .view_toasts(&self.theme, &self.settings, self.window_w)
                .map(Message::Notif),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(if is_right_corner {
            iced::alignment::Horizontal::Right
        } else {
            iced::alignment::Horizontal::Left
        })
        .align_y(if is_top_corner {
            iced::alignment::Vertical::Top
        } else {
            iced::alignment::Vertical::Bottom
        })
        .padding(Padding {
            top: pad_top,
            right: pad_right,
            bottom: pad_bottom,
            left: pad_left,
        })
        .style(|_t| iced::widget::container::Style {
            background: None,
            ..Default::default()
        });

        let overlay_element: Option<Element<'_, Message>> = match &self.active_overlay {
            Some(OverlayTarget::Launcher) => Some(
                self.launcher
                    .view(&self.theme, &self.settings)
                    .map(Message::Launcher),
            ),
            Some(OverlayTarget::Calc) => Some(
                self.calc
                    .view(&self.theme, &self.settings)
                    .map(Message::Calc),
            ),
            Some(OverlayTarget::Clipboard) => Some(
                self.clipboard
                    .view(&self.theme, &self.settings)
                    .map(Message::Clipboard),
            ),
            Some(OverlayTarget::Notes) => Some(
                self.notes
                    .view(&self.theme, &self.settings)
                    .map(Message::Notes),
            ),
            Some(OverlayTarget::Pass) => Some(
                self.pass
                    .view(&self.theme, &self.settings)
                    .map(Message::Pass),
            ),
            Some(OverlayTarget::Power) => Some(self.power.view(&self.theme).map(Message::Power)),
            Some(OverlayTarget::Todo) => Some(
                self.todo
                    .view(&self.theme, &self.settings)
                    .map(Message::Todo),
            ),
            Some(OverlayTarget::Reminders) => Some(
                self.reminders
                    .view(&self.theme, &self.settings)
                    .map(Message::Reminders),
            ),
            Some(OverlayTarget::Dictionary) => Some(
                self.dict
                    .view(&self.theme, &self.settings)
                    .map(Message::Dict),
            ),
            Some(OverlayTarget::Unicode) => Some(
                self.unicode
                    .view(&self.theme, &self.settings)
                    .map(Message::Unicode),
            ),
            Some(OverlayTarget::Web) => Some(self.web.view(&self.theme).map(Message::Web)),
            Some(OverlayTarget::Amogus) => Some(
                self.amogus
                    .view(&self.theme, &self.settings)
                    .map(Message::Amogus),
            ),
            Some(OverlayTarget::Rng) => {
                Some(self.rng.view(&self.theme, &self.settings).map(Message::Rng))
            }
            Some(OverlayTarget::Email) => Some(
                self.email
                    .view(&self.theme, &self.settings)
                    .map(Message::Email),
            ),
            Some(OverlayTarget::NotificationsHistory) => Some(
                self.notifs
                    .view_history(&self.theme, &self.settings)
                    .map(Message::Notif),
            ),
            Some(OverlayTarget::Settings) => Some(
                self.settings_ui
                    .view(&self.settings, &self.theme)
                    .map(Message::Settings),
            ),
            Some(OverlayTarget::Lockscreen) => {
                Some(self.lockscreen.view(&self.theme).map(Message::Lock))
            }
            Some(OverlayTarget::Annotator) => Some(self.annotator.view().map(Message::Annotator)),
            None => None,
        };

        if self.is_standalone {
            if let Some(overlay) = overlay_element {
                return Stack::new().push(overlay).push(notifs_view).into();
            } else {
                eprintln!("[FATAL-EXIT] is_standalone=true but overlay_element=None");
                eprintln!("[FATAL-EXIT] active_overlay = {:?}", self.active_overlay);
                eprintln!("[FATAL-EXIT] is_notif_surface = {}", self.is_notif_surface);
                std::process::exit(0);
            }
        }

        let bar_view = TopBar::view(
            &self.settings,
            &self.theme,
            &self.telemetry,
            self.tray_expanded,
            &self.tray_items,
            self.tray_anim_width,
        )
        .map(Message::TopBar);

        let base_content: Element<'_, Message> = bar_view;

        if let Some(overlay) = overlay_element {
            Stack::new().push(base_content).push(overlay).into()
        } else {
            base_content
        }
    }
}
