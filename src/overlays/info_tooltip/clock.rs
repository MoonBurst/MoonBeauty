use super::*;
use iced::widget::column;
use crate::overlays::tooltip_common::octagon_row;

impl App {
    pub(super) fn render_clock<'a>(
        &'a self,
        cols: &'a Vec<crate::libraries::backend::WorldZoneColumn>,
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
        let muted = a(t.muted);
        let fs = settings.tooltip_font_size;
        let gap = 8.0_f32;
        let mk_close = || -> Element<'a, Message> {
            iced::widget::container(iced::widget::Space::new())
                .width(Length::Fixed(0.0))
                .height(Length::Fixed(0.0))
                .into()
        };
        let success = a(t.success);

        let header_h = fs as f32 + 16.0;
        let header = row![
            text("WORLD CLOCK")
                .size((fs.max(14)) as f32)
                .bold()
                .color(text_col),
            iced::widget::space::horizontal(),
            mk_close(),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);
        let cut_f = (w_f.min(total_h) * 0.15).max(0.0);
        col = col.push(octagon_row(
            header.into(),
            y + header_h * 0.5,
            total_h,
            cut_f,
            safe,
            w_f,
            header_h,
        ));
        y += header_h + gap;

        let mut grid_row = row![].spacing(12).width(Length::Fill);
        for c in cols {
            let mut col_widget = column![].spacing(6).width(Length::FillPortion(1));
            col_widget = col_widget.push(
                text(c.title)
                    .size((fs - 1) as f32)
                    .bold()
                    .color(muted)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            );
            for (name, code, time, is_me) in &c.zones {
                let t_color = if *is_me { success } else { text_col };
                let is_me_val = *is_me;
                let bg = if is_me_val {
                    Some(iced::Background::Color(a(t.surface_alt)))
                } else {
                    None
                };
                let border_c = if is_me_val {
                    success
                } else {
                    Color::TRANSPARENT
                };

                let item = container(
                    row![
                        column![
                            text(*name)
                                .size((fs - 2) as f32)
                                .bold()
                                .color(t_color)
                                .wrapping(iced::widget::text::Wrapping::None),
                            text(*code)
                                .size((fs - 3) as f32)
                                .color(muted)
                                .wrapping(iced::widget::text::Wrapping::None),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        text(time)
                            .size((fs + 1) as f32)
                            .bold()
                            .color(t_color)
                            .wrapping(iced::widget::text::Wrapping::None),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([4, 8])
                .width(Length::Fill)
                .style(move |_t| container::Style {
                    background: bg,
                    border: iced::Border {
                        color: border_c,
                        width: if is_me_val { 1.0 } else { 0.0 },
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });
                col_widget = col_widget.push(item);
            }
            grid_row = grid_row.push(col_widget);
        }

        let grid_h = 5.0 * 48.0;
        col = col.push(octagon_row(
            grid_row.into(),
            y + grid_h * 0.5,
            total_h,
            cut_f,
            safe,
            w_f,
            grid_h,
        ));
        *y_ref = y;
        col
    }
}
