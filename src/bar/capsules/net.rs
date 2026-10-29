use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let down = if telemetry.net.down_str.is_empty() { "0.0Mb" } else { &telemetry.net.down_str };
    let up = if telemetry.net.up_str.is_empty() { "0.0Mb" } else { &telemetry.net.up_str };
    let ping = if telemetry.net.ping_str.is_empty() { "--" } else { &telemetry.net.ping_str };
    let ping_col = if ping.contains("OFFLIN") { theme.danger } else { theme.text };
    let elem = label::<Message>(
        &[
            ("NET:".to_string(), theme.success),
            (" ▼".to_string(), theme.text),
            (down.to_string(), theme.text),
            (" ▲".to_string(), theme.text),
            (up.to_string(), theme.text),
            (" ".to_string(), theme.text),
            (ping.to_string(), ping_col),
        ],
        theme.font_size_normal,
    );
    (elem, CapsuleAction::OpenOverlay("net".into()), theme.border)
}
