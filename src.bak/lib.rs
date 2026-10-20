pub mod backend;
pub mod bar;
pub mod config;
pub mod ipc;
pub mod overlays;
pub mod style;

pub use config::{Base16Palette, ShellSettings, ShellTheme};
pub use ipc::{send_client_command, IpcCommand, IpcServer, OverlayTarget};
