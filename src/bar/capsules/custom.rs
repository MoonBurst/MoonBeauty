use iced::{Color, Element};
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let col = theme.text;
    let elem = label::<Message>(&[("Custom".to_string(), col)], theme.font_size_normal);
    (elem, CapsuleAction::OpenOverlay("custom".into()), col)
}

pub(super) fn fallback<'a, Message: 'a>(
    id: &str,
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let elem = label::<Message>(&[(id.to_uppercase(), theme.text)], theme.font_size_normal);
    (elem, CapsuleAction::OpenOverlay(id.to_string()), theme.border)
}
