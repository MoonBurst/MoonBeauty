use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    if !telemetry.battery.has_battery {
        let elem = label::<Message>(
            &[
                ("⚡ AC:".to_string(), theme.text),
                (" Online".to_string(), theme.success),
            ],
            theme.font_size_normal,
        );
        return (elem, CapsuleAction::OpenOverlay("power".into()), theme.border);
    }
    let pct = telemetry.battery.percent;
    let status = telemetry.battery.status.to_lowercase();
    let icon = if status.contains("charging") {
        "⚡ "
    } else if pct <= 20 {
        "🪫 "
    } else {
        "🔋 "
    };
    let val_col = if status.contains("charging") || status.contains("full") || status.contains("mains") {
        theme.success
    } else if pct <= 20 {
        theme.danger
    } else if pct <= 40 {
        theme.warning
    } else {
        theme.text
    };
    let elem = label::<Message>(
        &[
            (format!("{}BAT:", icon), theme.text),
            (format!(" {}% ({:.1}W)", pct, telemetry.battery.power_w), val_col),
        ],
        theme.font_size_normal,
    );
    (elem, CapsuleAction::OpenOverlay("power".into()), val_col)
}
