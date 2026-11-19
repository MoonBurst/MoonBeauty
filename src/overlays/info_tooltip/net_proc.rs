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
    crate::overlays::tooltip_common::proc_kill_row(
        crate::overlays::tooltip_common::truncate_proc_name(&p.name),
        p.metric.clone(),
        180.0,
        Message::KillProcess(p.pid),
        t, font_size, alpha, row_w, row_h, slant_right, tooltip_slope,
    )
}
