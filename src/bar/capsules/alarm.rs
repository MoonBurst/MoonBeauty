use super::{label, CapsuleAction};
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};
use std::sync::{Arc, Mutex, OnceLock};

// Fix 2: cache reminders with mtime check. TopBar::view runs at 2-5 Hz
// baseline, up to 60 Hz during tray animation; each render previously
// did file I/O + JSON parse (RemindersOverlay::load). With mtime caching
// we stat() (1 syscall) and only re-parse on change.
struct RemindersCache {
    mtime: Option<std::time::SystemTime>,
    entries: Arc<Vec<crate::overlays::reminders::ReminderEntry>>,
}

static REMINDERS_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
static REMINDERS_CACHE: Mutex<Option<RemindersCache>> = Mutex::new(None);

fn reminders_path() -> &'static std::path::Path {
    REMINDERS_PATH.get_or_init(|| crate::overlays::reminders::RemindersOverlay::storage_path())
}

fn load_cached() -> Arc<Vec<crate::overlays::reminders::ReminderEntry>> {
    let path = reminders_path();
    let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
    let mut g = REMINDERS_CACHE.lock().unwrap();
    if let Some(c) = g.as_ref() {
        if c.mtime == mtime {
            return Arc::clone(&c.entries);
        }
    }
    let entries = Arc::new(crate::overlays::reminders::RemindersOverlay::load());
    *g = Some(RemindersCache {
        mtime,
        entries: Arc::clone(&entries),
    });
    entries
}

pub(super) fn render<'a, Message: 'a>(
    theme: &ShellTheme,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let entries = load_cached();
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
    (
        elem,
        CapsuleAction::OpenOverlay("alarm".into()),
        theme.border,
    )
}
