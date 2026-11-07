use super::{label, CapsuleAction};
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let paused = crate::overlays::notifications::dnd_active();
    let held_count = crate::overlays::notifications::held_notifs_count();
    let active_count = crate::overlays::notifications::read_active_count();
    let total_count = held_count + active_count;
    // Fix 5: theme.danger is already hex_to_color(&palette.base08) —
    // no need to re-parse the hex string per frame.
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
