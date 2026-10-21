#![allow(clippy::new_without_default)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::should_implement_trait)]

pub mod backend;
pub mod bar;
pub mod config;
pub mod ipc;
pub mod overlays;
pub mod style;

pub use config::{Base16Palette, ShellSettings, ShellTheme};
pub use ipc::{send_client_command, IpcCommand, IpcServer, OverlayTarget};
