use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let (txt, col) = if telemetry.podman.is_rate_limited {
        ("Miner: Limited!".to_string(), theme.warning)
    } else if telemetry.borg.is_active && !telemetry.borg.state_label.is_empty() {
        (telemetry.borg.state_label.clone(), theme.success)
    } else if let Some(w) = telemetry
        .podman
        .watchers
        .iter()
        .find(|w| w.is_running && !w.streamer.is_empty())
    {
        (format!("📺 {}", w.streamer), theme.success)
    } else if !telemetry.podman.summary_label.is_empty() {
        (telemetry.podman.summary_label.clone(), theme.success)
    } else {
        ("Sys OK".to_string(), theme.success)
    };
    let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
    (elem, CapsuleAction::OpenOverlay("unified".into()), theme.border)
}
