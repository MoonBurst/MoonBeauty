use super::*;

impl App {
    pub(super) fn render_calendar<'a>(
        &'a self,
        c: &'a crate::libraries::backend::CalendarViewData,
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

        // Compute exact content height from weeks
        let total_cells = c.first_day_offset as u32 + c.days_in_month;
        let weeks = total_cells.div_ceil(7);
        let _content_h = 40.0 + 36.0 + 24.0 + (weeks as f32) * 50.0 + 20.0;

        let header_h = fs as f32 + 16.0;
        let header = row![
            text("CALENDAR")
                .size((fs.max(14)) as f32)
                .bold()
                .color(text_col)
                .wrapping(iced::widget::text::Wrapping::None),
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

        // Month Label
        let month_h = fs as f32 + 12.0;
        let month_el = row![text(&c.month_label)
            .size((fs + 2) as f32)
            .bold()
            .color(text_col)
            .width(Length::Fill)
            .align_x(Alignment::Center),]
        .width(Length::Fill)
        .into();
        col = col.push(slanted_row(
            month_el,
            y + month_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            month_h,
        ));
        y += month_h + gap;

        // Grid setup
        let cell_h = fs as f32 + 8.0;
        // Fixed width for each of the 7 days so they don't collapse
        let cell_w = (w_f - (slant_h * 2.0) - safe * 2.0) / 7.0;

        // Days of week
        let dow_h = fs as f32 + 4.0;
        let mut dow_row = row![].spacing(0);
        for day_name in ["S", "M", "T", "W", "T", "F", "S"] {
            dow_row = dow_row.push(
                container(text(day_name).size(fs as f32).color(muted))
                    .width(Length::Fixed(cell_w))
                    .align_x(Alignment::Center),
            );
        }
        col = col.push(slanted_row(
            dow_row.into(),
            y + dow_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            dow_h,
        ));
        y += dow_h + gap;

        let mut day = 1;
        for _row in 0..6 {
            let mut row_widgets = row![].spacing(0);
            for col_idx in 0..7 {
                if _row == 0 && col_idx < c.first_day_offset {
                    row_widgets = row_widgets
                        .push(container(text("")).width(Length::Fixed(cell_w)));
                } else if day <= c.days_in_month {
                    let is_today = day == c.today_day;
                    let color = if is_today { a(t.danger) } else { text_col };
                    let bg_col = if is_today {
                        Some(iced::Background::Color(a(t.surface_alt)))
                    } else {
                        None
                    };
                    let border_col = if is_today {
                        a(t.danger)
                    } else {
                        Color::TRANSPARENT
                    };

                    let cell = container(
                        text(format!("{:2}", day)).size(fs as f32).color(color),
                    )
                    .width(Length::Fixed(cell_w))
                    .height(Length::Fixed(cell_h))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .style(move |_t| {
                        container::Style {
                            background: bg_col,
                            border: iced::Border {
                                color: border_col,
                                width: if is_today { 1.5 } else { 0.0 },
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }
                    });

                    row_widgets = row_widgets.push(cell);
                    day += 1;
                } else {
                    row_widgets = row_widgets
                        .push(container(text("")).width(Length::Fixed(cell_w)));
                }
            }
            col = col.push(slanted_row(
                row_widgets.into(),
                y + cell_h * 0.5,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                cell_h,
            ));
            y += cell_h + 2.0;
            if day > c.days_in_month {
                break;
            }
        }
        *y_ref = y;
        col
    }
}
