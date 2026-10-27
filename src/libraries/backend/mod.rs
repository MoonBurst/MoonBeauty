pub mod annotator_ipc;
pub mod audio;
pub mod email_client;
pub mod borg;
pub mod clock;
pub mod mpris;
pub mod podman_twitch;
pub mod system;
pub mod tray_watcher;
pub mod unified;
pub mod weather;

pub use audio::{AudioBackend, AudioStatus, play_alarm_sound, resolve_alarm_sound_path};
pub use borg::{BorgBackend, BorgStatus};
pub use clock::{CalendarViewData, ClockBackend, WorldZoneColumn};
pub use mpris::{MprisBackend, MprisTrack};
pub use podman_twitch::{PodmanBackend, PodmanStatus, WatcherDetails};
pub use unified::{UnifiedItemStatus, check_unified_items, toggle_item};
pub use system::{BatteryMetrics, CpuMetrics, GpuInfo, GpuMetrics, NetMetrics, RamMetrics, SystemCollector};
pub use weather::{WeatherAlert, WeatherBackend, WeatherData};
pub use tray_watcher::{TrayItem, TrayMenuItem, fetch_tray_items, activate_tray_item, get_menu_tree, trigger_menu_item};

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
