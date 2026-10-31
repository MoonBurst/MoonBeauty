use super::{label, CapsuleAction};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let (txt, col) = match &telemetry.weather {
        Some(w) => {
            let (prefix, col) = match &w.alert {
                crate::bar::weather::WeatherAlert::Danger(_) => ("🚨 ", theme.danger),
                crate::bar::weather::WeatherAlert::Upcoming(_) => ("⚠️ ", theme.warning),
                crate::bar::weather::WeatherAlert::None => ("", theme.text),
            };
            (format!("{}{}°F/{}°C", prefix, w.temp_f, w.temp_c), col)
        }
        None => ("--°F/--°C".into(), theme.text),
    };
    let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
    (elem, CapsuleAction::OpenOverlay("weather".into()), col)
}
