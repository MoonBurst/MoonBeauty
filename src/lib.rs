#![allow(clippy::new_without_default)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::should_implement_trait)]

pub mod bar;
pub mod libraries;
pub mod overlays;

pub use libraries::config::{Base16Palette, ShellSettings, ShellTheme};
pub use libraries::ipc::{send_client_command, IpcCommand, IpcServer, OverlayTarget};


/// Debug-only logging. No-ops unless `MBS_DEBUG` is set in the environment.
/// Use this for hot-path diagnostics that must not spam the journal every frame.
#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        if ::std::env::var_os("MBS_DEBUG").is_some() {
            eprintln!($($arg)*);
        }
    };
}
