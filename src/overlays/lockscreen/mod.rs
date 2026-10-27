//! Lockscreen overlay — coordinator process, per-output child UI, keyboard capture.

pub mod ui;
pub mod coordinator;
pub mod keys;

pub use ui::{LockscreenOverlay, LockMessage};
pub use coordinator::{LockCoordinator, LockIpc};
pub use keys::resolve_lock_key;
