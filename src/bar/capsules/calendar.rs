use super::{label, CapsuleAction};
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let date_str = chrono::Local::now().format("%a, %b %d").to_string();
    let elem = label::<Message>(&[(date_str, theme.text)], theme.font_size_normal);
    (
        elem,
        CapsuleAction::OpenOverlay("calendar".into()),
        theme.border,
    )
}
