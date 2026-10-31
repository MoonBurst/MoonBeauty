pub mod amogus;
pub mod calc;
pub mod clipboard;
pub mod dictionary;
pub mod email;
pub mod info_tooltip;
pub mod launcher;
pub mod lockscreen;
pub mod notes;
pub mod notifications;
pub mod pass;
pub mod power;
pub mod reminders;
pub mod rng;
pub mod screenshot;
pub mod settings;
pub mod size_resizer;
pub mod system_tooltip;
pub mod todo;
pub mod tooltip_engine;
pub mod unicode;
pub mod websearch;

pub use amogus::{AmogusMessage, AmogusOverlay};
pub use calc::{CalcMessage, CalculatorOverlay};
pub use clipboard::{ClipboardMessage, ClipboardOverlay};
pub use dictionary::{DictMessage, DictionaryOverlay};
pub use email::{EmailMessage, EmailOverlay};
pub use launcher::{AppLauncher, LauncherMessage};
pub use lockscreen::{LockMessage, LockscreenOverlay};
pub use notes::{NotesMessage, NotesOverlay};
pub use notifications::{NotifMessage, NotificationOverlay};
pub use pass::{PassMessage, PassOverlay};
pub use power::{PowerAction, PowerMessage, PowerOverlay};
pub use reminders::{ReminderMessage, RemindersOverlay};
pub use rng::{RngMessage, RngOverlay};
pub use screenshot::{
    grim_capture, save_annotated_image, slurp_region, AnnotatorMessage, AnnotatorOverlay,
};
pub use settings::{SettingsMessage, SettingsOverlay};
pub use todo::{TodoMessage, TodoOverlay};
pub use unicode::{UnicodeMessage, UnicodeOverlay};
pub use websearch::{WebSearchMessage, WebSearchOverlay};

pub use screenshot::watermark::{add_plate_watermark, reveal};
