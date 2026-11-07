use super::*;

impl App {
    pub(super) fn render_unified<'a>(
        &'a self,
        podman: &'a PodmanStatus,
        borg: &'a BorgStatus,
        custom_items: &'a Vec<crate::bar::unified::UnifiedItemStatus>,
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
        let danger = a(t.danger);
        let warning = a(t.warning);
        let success = a(t.success);

        let header_h = fs as f32 + 16.0;
        let header = row![
            text("SYSTEM")
                .size((fs.max(14)) as f32)
                .bold()
                .color(text_col),
            iced::widget::space::horizontal(),
            mk_close(),
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

        // Containers
        if !podman.containers.is_empty() {
            let title_h = fs as f32 + 4.0;
            let title_el: Element<'_, Message> = text("Containers")
                .size((fs - 2) as f32)
                .bold()
                .color(muted)
                .into();
            col = col.push(slanted_row(
                title_el,
                y + title_h * 0.5,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                title_h,
            ));
            y += title_h + gap;

            for c in podman.containers.iter().take(6) {
                let row_h = fs as f32 + 12.0;
                let dot = if c.is_running { "●" } else { "○" };
                let dot_color = if c.is_running { success } else { muted };
                let cname = c.name.clone();

                let restart_btn = make_slanted_button(
                    "↻".into(),
                    Message::PodmanRestart(cname.clone()),
                    a(t.background),
                    a(t.text),
                    a(t.text),
                    4.0,
                    fs - 3,
                );

                let row_el = row![
                    text(dot).size((fs) as f32).color(dot_color),
                    text(&c.name)
                        .size((fs - 1) as f32)
                        .color(text_col)
                        .width(Length::Fill),
                    text(&c.status).size((fs - 3) as f32).color(muted),
                    container(restart_btn)
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(row_h - 2.0)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::Fill);

                col = col.push(slanted_row(
                    row_el.into(),
                    y + row_h * 0.5,
                    total_h,
                    slant_h,
                    slant_right,
                    safe,
                    w_f,
                    row_h,
                ));
                y += row_h + gap;
            }
        }

        // Custom items
        if !custom_items.is_empty() {
            let title_h = fs as f32 + 4.0;
            let title_el: Element<'_, Message> = text("Custom Items")
                .size((fs - 2) as f32)
                .bold()
                .color(muted)
                .into();
            col = col.push(slanted_row(
                title_el,
                y + title_h * 0.5,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                title_h,
            ));
            y += title_h + gap;

            for (idx, item) in custom_items.iter().enumerate() {
                let row_h = fs as f32 + 12.0;
                let dot = if item.is_ok { "●" } else { "○" };
                let dot_color = if item.is_ok { success } else { danger };

                let action_el: Element<'_, Message> = if item.has_toggle {
                    make_slanted_button(
                        "↻".into(),
                        Message::UnifiedToggle(idx),
                        a(t.background),
                        a(t.text),
                        a(t.text),
                        4.0,
                        fs - 3,
                    )
                } else {
                    container(text("")).width(Length::Fixed(28.0)).into()
                };

                let msg_str = if item.message.is_empty() {
                    String::new()
                } else {
                    let truncated: String = item.message.chars().take(18).collect();
                    format!(" ({})", truncated)
                };

                let row_el = row![
                    text(dot).size((fs) as f32).color(dot_color),
                    text(&item.name)
                        .size((fs - 1) as f32)
                        .color(text_col)
                        .width(Length::Fill),
                    text(msg_str).size((fs - 3) as f32).color(muted),
                    container(action_el)
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(row_h - 2.0)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::Fill);

                col = col.push(slanted_row(
                    row_el.into(),
                    y + row_h * 0.5,
                    total_h,
                    slant_h,
                    slant_right,
                    safe,
                    w_f,
                    row_h,
                ));
                y += row_h + gap;
            }
        }

        if podman.containers.is_empty() && custom_items.is_empty() {
            let msg_h = fs as f32 + 8.0;
            let msg: Element<'_, Message> =
                text("(no containers or custom items — add via Settings)")
                    .size((fs - 2) as f32)
                    .color(muted)
                    .into();
            col = col.push(slanted_row(
                msg,
                y + msg_h * 0.5,
                total_h,
                slant_h,
                slant_right,
                safe,
                w_f,
                msg_h,
            ));
            y += msg_h + gap;
        }

        // Borg — three states: Failed (red) > Running (yellow) > Normal (green)
        let title_h = fs as f32 + 4.0;
        let title_el: Element<'_, Message> = text("Borg")
            .size((fs - 2) as f32)
            .bold()
            .color(muted)
            .into();
        col = col.push(slanted_row(
            title_el,
            y + title_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            title_h,
        ));
        y += title_h + gap;

        let borg_h = fs as f32 + 12.0;
        let (borg_dot_color, borg_label) = if borg.is_failed {
            let msg = borg
                .details
                .first()
                .cloned()
                .unwrap_or_else(|| "Failed".to_string());
            (danger, msg)
        } else if borg.is_active {
            let status = if borg.percent > 0 {
                format!("Running ({}%)", borg.percent)
            } else {
                "Running".to_string()
            };
            (warning, status)
        } else if borg.is_mounted {
            (success, "Mounted".to_string())
        } else {
            (success, "Idle (Synced)".to_string())
        };

        let borg_el = row![
            text("●").size((fs) as f32).color(borg_dot_color),
            text(borg_label)
                .size((fs - 1) as f32)
                .color(text_col)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();
        col = col.push(slanted_row(
            borg_el,
            y + borg_h * 0.5,
            total_h,
            slant_h,
            slant_right,
            safe,
            w_f,
            borg_h,
        ));
        *y_ref = y;
        col
    }
}
