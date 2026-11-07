use super::*;

pub(super) fn render_net_proc_row<'a>(
    p: &'a ProcessItem,
    t: &ShellTheme,
    font_size: u16,
    alpha: f32,
    row_w: f32,
    row_h: f32,
    slant_right: bool,
    tooltip_slope: f32,
) -> Element<'a, Message> {
    let apply = |c: Color| Color {
        a: c.a * alpha,
        ..c
    };
    let name = if p.name.chars().count() > 22 {
        let mut s: String = p.name.chars().take(19).collect();
        s.push_str("...");
        s
    } else {
        p.name.clone()
    };
    let pid = p.pid;
    let muted = apply(t.muted);
    let danger = apply(t.danger);
    let text_col = apply(t.text);
    let surface_alt = apply(t.surface_alt);

    let metric_str = &p.metric;

    let kill_w = 32.0;
    let kill_h = 24.0;
    let kill_border = t.control_border_width;
    let half_k = kill_border * 0.5;
    let x_slant = tooltip_slope * (kill_h - 2.0 * half_k);
    let metric_w = 180.0;
    let spacing = 8.0;

    let kill_shape = KillButtonShape {
        w: kill_w,
        h: kill_h,
        slant: x_slant,
        slant_right,
        background: surface_alt,
        border_color: danger,
        border_width: kill_border,
        x_color: danger,
    };

    let kill_canvas: Element<'a, Message> = Canvas::new(kill_shape)
        .width(Length::Fixed(kill_w))
        .height(Length::Fixed(kill_h))
        .into();

    let kill_btn = button(kill_canvas)
        .on_press(Message::KillProcess(pid))
        .padding(0)
        .width(Length::Fixed(kill_w))
        .height(Length::Fixed(row_h))
        .style(|_t, _s| button::Style {
            background: None,
            ..Default::default()
        });

    let name_clip = container(
        text(name)
            .size((font_size.saturating_sub(4).max(9)) as f32)
            .color(text_col)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(Length::Fixed(row_h))
    .align_y(Alignment::Center)
    .clip(true);

    let metric_container = container(
        text(metric_str)
            .size((font_size.saturating_sub(4).max(9)) as f32)
            .color(muted)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fixed(metric_w))
    .height(Length::Fixed(row_h))
    .align_y(Alignment::Center)
    .padding([0.0, 6.0])
    .clip(true);

    row![name_clip, metric_container, kill_btn,]
        .spacing(spacing)
        .align_y(Alignment::Center)
        .width(Length::Fixed(row_w))
        .into()
}
