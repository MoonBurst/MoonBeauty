use super::*;
use iced::widget::column;

impl App {
    pub(super) fn render_music<'a>(
        &'a self,
        track_opt: &'a Option<MprisTrack>,
        mut col: iced::widget::Column<'a, Message>,
        y_ref: &mut f32,
        t: &ShellTheme,
        settings: &ShellSettings,
        total_h: f32,
        slant_h: f32,
        slant_right: bool,
        safe: f32,
        w_f: f32,
        alpha: f32,
    ) -> iced::widget::Column<'a, Message> {
        let mut y = *y_ref;
        let a = move |color: Color| Color { a: color.a * alpha, ..color };
        let text_col = a(t.text);
        let _muted = a(t.muted);
        let fs = settings.tooltip_font_size;
        let gap = 8.0_f32;
        let danger = a(t.danger);

        // Extra horizontal padding so widgets don't clip against the diagonal edges
        let safe = safe + 20.0;

        let (title, artist, _album, playing, pos, len, url, vol) = match track_opt {
            Some(tr) => (
                tr.title.clone(),
                tr.artist.clone(),
                tr.album.clone(),
                tr.is_playing,
                tr.position_sec,
                tr.length_sec,
                tr.url.clone(),
                tr.volume_pct,
            ),
            None => (
                "No Track".into(),
                "No Artist".into(),
                "".into(),
                false,
                0.0,
                0.0,
                None,
                100,
            ),
        };

        let pos_str = format!("{}:{:02}", (pos as u32) / 60, (pos as u32) % 60);
        let len_str = format!("{}:{:02}", (len as u32) / 60, (len as u32) % 60);

        let header_h = fs as f32 + 16.0;
        let header = row![
            text("MUSIC")
                .size((fs.max(14)) as f32)
                .bold()
                .color(text_col),
            iced::widget::space::horizontal(),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);
        col = col.push(slanted_row(
            header.into(),
            y + header_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            header_h,
        ));
        y += header_h + gap;

        // Track card
        let card_h = 80.0;
        let card_bg_col = a(t.background);
        let card_border_col = a(t.text);
        let card_text_bold = a(t.text);
        let card_text_muted = Color {
            a: a(t.text).a * 0.8,
            ..a(t.text)
        };

        let trunc_title = if title.chars().count() > 24 {
            let s: String = title.chars().take(21).collect();
            format!("{}...", s)
        } else {
            title.clone()
        };

        let card_el = column![
            text(trunc_title)
                .size((fs + 2) as f32)
                .bold()
                .color(card_text_bold)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .wrapping(iced::widget::text::Wrapping::None),
            text(artist)
                .size((fs) as f32)
                .color(card_text_muted)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .wrapping(iced::widget::text::Wrapping::None),
        ]
        .spacing(4);
        let card_shape = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: 16 })
            .background(card_bg_col)
            .border(card_border_col, 1.5)
            .size(Length::Fill, Length::Fill);
        let card_layer = Stack::new().push(card_shape.view()).push(
            container(card_el)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        );
        col = col.push(slanted_row(
            card_layer.into(),
            y + card_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            card_h,
        ));
        y += card_h + gap * 2.0;

        // Seek bar
        let seek_h = 20.0;
        let slider_fg = a(t.text);
        let slider_bg = a(t.muted);
        let seek_bar = row![
            text(pos_str).size((fs - 2) as f32).color(text_col),
            container(
                iced::widget::slider(0.0..=len.max(1.0), pos, Message::MusicSeek)
                    .style(move |_t, _s| iced::widget::slider::Style {
                        rail: iced::widget::slider::Rail {
                            backgrounds: (
                                iced::Background::Color(slider_fg),
                                iced::Background::Color(slider_bg)
                            ),
                            width: 2.0,
                            border: iced::Border::default(),
                        },
                        handle: iced::widget::slider::Handle {
                            shape: iced::widget::slider::HandleShape::Rectangle {
                                width: 10,
                                border_radius: 0.0.into()
                            },
                            background: iced::Background::Color(slider_fg),
                            border_color: slider_fg,
                            border_width: 0.0,
                        },
                    })
                    .width(Length::Fill)
            )
            .width(Length::Fill)
            .padding([0, 8]),
            text(len_str).size((fs - 2) as f32).color(text_col),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);
        col = col.push(slanted_row(
            seek_bar.into(),
            y + seek_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            seek_h,
        ));
        y += seek_h + gap;

        // Volume bar
        let vol_h = 20.0;
        let vol_bar = row![
            text("VOL").size((fs - 2) as f32).bold().color(text_col),
            container(
                iced::widget::slider(0..=100, vol, Message::MusicSetVolume)
                    .style(move |_t, _s| iced::widget::slider::Style {
                        rail: iced::widget::slider::Rail {
                            backgrounds: (
                                iced::Background::Color(slider_fg),
                                iced::Background::Color(slider_bg)
                            ),
                            width: 2.0,
                            border: iced::Border::default(),
                        },
                        handle: iced::widget::slider::Handle {
                            shape: iced::widget::slider::HandleShape::Rectangle {
                                width: 10,
                                border_radius: 0.0.into()
                            },
                            background: iced::Background::Color(slider_fg),
                            border_color: slider_fg,
                            border_width: 0.0,
                        },
                    })
                    .width(Length::Fill)
            )
            .width(Length::Fill)
            .padding([0, 8]),
            text(format!("{}%", vol))
                .size((fs - 2) as f32)
                .color(text_col),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);
        col = col.push(slanted_row(
            vol_bar.into(),
            y + vol_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            vol_h,
        ));
        y += vol_h + gap * 2.0;

        // Playback controls
        let ctrl_h = 32.0;
        let btn_fs = fs - 2;
        let text_col_l = a(t.text);
        let bg_col_l = a(t.background);

        let prev_btn = make_slanted_button(
            "< Prev".into(),
            Message::MusicPrev,
            bg_col_l,
            text_col_l,
            text_col_l,
            6.0,
            btn_fs,
        );
        let play_btn = if playing {
            make_slanted_button(
                "Pause".into(),
                Message::MusicPlayPause,
                text_col_l,
                bg_col_l,
                text_col_l,
                6.0,
                btn_fs,
            )
        } else {
            make_slanted_button(
                "Play".into(),
                Message::MusicPlayPause,
                bg_col_l,
                text_col_l,
                text_col_l,
                6.0,
                btn_fs,
            )
        };
        let next_btn = make_slanted_button(
            "Next >".into(),
            Message::MusicNext,
            bg_col_l,
            text_col_l,
            text_col_l,
            6.0,
            btn_fs,
        );
        let cycle_btn = make_slanted_button(
            "Cycle".into(),
            Message::CyclePlayer,
            bg_col_l,
            text_col_l,
            text_col_l,
            6.0,
            btn_fs,
        );
        let ctrl_row: Element<'_, Message> =
            row![prev_btn, play_btn, next_btn, cycle_btn]
                .spacing(6)
                .width(Length::Fill)
                .height(Length::Fixed(ctrl_h))
                .into();
        col = col.push(slanted_row(
            ctrl_row,
            y + ctrl_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            ctrl_h,
        ));
        y += ctrl_h + gap * 2.0;

        // Actions (folder / trash)
        let act_h = 28.0;
        let folder_btn = make_slanted_button(
            "Folder".into(),
            Message::MusicOpenFolder(url.clone()),
            bg_col_l,
            text_col_l,
            text_col_l,
            4.0,
            btn_fs,
        );
        let trash_btn = if self.trash_confirm {
            make_slanted_button(
                "Confirm?".into(),
                Message::MusicTrashExecute(url.clone()),
                bg_col_l,
                danger,
                danger,
                4.0,
                btn_fs,
            )
        } else {
            make_slanted_button(
                "Trash".into(),
                Message::MusicTrashConfirm,
                bg_col_l,
                text_col_l,
                text_col_l,
                4.0,
                btn_fs,
            )
        };
        let act_row: Element<'_, Message> = row![folder_btn, trash_btn]
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fixed(act_h))
            .into();
        col = col.push(slanted_row(
            act_row,
            y + act_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            act_h,
        ));
        y += act_h + gap * 2.0;
        // ── Search UI ──
        let search_h = 32.0;
        let search_input = text_input("Search MPD...", &self.music_search_query)
            .on_input(Message::MusicSearchChanged)
            .on_submit(Message::MusicSearchSubmit)
            .padding(6)
            .size((fs - 2) as f32)
            .style(move |_t, _s| iced::widget::text_input::Style {
                background: iced::Background::Color(bg_col_l),
                border: iced::Border {
                    color: text_col_l,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                icon: text_col_l,
                placeholder: text_col_l,
                value: text_col_l,
                selection: text_col_l,
            });

        col = col.push(slanted_row(
            search_input.into(),
            y + search_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            search_h,
        ));
        y += search_h + gap;

        if !self.music_search_results.is_empty() {
            let mut results_col = column![].spacing(2);
            for (i, (title, url)) in self.music_search_results.iter().enumerate() {
                let url_clone = url.clone();
                let is_sel = i == self.music_selected_idx;
                let bg = if is_sel { text_col_l } else { bg_col_l };
                let fg = if is_sel { bg_col_l } else { text_col_l };

                let btn = button(text(title).size((fs - 3) as f32).color(fg))
                    .on_press(Message::MusicPlaySong(url_clone))
                    .padding(4)
                    .width(Length::Fill)
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(bg)),
                        border: iced::Border {
                            color: text_col_l,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    });
                results_col = results_col.push(btn);
            }
            let scroll_results = scrollable(results_col)
                .id(iced::widget::Id::new("music_scroll"))
                .height(Length::Fixed(100.0));

            col = col.push(slanted_row(
                scroll_results.into(),
                y + 50.0,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                100.0,
            ));
            y += 100.0 + gap;
        }

        // Sources row
        let src_h = 36.0;
        let src_btn = |label: &'static str, idx: usize| -> Element<'_, Message> {
            let active = match idx {
                0 => self.music_sources.local,
                1 => self.music_sources.spotify,
                2 => self.music_sources.browser,
                _ => false,
            };
            let bg = if active { a(t.text) } else { a(t.background) };
            let fg = if active { a(t.background) } else { a(t.text) };
            let border = a(t.text);
            button(
                container(text(label).size((fs - 1) as f32).bold().color(fg))
                    .center_y(Length::Fill),
            )
            .on_press(Message::MusicToggleSource(idx))
            .padding([0, 16])
            .height(Length::Fill)
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(bg)),
                border: iced::Border {
                    color: border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                text_color: fg,
                ..Default::default()
            })
            .into()
        };

        let src_row = row![
            text("Sources:")
                .size((fs - 1) as f32)
                .bold()
                .color(text_col),
            iced::widget::space::horizontal(),
            src_btn("Local", 0),
            src_btn("Spotify", 1),
            src_btn("Browser", 2),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);
        col = col.push(slanted_row(
            src_row.into(),
            y + src_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            src_h,
        ));
        *y_ref = y;
        col
    }
}
