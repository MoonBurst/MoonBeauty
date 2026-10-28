//! Lockscreen overlay — coordinator process, per-output child UI, keyboard capture.

pub mod coordinator;
pub mod keys;
pub mod ui;

pub use coordinator::{LockCoordinator, LockIpc};
pub use keys::resolve_lock_key;
pub use ui::{LockMessage, LockscreenOverlay};
