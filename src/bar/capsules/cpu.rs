use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let cpu_danger = telemetry.cpu.temp_c >= theme.cpu_temp_danger
        || u32::from(telemetry.cpu.usage_pct) >= theme.cpu_usage_danger;
    let cpu_warn = telemetry.cpu.temp_c >= theme.cpu_temp_warn
        || u32::from(telemetry.cpu.usage_pct) >= theme.cpu_usage_warn;
    let val_col = if cpu_danger { theme.danger } else if cpu_warn { theme.warning } else { theme.text };
    let border_col = if val_col == theme.text { theme.border } else { val_col };

    let trimmed = telemetry.cpu.usage_pct.to_string();
    let leading_zero_count = 3usize.saturating_sub(trimmed.len());
    let zero_pad: String = "0".repeat(leading_zero_count);

    let mut segments: Vec<(String, Color)> = vec![
        ("CPU:".to_string(), theme.success),
        (" ".to_string(), theme.success),
    ];
    if !zero_pad.is_empty() {
        segments.push((zero_pad, theme.background));
    }
    segments.push((format!("{}% {}°C", trimmed, telemetry.cpu.temp_c), val_col));
    let elem = label::<Message>(&segments, theme.font_size_normal);
    (elem, CapsuleAction::OpenOverlay("cpu".into()), border_col)
}
