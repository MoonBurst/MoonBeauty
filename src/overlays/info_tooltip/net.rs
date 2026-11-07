use super::*;
use super::net_proc::render_net_proc_row;
use iced::widget::column;

impl App {
    pub(super) fn render_net<'a>(
        &'a self,
        net: &'a crate::libraries::backend::NetMetrics,
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
        tooltip_slope: f32,
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

        let header = row![
            text("NETWORK").size((fs) as f32).bold().color(text_col),
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

        let stats_h = fs as f32 + 8.0;
        let stats_el = row![
            text(if net.iface.is_empty() {
                "—"
            } else {
                &net.iface
            })
            .size((fs) as f32)
            .color(text_col),
            text(" ▼ ")
                .size((fs.saturating_sub(2)) as f32)
                .color(success),
            text(&net.down_str).size((fs) as f32).bold().color(text_col),
            text(" ▲ ")
                .size((fs.saturating_sub(2)) as f32)
                .color(success),
            text(&net.up_str).size((fs) as f32).bold().color(text_col),
        ]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();
        col = col.push(slanted_row(
            stats_el,
            y + stats_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            stats_h,
        ));
        y += stats_h + gap;

        // Search Bar
        let search_h = 40.0;
        let y_mid_search = y + search_h * 0.5;
        let frac_s = (y_mid_search / total_h.max(1.0)).clamp(0.0, 1.0);
        let (lp_s, rp_s) = if slant_right {
            (slant_h * (1.0 - frac_s) + safe, slant_h * frac_s + safe)
        } else {
            (slant_h * frac_s + safe, slant_h * (1.0 - frac_s) + safe)
        };
        let search_w = (w_f - lp_s - rp_s).max(100.0);
        let search_border = settings.control_border_width;
        let half_s = search_border * 0.5;
        let search_slant = tooltip_slope * (search_h - 2.0 * half_s);

        let search_bg = crate::overlays::tooltip_engine::ParallelogramBg {
            w: search_w,
            h: search_h,
            slant: search_slant,
            slant_right,
            background: a(t.surface_alt),
            border_color: a(t.text),
            border_width: search_border,
            frame: self.opened.elapsed().as_millis() as u64,
        };

        let input_muted = Color { a: 1.0, ..t.muted };
        let input_text = Color { a: 1.0, ..t.text };
        let input_focus = Color { a: 1.0, ..t.focus };
        let search_input = text_input("Search programs...", &self.search_query)
            .on_input(Message::SearchChanged)
            .padding(Padding {
                top: 8.0,
                bottom: 8.0,
                left: search_slant + 8.0,
                right: search_slant + 8.0,
            })
            .size((fs) as f32)
            .width(Length::Fill)
            .style(move |_t, _s| iced::widget::text_input::Style {
                background: iced::Background::Color(Color::TRANSPARENT),
                border: iced::Border::default(),
                icon: input_muted,
                placeholder: input_muted,
                value: input_text,
                selection: input_focus,
            });
        let search_layer: Element<'a, Message> = Stack::new()
            .push(search_bg.view())
            .push(
                container(search_input)
                    .width(Length::Fixed(search_w))
                    .height(Length::Fixed(search_h))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .width(Length::Fixed(search_w))
            .height(Length::Fixed(search_h))
            .into();

        col = col.push(slanted_row(
            search_layer,
            y_mid_search,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            search_h,
        ));
        y += search_h + gap;

        // Process List
        let proc_h = fs as f32 + 10.0;
        let proc_gap = 2.0;
        let mut procs_col = column![].spacing(proc_gap).width(Length::Fill);
        let q = self.search_query.to_lowercase();
        let items: Vec<&ProcessItem> = net
            .top_procs
            .iter()
            .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
            .collect();
        for &p in items.iter().take(10) {
            let inner_w = w_f
                - (slant_h * (1.0 - (y + proc_h * 0.5) / total_h.max(1.0)) + safe)
                - (slant_h * ((y + proc_h * 0.5) / total_h.max(1.0)) + safe);
            let proc_row = render_net_proc_row(
                p,
                t,
                fs,
                alpha,
                inner_w,
                proc_h,
                slant_right,
                tooltip_slope,
            );
            procs_col = procs_col.push(slanted_row(
                proc_row,
                y + proc_h * 0.5,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                proc_h,
            ));
            y += proc_h + proc_gap;
        }
        if !items.is_empty() {
            col = col.push(procs_col);
        } else {
            let empty_el: Element<'a, Message> = container(
                text(if net.top_procs.is_empty() {
                    "(no active network procs)"
                } else {
                    "(no match)"
                })
                .size((fs.saturating_sub(4).max(9)) as f32)
                .color(muted)
                .wrapping(iced::widget::text::Wrapping::None),
            )
            .into();
            col = col.push(slanted_row(
                empty_el,
                y + 10.0,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                20.0,
            ));
        }
        *y_ref = y;
        col
    }
}
