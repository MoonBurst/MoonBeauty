//! Unified capsule data — podman / borg / checks status for the unified
//! bar module and its tooltip.
//!
//! - `podman` : PodmanBackend (docker/podman socket polling), PodmanStatus,
//!              WatcherDetails (Twitch drops watcher info), ContainerInfo
//! - `borg`   : BorgBackend (backup status file poller), BorgStatus
//! - `checks` : check_unified_items, toggle_item, UnifiedItemStatus
//!              (declarative health checks from settings.unified_items)

pub mod podman;
pub mod borg;
pub mod checks;

pub use podman::{PodmanBackend, PodmanStatus, WatcherDetails, ContainerInfo};
pub use borg::{BorgBackend, BorgStatus};
pub use checks::{UnifiedItemStatus, check_unified_items, toggle_item};
