use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let muted = telemetry.mic.is_muted;
    let val_col = if muted { theme.danger } else { theme.text };
    let val_str = if muted {
        "MUTED".to_string()
    } else {
        format!("{}%", telemetry.mic.volume_pct)
    };
    let elem = label::<Message>(
        &[
            ("Mic:".to_string(), theme.success),
            (" ".to_string(), theme.success),
            (val_str, val_col),
        ],
        theme.font_size_normal,
    );
    let border = if muted { theme.danger } else { theme.border };
    (elem, CapsuleAction::ToggleSourceMute, border)
}
