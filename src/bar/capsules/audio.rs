use super::{label, CapsuleAction};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

/// Shared audio/mic capsule render. Sink and source differ only in the
/// label, muted-string, action, and which `AudioStatus` field to read.
fn render_status<'a, Message: 'a>(
    label_text: &'static str,
    volume_pct: u8,
    is_muted: bool,
    mute_str: &'static str,
    action: CapsuleAction,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let val_col = if is_muted { theme.danger } else { theme.text };
    let val_str = if is_muted {
        mute_str.to_string()
    } else {
        format!("{}%", volume_pct)
    };
    let elem = label::<Message>(
        &[
            (label_text.to_string(), theme.success),
            (" ".to_string(), theme.success),
            (val_str, val_col),
        ],
        theme.font_size_normal,
    );
    let border = if is_muted { theme.danger } else { theme.border };
    (elem, action, border)
}

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    render_status(
        "Audio:",
        telemetry.audio.volume_pct,
        telemetry.audio.is_muted,
        "Mute",
        CapsuleAction::ToggleSinkMute,
        theme,
    )
}

pub(super) fn render_mic<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    render_status(
        "Mic:",
        telemetry.mic.volume_pct,
        telemetry.mic.is_muted,
        "MUTED",
        CapsuleAction::ToggleSourceMute,
        theme,
    )
}
