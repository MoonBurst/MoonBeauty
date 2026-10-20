pub mod audio;
pub mod borg;
pub mod clock;
pub mod mpris;
pub mod notif_dbus;
pub mod podman_twitch;
pub mod system;
pub mod weather;

pub use audio::{AudioBackend, AudioStatus};
pub use borg::{BorgBackend, BorgStatus};
pub use clock::{CalendarViewData, ClockBackend, WorldZoneColumn};
pub use mpris::{MprisBackend, MprisTrack};
pub use podman_twitch::{PodmanBackend, PodmanStatus, WatcherDetails};
pub use system::{BatteryMetrics, CpuMetrics, GpuMetrics, NetMetrics, RamMetrics, SystemCollector};
pub use weather::{WeatherAlert, WeatherBackend, WeatherData};

#[derive(Debug, Clone)]
pub struct UnifiedTelemetry {
    pub cpu: CpuMetrics,
    pub gpu: GpuMetrics,
    pub ram: RamMetrics,
    pub net: NetMetrics,
    pub battery: BatteryMetrics,
    pub audio: AudioStatus,
    pub mpris: Option<MprisTrack>,
    pub weather: Option<WeatherData>,
    pub podman: PodmanStatus,
    pub borg: BorgStatus,
    pub bar_time: String,
}
