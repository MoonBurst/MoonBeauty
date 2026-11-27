//! App core: Message enum + MoonBeautyShellApp struct.
//!
//! Bin-only module (submodule of main.rs).

pub mod helpers;
pub mod keys;
pub mod new_app;
pub mod subscription;
pub mod update;
pub mod view;

// Helper functions from crate root (main.rs). Rust allows child modules
// to reach parent-module items without them being `pub`.
use crate::{detect_notif_surface, detect_standalone_overlay};
use moonbeauty::libraries::process::resolve_exe;

use iced::widget::{container, Stack};
use iced::{Element, Length, Padding, Task as Command};
use moonbeauty::bar::geometry::{capsule_actual_width, capsule_center_x};
use moonbeauty::bar::tooltip_spawn::get_screen_width_for;
use moonbeauty::libraries::backend::UnifiedTelemetry;
use moonbeauty::bar::{CapsuleAction, TopBar, TopBarMessage};
use moonbeauty::libraries::backend::*;
use moonbeauty::libraries::config::{ShellSettings, ShellTheme};
use moonbeauty::libraries::ipc::{send_client_command, IpcCommand, OverlayTarget};
use moonbeauty::overlays::screenshot::annotator::Tool;
use moonbeauty::overlays::*;

#[derive(Debug, Clone)]
pub enum Message {
    #[allow(dead_code)]
    Noop,
    TickHardware,
    TickWeather,
    TickEmailSync,
    EmailSynced,
    EmailSyncedWithDiff(
        Vec<moonbeauty::libraries::backend::email::client::NativeEnvelope>,
        Vec<moonbeauty::libraries::backend::email::client::NativeEnvelope>,
    ),
    ComposeTabPressed(bool),
    WeatherLoaded(Option<moonbeauty::bar::weather::WeatherData>),
    TickClock,
    IdleTick,
    TickToasts,
    MetricsCollected(
        CpuMetrics,
        RamMetrics,
        GpuMetrics,
        AudioStatus,
        AudioStatus,
        Option<MprisTrack>,
        NetMetrics,
    ),
    IpcReceived(IpcCommand),
    PollNotifs,
    ResizeNotifSurface(Option<iced::window::Id>, u32, u32),
    OpenNotifLayer(iced::window::Id, iced_layershell::reexport::NewLayerShellSettings),
    CloseNotifLayer(iced::window::Id),
    TopBar(TopBarMessage),
    Launcher(LauncherMessage),
    Calc(CalcMessage),
    Clipboard(ClipboardMessage),
    Notes(NotesMessage),
    Pass(PassMessage),
    Power(PowerMessage),
    Todo(TodoMessage),
    Reminders(ReminderMessage),
    Dict(DictMessage),
    Unicode(UnicodeMessage),
    Web(WebSearchMessage),
    Amogus(AmogusMessage),
    Annotator(AnnotatorMessage),
    Rng(RngMessage),
    Email(EmailMessage),
    Notif(NotifMessage),
    Settings(SettingsMessage),
    Lock(LockMessage),
    DismissActiveOverlay,
    CloseShell,
    /// Background fetch of tray items — fires on startup and
    /// every 15s so the first open animates straight to the right width.
    FetchTrayItems,
    TrayItemsLoaded(Vec<moonbeauty::bar::tray::TrayItem>),
    TrayWatchdogTick,
    InBarWeatherTip(moonbeauty::bar::weather::tooltip::Message),
    InBarSysTip(moonbeauty::overlays::system_tooltip::Message),
    InBarInfoTip(moonbeauty::overlays::info_tooltip::Message),
    /// Pre-computed toast rectangles (surface-local coords) emitted
    /// whenever the toast count changes so `SetInputRegion` shrinks the
    /// layer surface's click target to just the visible toasts.
    SetToastInputRegion(Option<iced::window::Id>, Vec<(i32, i32, i32, i32)>),
    RequestXdgActivation(iced_layershell::actions::ActivationCallback),
    /// Music cycle press completed — carries the updated state so
    /// subsequent presses continue from the right position.
    MusicCycleDone(moonbeauty::libraries::backend::MusicCycleState),
}

