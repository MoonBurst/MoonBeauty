//! Unified capsule data — podman / borg / checks status for the unified
//! bar module and its tooltip.
//!
//! - `podman` : PodmanBackend (docker/podman socket polling), PodmanStatus,
//!              WatcherDetails (Twitch drops watcher info), ContainerInfo
//! - `borg`   : BorgBackend (backup status file poller), BorgStatus
//! - `checks` : check_unified_items, toggle_item, UnifiedItemStatus
//!              (declarative health checks from settings.unified_items)
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

pub mod borg;
pub mod checks;
pub mod podman;
pub mod telemetry;

pub use borg::{BorgBackend, BorgStatus};
pub use checks::{check_unified_items, toggle_item, UnifiedItemStatus};
pub use podman::{ContainerInfo, PodmanBackend, PodmanStatus, WatcherDetails};
pub use telemetry::UnifiedTelemetry;
