use iced::{Color, Element};
use crate::libraries::config::theme::ShellTheme;
use super::{label, CapsuleAction};

pub(super) fn render<'a, Message: 'a>(
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let entries = crate::overlays::reminders::RemindersOverlay::load();
    let now = chrono::Local::now().timestamp();
    let active: Vec<_> = entries.iter().filter(|e| e.target_epoch > now).collect();
    let (txt, col) = if let Some(first) = active.iter().min_by_key(|e| e.target_epoch) {
        let diff = (first.target_epoch - now).max(0);
        let label = if diff >= 3600 {
            format!("⏰ {}h {}m", diff / 3600, (diff % 3600) / 60)
        } else if diff >= 60 {
            format!("⏰ {}m {}s", diff / 60, diff % 60)
        } else {
            format!("⏰ {}s", diff)
        };
        (label, theme.warning)
    } else {
        ("No Alarm".to_string(), theme.text)
    };
    let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
    (elem, CapsuleAction::OpenOverlay("alarm".into()), theme.border)
}