impl TryInto<iced_layershell::actions::LayershellCustomActionWithId> for Message {
    type Error = Message;
    fn try_into(
        self,
    ) -> Result<iced_layershell::actions::LayershellCustomActionWithId, Self::Error> {
        match self {
            Message::ResizeNotifSurface(id, w, h) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    id,
                    iced_layershell::actions::LayershellCustomAction::SizeChange((w, h)),
                ))
            }
            Message::SetToastInputRegion(id, rects) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    id,
                    iced_layershell::actions::LayershellCustomAction::SetInputRegion(
                        iced_layershell::actions::ActionCallback::new(move |region| {
                            for (x, y, w, h) in &rects {
                                region.add(*x, *y, *w, *h);
                            }
                        }),
                    ),
                ))
            }
            Message::OpenNotifLayer(id, settings) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    None,
                    iced_layershell::actions::LayershellCustomAction::NewLayerShell {
                        settings,
                        id,
                    },
                ))
            }
            Message::CloseNotifLayer(id) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    Some(id),
                    iced_layershell::actions::LayershellCustomAction::RemoveWindow,
                ))
            }
            Message::RequestXdgActivation(cb) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    None,
                    iced_layershell::actions::LayershellCustomAction::RequestActivationToken(cb),
                ))
            }
            other => Err(other),
        }
    }
}

pub enum InBarTooltip {
    Weather(moonbeauty::bar::weather::tooltip::App),
    System(moonbeauty::overlays::system_tooltip::App),
    Info(moonbeauty::overlays::info_tooltip::App),
}

pub struct MoonBeautyShellApp {
    pub settings: ShellSettings,
    pub theme: ShellTheme,
    pub telemetry: UnifiedTelemetry,
    pub active_overlay: Option<OverlayTarget>,
    pub is_standalone: bool,
    pub system_collector: std::sync::Arc<std::sync::Mutex<SystemCollector>>,
    pub standalone_child: Option<(OverlayTarget, std::process::Child)>,
    pub is_notif_surface: bool,
    pub notif_rx: Option<
        std::sync::mpsc::Receiver<moonbeauty::overlays::notifications::IncomingNotification>,
    >,
    pub settings_json_mtime: Option<std::time::SystemTime>,
    pub notif_screen_at_startup: String,
    pub notif_surface_size: (u32, u32),
    pub notif_surface_child: Option<std::process::Child>,
    pub in_bar_tooltip: Option<InBarTooltip>,
    pub tooltip_window_id: Option<iced::window::Id>,
    pub closing_tooltip_id: Option<iced::window::Id>,
    pub tray_expanded: bool,
    pub tray_items: Vec<moonbeauty::bar::tray::TrayItem>,
    pub tray_anim_width: f32,
    pub tray_target_width: f32,
    pub tray_anim_start_width: f32,
    pub tray_anim_started_at: std::time::Instant,
    pub tray_anim_duration_ms: u64,
    pub tray_timeout_gen: u64,
    /// When the tray was last opened. Used by the TrayWatchdogTick
    /// subscription to close the tray when `settings.tray_timeout_sec`
    /// expires. Independent from `tray_anim_started_at` because the
    /// latter is reset by TrayItemsLoaded when item count changes.
    pub tray_opened_at: std::time::Instant,
    pub detected_gpus: Vec<moonbeauty::libraries::backend::GpuInfo>,

    pub launcher: AppLauncher,
    pub calc: CalculatorOverlay,
    pub clipboard: ClipboardOverlay,
    pub notes: NotesOverlay,
    pub pass: PassOverlay,
    pub power: PowerOverlay,
    pub todo: TodoOverlay,
    pub reminders: RemindersOverlay,
    pub dict: DictionaryOverlay,
    pub unicode: UnicodeOverlay,
    pub web: WebSearchOverlay,
    pub amogus: AmogusOverlay,
    pub annotator: AnnotatorOverlay,
    pub rng: RngOverlay,
    pub email: EmailOverlay,
    pub notifs: NotificationOverlay,
    pub settings_ui: SettingsOverlay,
    pub lockscreen: LockscreenOverlay,
    /// Music cycle state — which player to target next, and which we
    /// most recently started (for pause priority when multiple are playing).
    pub music_cycle_state: moonbeauty::libraries::backend::MusicCycleState,
    pub is_cycling: bool,

    pub last_cursor_x: f32,
    pub last_cursor_time: std::time::Instant,
    /// Last observed active-toast count. Changes trigger a fresh
    /// SetToastInputRegion emit so the click target tracks the toasts.
    pub last_toast_count: usize,
    /// (count, written_at) for the active+pending count file. Write on
    /// change; heartbeat every 3s so the file's mtime stays fresh and
    /// the bar's staleness check never trips while we're alive.
    pub last_active_pending_written: Option<(usize, std::time::Instant)>,
}

pub mod restart;
