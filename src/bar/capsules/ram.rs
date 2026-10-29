use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let (display, _total) = if telemetry.ram.effective_avail_gib > 0.0 {
        (
            telemetry.ram.effective_avail_gib,
            telemetry.ram.total_gib + telemetry.ram.zram_saved_gib,
        )
    } else {
        (telemetry.ram.available_gib, telemetry.ram.total_gib)
    };
    let warn_gib = theme.ram_warn_gib as f32;
    let danger_gib = theme.ram_danger_gib as f32;
    let val_col = if display <= danger_gib {
        theme.danger
    } else if display <= warn_gib {
        theme.warning
    } else {
        theme.text
    };

    let mut segments: Vec<(String, Color)> = vec![("RAM:".to_string(), theme.success)];
    if display == 0.0 {
        segments.push((" -- GiB".to_string(), val_col));
    } else {
        let num_str = format!("{:.1}", display);
        let (int_part, frac_part) = num_str.split_once('.').unwrap_or((num_str.as_str(), ""));
        let pad_count = 2usize.saturating_sub(int_part.len());
        segments.push((" ".to_string(), val_col));
        if pad_count > 0 {
            segments.push(("0".repeat(pad_count), theme.background));
        }
        segments.push((format!("{}.{} GiB", int_part, frac_part), val_col));
    }
    let elem = label::<Message>(&segments, theme.font_size_normal);
    let border_col = if display <= danger_gib {
        theme.danger
    } else if display <= warn_gib {
        theme.warning
    } else {
        theme.border
    };
    (elem, CapsuleAction::OpenOverlay("ram".into()), border_col)
}
