use super::{label, CapsuleAction};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let display = match &telemetry.mpris {
        Some(m) if !m.title.is_empty() && !m.artist.is_empty() => {
            format!("{} - {}", m.artist, m.title)
        }
        Some(m) if !m.title.is_empty() => m.title.clone(),
        Some(m) if !m.artist.is_empty() => m.artist.clone(),
        _ => "No Track".into(),
    };
    let truncated: String = if display.chars().count() > 20 {
        let s: String = display.chars().take(20).collect();
        format!("{}…", s.trim())
    } else {
        display
    };
    let elem = label::<Message>(&[(truncated, theme.text)], theme.font_size_normal);
    (
        elem,
        CapsuleAction::OpenOverlay("music".into()),
        theme.border,
    )
}
