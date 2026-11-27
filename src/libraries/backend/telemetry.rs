//! UnifiedTelemetry — aggregate snapshot for the bar's status capsules.
//!
//! Every field here is what the bar renders + what the unified capsule
//! tooltip displays. The struct is rebuilt on each hardware poll and
//! cloned into the render path.

use crate::bar::weather::WeatherData;
use crate::libraries::backend::audio::AudioStatus;
use crate::libraries::backend::music::MprisTrack;
use crate::libraries::backend::system::{
    BatteryMetrics, CpuMetrics, GpuMetrics, NetMetrics, RamMetrics,
};
use crate::libraries::backend::unified::{borg::BorgStatus, podman::PodmanStatus};

#[derive(Debug, Clone, Default)]
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
    pub next_alarm_epoch: Option<i64>,
    pub notify_paused: bool,
    pub notify_count: usize,
}
