use crate::bar::unified::{BorgStatus, PodmanStatus};
use crate::bar::weather::WeatherData;
pub mod audio;
pub mod email_client;
pub mod clock;
pub mod mpris;
pub mod system;

pub use audio::{AudioBackend, AudioStatus, play_alarm_sound, resolve_alarm_sound_path};
pub use clock::{CalendarViewData, ClockBackend, WorldZoneColumn};
pub use mpris::{MprisBackend, MprisTrack};
pub use system::{BatteryMetrics, CpuMetrics, GpuInfo, GpuMetrics, NetMetrics, RamMetrics, SystemCollector};

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
