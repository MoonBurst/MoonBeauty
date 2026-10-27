//! UnifiedTelemetry — aggregate snapshot for the bar's status capsules.
//!
//! Every field here is what the bar renders + what the unified capsule
//! tooltip displays. The struct is rebuilt on each hardware poll and
//! cloned into the render path.
//!
//! Lives in bar/unified/ (not libraries/backend) because it aggregates
//! types from multiple bar/ modules (weather, unified) alongside backend
//! ones (audio, mpris, system). Keeping it in backend forced backend to
//! `use crate::bar::…` for every relocated type, creating a backend → bar
//! dependency that this move eliminates.

use crate::bar::weather::WeatherData;
use crate::bar::unified::{BorgStatus, PodmanStatus};
use crate::libraries::backend::audio::AudioStatus;
use crate::libraries::backend::mpris::MprisTrack;
use crate::libraries::backend::system::{
    BatteryMetrics, CpuMetrics, GpuMetrics, NetMetrics, RamMetrics,
};

#[derive(Debug, Clone)]
pub struct UnifiedTelemetry {
    pub cpu: CpuMetrics,
    pub gpu: GpuMetrics,
    pub gpu_selected_idx: usize,
    pub ram: RamMetrics,
    pub net: NetMetrics,
    pub battery: BatteryMetrics,
    pub audio: AudioStatus,
    pub mic: AudioStatus,
    pub mpris: Option<MprisTrack>,
    pub weather: Option<WeatherData>,
    pub podman: PodmanStatus,
    pub borg: BorgStatus,
    pub bar_time: String,
}
