use super::{label, CapsuleAction};
use crate::libraries::backend::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let now = chrono::Local::now().timestamp();
    let (txt, col) = match telemetry.next_alarm_epoch.filter(|&epoch| epoch > now) {
        Some(target_epoch) => {
            let diff = (target_epoch - now).max(0);
            let label = if diff >= 3600 {
                format!("⏰ {}h {}m", diff / 3600, (diff % 3600) / 60)
            } else if diff >= 60 {
                format!("⏰ {}m {}s", diff / 60, diff % 60)
            } else {
                format!("⏰ {}s", diff)
            };
            (label, theme.warning)
        }
        None => ("No Alarm".to_string(), theme.text),
    };
    let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
    (
        elem,
        CapsuleAction::OpenOverlay("alarm".into()),
        theme.border,
    )
}
