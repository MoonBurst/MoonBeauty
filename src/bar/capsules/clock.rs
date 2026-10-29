use iced::{Color, Element};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let elem = label::<Message>(
        &[(telemetry.bar_time.clone(), theme.text)],
        theme.font_size_normal,
    );
    (elem, CapsuleAction::OpenOverlay("clock".into()), theme.border)
}
