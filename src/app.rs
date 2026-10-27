//! App core: Message enum + MoonBeautyShellApp struct.
//!
//! Bin-only module (submodule of main.rs). Not part of the
//! `moon_beauty_shell` library crate — the app orchestrator is
//! binary-only.
//!
//! Commit 10a scope: type definitions only. All `impl MoonBeautyShellApp`
//! blocks and helper functions still live in main.rs and reference these
//! types via `use app::{Message, MoonBeautyShellApp};`.
//!
//! Subsequent commits (10b+) will progressively move:
//!   - update()      (10b)
//!   - view()        (10c)
//!   - subscription()(10d)
//!   - new_app() + small impls (10e)
//!   - helpers        (10f)
//!   - split app.rs into app/{mod,update,view,...} submodules (10g)

use moon_beauty_shell::bar::unified::UnifiedTelemetry;
use moon_beauty_shell::bar::TopBarMessage;
use moon_beauty_shell::libraries::backend::*;
use moon_beauty_shell::libraries::config::{ShellSettings, ShellTheme};
use moon_beauty_shell::libraries::ipc::{IpcCommand, OverlayTarget};
use moon_beauty_shell::overlays::*;

#[derive(Debug, Clone)]
pub enum Message {
    Noop,
    TickHardware,
    TickWeather,
    TickEmailSync,
    EmailSynced,
    EmailSyncedWithDiff(
        Vec<moon_beauty_shell::libraries::backend::email_client::NativeEnvelope>,
        Vec<moon_beauty_shell::libraries::backend::email_client::NativeEnvelope>,
    ),
    FocusComposeField,
    ComposeTabPressed(bool),
    WeatherLoaded(Option<moon_beauty_shell::bar::weather::WeatherData>),
    TickClock,
    TickToasts,
    MetricsCollected(
        CpuMetrics,
        RamMetrics,
        GpuMetrics,
        AudioStatus,
        AudioStatus,
        Option<MprisTrack>,
    ),
    IpcReceived(IpcCommand),
    PollIpc,
    PollNotifs,
    ResizeNotifSurface(u32, u32),
    WindowResized(f32),
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
    /// every 15s. Ensures the first open has a correct
    /// item count (avoids the 206→186 re-animation flicker).
    FetchTrayItems,
    TrayItemsLoaded(Vec<moon_beauty_shell::bar::tray::TrayItem>),
    TrayAnimTick,
    TrayWatchdogTick,
    CollapseTray,
    /// Pre-computed toast rectangles (surface-local coords). Emitted
    /// whenever the toast count changes; `try_into` converts it to
    /// `LayershellCustomAction::SetInputRegion` so the layer surface's
    /// click target shrinks to just the visible toasts. Everything else
    /// (the empty area around/above/below toasts) becomes pass-through.
    SetToastInputRegion(Vec<(i32, i32, i32, i32)>),
    /// Fires from the 500ms subscription. If `standalone_child` is
    /// Lockscreen and has exited (TTY switch, panic, SIGKILL), respawn
    /// it immediately. Lockscreen must NEVER be absent — a dead lock is
    /// an unlocked screen. Runs fast (500ms) so the worst-case window
    /// between crash and respawn is 500ms + child boot (~300ms).
    CheckLockscreenAlive,
}

impl TryInto<iced_layershell::actions::LayershellCustomActionWithId> for Message {
    type Error = Message;
    fn try_into(
        self,
    ) -> Result<iced_layershell::actions::LayershellCustomActionWithId, Self::Error> {
        match self {
            Message::ResizeNotifSurface(w, h) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    None,
                    iced_layershell::actions::LayershellCustomAction::SizeChange((w, h)),
                ))
            }
            Message::SetToastInputRegion(rects) => {
                Ok(iced_layershell::actions::LayershellCustomActionWithId::new(
                    None,
                    iced_layershell::actions::LayershellCustomAction::SetInputRegion(
                        iced_layershell::actions::ActionCallback::new(move |region| {
                            for (x, y, w, h) in &rects {
                                region.add(*x, *y, *w, *h);
                            }
                        }),
                    ),
                ))
            }
            other => Err(other),
        }
    }
}

pub struct MoonBeautyShellApp {
    pub weather_tooltip_child: Option<std::process::Child>,
    pub settings: ShellSettings,
    pub theme: ShellTheme,
    pub telemetry: UnifiedTelemetry,
    pub active_overlay: Option<OverlayTarget>,
    pub is_standalone: bool,
    pub system_collector: SystemCollector,
    pub standalone_child: Option<(OverlayTarget, std::process::Child)>,
    pub ipc_rx: std::sync::mpsc::Receiver<IpcCommand>,
    pub is_notif_surface: bool,
    pub notif_rx: Option<
        std::sync::mpsc::Receiver<moon_beauty_shell::overlays::notifications::IncomingNotification>,
    >,
    pub settings_json_mtime: Option<std::time::SystemTime>,
    pub window_w: f32,
    pub notif_screen_at_startup: String,
    pub notif_surface_size: (u32, u32),
    pub tray_expanded: bool,
    pub tray_items: Vec<moon_beauty_shell::bar::tray::TrayItem>,
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
    pub detected_gpus: Vec<moon_beauty_shell::libraries::backend::GpuInfo>,

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
    pub weather_tick: bool,

    pub last_cursor_x: f32,
    pub last_cursor_time: std::time::Instant,
    /// Timestamp of the previous CheckLockscreenAlive tick. If a tick
    /// fires more than ~2s after the previous one, the session was
    /// paused (TTY switch, suspend, hibernate). In that case the lock
    /// child surfaces are likely stale (sway drops layer surfaces on
    /// output detach without telling the client), so force-respawn.
    pub last_lockscreen_check: std::time::Instant,
    /// Last observed active-toast count. Changes trigger a fresh
    /// SetToastInputRegion emit so the click target tracks the toasts.
    pub last_toast_count: usize,
    /// (count, written_at) for the active+pending count file. Write on
    /// change; heartbeat every 3s so the file's mtime stays fresh and
    /// the bar's staleness check never trips while we're alive.
    pub last_active_pending_written: Option<(usize, std::time::Instant)>,
}
