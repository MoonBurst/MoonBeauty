use super::*;

impl App {
    pub(super) fn render_custom<'a>(
        &'a self,
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

        let header = row![
            text("CUSTOM MODULE")
                .size((fs) as f32)
                .bold()
                .color(text_col),
            iced::widget::space::horizontal(),
            mk_close(),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill);
        col = col.push(slanted_row(
            header.into(),
            y + 16.0,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            32.0,
        ));
        y += 32.0 + gap;
        let msg = text("Configure in Settings → Bar Layout")
            .size((fs) as f32)
            .color(muted)
            .into();
        col = col.push(slanted_row(
            msg,
            y + 10.0,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            20.0,
        ));
        *y_ref = y;
        col
    }
}
