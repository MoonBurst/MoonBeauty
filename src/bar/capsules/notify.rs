use super::{label, CapsuleAction};
use crate::libraries::backend::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let paused = telemetry.notify_paused;
    let total_count = telemetry.notify_count;
    let base08_col = theme.danger;
    let text_col = if paused { base08_col } else { theme.text };
    let border_col = if paused { base08_col } else { theme.border };

    let elem = label::<Message>(
        &[
            ("Notifications:".to_string(), text_col),
            (format!(" {}", total_count), text_col),
        ],
        theme.font_size_normal,
    );
    (elem, CapsuleAction::ToggleNotificationDnd, border_col)
}
