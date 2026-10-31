use super::{label, CapsuleAction};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let val_col = if telemetry.audio.is_muted {
        theme.danger
    } else {
        theme.text
    };
    let val_str = if telemetry.audio.is_muted {
        "Mute".to_string()
    } else {
        format!("{}%", telemetry.audio.volume_pct)
    };
    let elem = label::<Message>(
        &[
            ("Audio:".to_string(), theme.success),
            (" ".to_string(), theme.success),
            (val_str, val_col),
        ],
        theme.font_size_normal,
    );
    let border = if telemetry.audio.is_muted {
        theme.danger
    } else {
        theme.border
    };
    (elem, CapsuleAction::ToggleSinkMute, border)
}
