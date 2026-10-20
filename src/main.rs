use iced::widget::{container, Stack};
use iced::{Color, Element, Length, Padding, Task as Command, Theme};
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings};
use iced_layershell::Application;

use moon_beauty_shell::backend::*;
use moon_beauty_shell::bar::{CapsuleAction, TopBar, TopBarMessage};
use moon_beauty_shell::config::{ShellSettings, ShellTheme};
use moon_beauty_shell::ipc::{send_client_command, send_annotator_show, IpcCommand, OverlayTarget};
use moon_beauty_shell::overlays::*;

fn detect_notif_surface(args: &[String]) -> bool {
    args.iter().any(|a| a == "--notif-surface")
}

fn detect_standalone_overlay(args: &[String]) -> Option<OverlayTarget> {
    for (i, arg) in args.iter().enumerate() {
        if arg == "--annotator-session" { return Some(OverlayTarget::Annotator); }
        if arg == "--overlay" || arg == "--ipc" {
            if let Some(target_str) = args.get(i + 1) {
                return OverlayTarget::from_str(target_str);
            }
        }
        if let Some(flag) = arg.strip_prefix("--") {
            if let Some(target) = OverlayTarget::from_str(flag) {
                return Some(target);
            }
        }
    }
    None
}

// Sync IPC helper — send_client_command uses tokio::net::UnixStream,
// which requires a tokio reactor. main() has no runtime, so we use
// std::os::unix::net::UnixStream directly. Same wire format.
fn send_ipc_sync(cmd: &IpcCommand) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let path = std::env::var("XDG_RUNTIME_DIR")
        .map(|d| std::path::PathBuf::from(d).join("moon-beauty-shell.sock"))
        .map_err(|e| format!("XDG_RUNTIME_DIR: {e}"))?;
    let mut stream = UnixStream::connect(&path)?;
    let encoded = serde_json::to_vec(cmd)?;
    stream.write_all(&encoded)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp)?;
    Ok(resp)
}


fn resolve_exe() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            // /proc/PID/exe on Linux returns "…/bin (deleted)" after `cargo build`
            // replaces the running binary. Strip the suffix so spawn still works.
            if let Some(stripped) = s.strip_suffix(" (deleted)") {
                std::path::PathBuf::from(stripped)
            } else {
                p
            }
        })
        .unwrap_or_else(|| std::path::PathBuf::from("moon-beauty-shell"))
}

pub fn main() -> Result<(), iced_layershell::Error> {
    let args: Vec<String> = std::env::args().collect();

    // --quickshot was replaced by --screenshot --annotate.
    if args.iter().any(|a| a == "--quickshot" || a == "quickshot") {
        eprintln!("[error] --quickshot is gone. Screenshot workflows now:");
        eprintln!("        --screenshot              capture region and save");
        eprintln!("        --screenshot --annotate   capture and open annotator");
        eprintln!("        --watermark \"name\"        embed invisible watermark");
        eprintln!();
        eprintln!("        Update your keybinds accordingly.");
        std::process::exit(2);
    }

    // ── Screenshot CLI ────────────────────────────────────────────────
    // --screenshot [--annotate] [--watermark "name"]
    // MUST be above the notif-surface spawn block, or the flag falls
    // through and boots the full bar.
    if args.iter().any(|a| a == "--screenshot") {
        let annotate = args.iter().any(|a| a == "--annotate");
        let watermark_cli = args.iter()
            .position(|a| a == "--watermark")
            .and_then(|i| args.get(i + 1).cloned());

        let region = slurp_region();
        let path = match grim_capture(region.as_deref()) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[screenshot] capture failed: {e}");
                std::process::exit(1);
            }
        };
        eprintln!("[screenshot] saved {}", path.display());

                if annotate {
            let path_str = path.display().to_string();
            // Route through the running bar's IPC so it opens the annotator
            // overlay. send_client_command is async — block on it (no runtime
            // here, we're in main() before iced starts).
            let cmd = IpcCommand::Open(OverlayTarget::Annotator, Some(path_str.clone()));
            match send_ipc_sync(&cmd) {
                Ok(resp) => eprintln!("[screenshot] annotator via bar IPC: {}", resp.trim()),
                Err(e) => {
                    eprintln!("[screenshot] bar IPC failed: {e}; trying annotator socket");
                    if let Err(e2) = send_annotator_show(path_str, watermark_cli) {
                        eprintln!("[screenshot] annotator socket also failed: {e2}");
                        eprintln!("[hint] the bar must be running (moon-beauty-shell &),");
                        eprintln!("[hint] or an --annotator-session must be active");
                        std::process::exit(1);
                    }
                }
            }
        }
        std::process::exit(0);
    }

    // CLI Notification Push: moon-beauty-shell --notify "LusterDawn" "Message"
    if args.len() >= 3 && (args[1] == "--notify" || (args.len() >= 4 && args[1] == "--ipc" && args[2] == "notify")) {
        let summary = if args[1] == "--notify" { &args[2] } else { &args[3] };
        let body = if args[1] == "--notify" { args.get(3).map(|s| s.as_str()).unwrap_or("") } else { args.get(4).map(|s| s.as_str()).unwrap_or("") };
        let _ = std::process::Command::new("notify-send")
            .args(["-a", "Desktop", summary, body])
            .spawn();
        std::process::exit(0);
    }

    let is_notif_surface = detect_notif_surface(&args);
    let standalone_target = detect_standalone_overlay(&args);
    let settings = ShellSettings::load();

    if !is_notif_surface && standalone_target.is_none() {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "--", "moon-beauty-shell --notif-surface"])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let exe = resolve_exe();
        let _ = std::process::Command::new(exe).arg("--notif-surface").spawn();
    }

    let binded_output_name = if settings.notif_screen_name != "Auto" && settings.notif_screen_name != "Auto (Primary)" && !settings.notif_screen_name.is_empty() {
        Some(settings.notif_screen_name.clone())
    } else {
        None
    };

    if is_notif_surface {
        eprintln!("[notif-surface] startup binded_output_name={:?}", binded_output_name);
    }
    let layer_settings = if is_notif_surface {
        let corner = &settings.notif_corner;
        let is_top = corner.contains("top");
        let is_right = corner.contains("right");
        let anchor = match (is_top, is_right) {
            (true, true)   => Anchor::Top    | Anchor::Right,
            (true, false)  => Anchor::Top    | Anchor::Left,
            (false, true)  => Anchor::Bottom | Anchor::Right,
            (false, false) => Anchor::Bottom | Anchor::Left,
        };
        let strip_w = (settings.notif_card_width as i32
            + 2 * settings.notif_margin_x + 40).max(200) as u32;
        let strip_h = (3 * settings.notif_card_height as i32
            + 2 * settings.notif_margin_x + 100).max(300) as u32;
        LayerShellSettings {
            size: Some((strip_w, strip_h)),
            exclusive_zone: -1,
            anchor,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::None,
            binded_output_name,
            ..Default::default()
        }
    } else if standalone_target.as_ref() == Some(&OverlayTarget::Annotator) {
        // Floating annotator window. wlr-layer-shell centers when all four
        // sides are anchored with a fixed size. Overlay layer keeps it above
        // the bar. Tune (W, H) to taste — this reads well on 1440p.
        LayerShellSettings {
            size: Some((1600, 1000)),
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            binded_output_name,
            ..Default::default()
        }
    } else if standalone_target.is_some() {
        LayerShellSettings {
            size: Some((0, 0)),
            exclusive_zone: -1,
            anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            binded_output_name,
            ..Default::default()
        }
    } else {
        LayerShellSettings {
            size: Some((0, settings.bar_height)),
            exclusive_zone: settings.bar_height as i32,
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            layer: Layer::Top,
            keyboard_interactivity: KeyboardInteractivity::None,
            binded_output_name,
            ..Default::default()
        }
    };

    MoonBeautyShellApp::run(Settings {
        layer_settings,
        ..Default::default()
    })
}

#[derive(Debug, Clone)]
pub enum Message {
    TickHardware,
    TickClock,
    TickToasts,
    MetricsCollected(CpuMetrics, RamMetrics, GpuMetrics, AudioStatus, AudioStatus, Option<MprisTrack>),
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
}

impl TryInto<iced_layershell::actions::LayershellCustomActionsWithInfo<()>> for Message {
    type Error = Message;
    fn try_into(
        self,
    ) -> Result<iced_layershell::actions::LayershellCustomActionsWithInfo<()>, Self::Error>
    {
        match self {
            Message::ResizeNotifSurface(w, h) => Ok(
                iced_layershell::actions::LayershellCustomActionsWithInfo::SizeChange((w, h)),
            ),
            other => Err(other),
        }
    }
}

pub struct MoonBeautyShellApp {
    settings: ShellSettings,
    theme: ShellTheme,
    telemetry: UnifiedTelemetry,
    active_overlay: Option<OverlayTarget>,
    is_standalone: bool,
    system_collector: SystemCollector,
    standalone_child: Option<(OverlayTarget, std::process::Child)>,
    ipc_rx: std::sync::mpsc::Receiver<IpcCommand>,
    is_notif_surface: bool,
    notif_rx: Option<std::sync::mpsc::Receiver<moon_beauty_shell::backend::notif_dbus::IncomingNotification>>,
    settings_json_mtime: Option<std::time::SystemTime>,
    window_w: f32,
    notif_screen_at_startup: String,
    notif_surface_size: (u32, u32),

    launcher: AppLauncher,
    calc: CalculatorOverlay,
    clipboard: ClipboardOverlay,
    notes: NotesOverlay,
    pass: PassOverlay,
    power: PowerOverlay,
    todo: TodoOverlay,
    reminders: RemindersOverlay,
    dict: DictionaryOverlay,
    unicode: UnicodeOverlay,
    web: WebSearchOverlay,
    amogus: AmogusOverlay,
    annotator: AnnotatorOverlay,
    rng: RngOverlay,
    email: EmailOverlay,
    notifs: NotificationOverlay,
    settings_ui: SettingsOverlay,
    lockscreen: LockscreenOverlay,
}

impl MoonBeautyShellApp {
    fn standalone_running(&mut self) -> Option<OverlayTarget> {
        if let Some((t, child)) = self.standalone_child.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                return Some(t.clone());
            }
        }
        if self.standalone_child.is_some() {
            if let Some((_, mut child)) = self.standalone_child.take() {
                let _ = child.wait();
            }
        }
        None
    }

    fn kill_standalone(&mut self) {
        if let Some((_, mut child)) = self.standalone_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn spawn_standalone(&mut self, target: OverlayTarget) {
        self.kill_standalone();
        let exe = resolve_exe();
        let tag = target.as_arg();
        let log_path = format!("/tmp/mbs-standalone-{}.log", tag);
        let stderr_file = std::fs::OpenOptions::new()
            .create(true).truncate(true).write(true)
            .open(&log_path).ok();
        let stdout_file = stderr_file.as_ref().and_then(|f| f.try_clone().ok());

        let mut cmd = std::process::Command::new(exe);
        cmd.arg(format!("--{}", tag));
        if let Some(sf) = stderr_file { cmd.stderr(sf); }
        if let Some(of) = stdout_file { cmd.stdout(of); }

        match cmd.spawn() {
            Ok(c) => self.standalone_child = Some((target, c)),
            Err(e) => eprintln!("[ipc] spawn {:?} failed: {e}", target),
        }
    }

    fn toggle_standalone(&mut self, target: OverlayTarget) {
        match self.standalone_running() {
            Some(ref t) if t == &target => self.kill_standalone(),
            _ => self.spawn_standalone(target),
        }
    }
}

impl iced_layershell::Application for MoonBeautyShellApp {
    type Message = Message;
    type Flags = ();
    type Theme = Theme;
    type Executor = iced::executor::Default;

    fn namespace(&self) -> String {
        "moon-beauty-shell".into()
    }

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        let args: Vec<String> = std::env::args().collect();
        let standalone_target = detect_standalone_overlay(&args);
        let is_notif_surface = detect_notif_surface(&args);
        let is_standalone = standalone_target.is_some();
        let settings = ShellSettings::load();
        let theme = ShellTheme::from_settings(&settings);

        let (ipc_tx, ipc_rx) = std::sync::mpsc::channel::<IpcCommand>();
        if standalone_target.is_none() && !is_notif_surface {
            std::thread::spawn(move || {
                use std::io::Read;
                use std::os::unix::net::UnixListener;
                let path = moon_beauty_shell::ipc::get_socket_path();
                let _ = std::fs::remove_file(&path);
                let listener = match UnixListener::bind(&path) {
                    Ok(l) => { eprintln!("[ipc] listener bound (sync thread)"); l }
                    Err(e) => { eprintln!("[ipc] bind failed: {e}"); return; }
                };
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                            let mut buf = Vec::new();
                            if let Err(e) = stream.read_to_end(&mut buf) {
                                eprintln!("[ipc] read error: {e}");
                                continue;
                            }
                            match serde_json::from_slice::<IpcCommand>(&buf) {
                                Ok(cmd) => {
                                    eprintln!("[ipc] accepted: {:?}", cmd);
                                    if ipc_tx.send(cmd).is_err() {
                                        eprintln!("[ipc] channel closed");
                                        break;
                                    }
                                }
                                Err(e) => eprintln!("[ipc] deserialize error: {e}"),
                            }
                        }
                        Err(e) => eprintln!("[ipc] accept error: {e}"),
                    }
                }
            });
        }

        let notif_rx = if is_notif_surface {
            let (tx, rx) = std::sync::mpsc::channel();
            moon_beauty_shell::backend::notif_dbus::spawn(tx);
            Some(rx)
        } else {
            None
        };

        let notif_screen_at_startup = settings.notif_screen_name.clone();
        let app = Self {
            settings_ui: SettingsOverlay::new(&settings),
            settings,
            theme,
            telemetry: UnifiedTelemetry {
                cpu: Default::default(),
                gpu: Default::default(),
                ram: Default::default(),
                net: Default::default(),
                battery: Default::default(),
                audio: Default::default(),
                mic: Default::default(),
                mpris: None,
                weather: None,
                podman: Default::default(),
                borg: Default::default(),
                bar_time: ClockBackend::get_bar_time(),
            },
            active_overlay: standalone_target,
            is_standalone,
            system_collector: SystemCollector::new(),
            standalone_child: None,
            ipc_rx,
            is_notif_surface,
            notif_rx,
            settings_json_mtime: std::fs::metadata(ShellSettings::config_path())
                .ok()
                .and_then(|m| m.modified().ok()),
            window_w: 1920.0,
            notif_screen_at_startup,
            notif_surface_size: if is_notif_surface { (10, 10) } else { (0, 0) },

            launcher: AppLauncher::new(),
            calc: CalculatorOverlay::new(),
            clipboard: ClipboardOverlay::new(),
            notes: NotesOverlay::new(),
            pass: PassOverlay::new(),
            power: PowerOverlay::new(),
            todo: TodoOverlay::new(),
            reminders: RemindersOverlay::new(),
            dict: DictionaryOverlay::new(),
            unicode: UnicodeOverlay::new(),
            web: WebSearchOverlay::new(),
            amogus: AmogusOverlay::new(),
            annotator: {
                let mut a = AnnotatorOverlay::new();
                if let Ok(p) = std::env::var("MBS_ANNOTATOR_PATH") {
                    let pb = std::path::PathBuf::from(&p);
                    match std::fs::read(&pb) {
                        Ok(bytes) => {
                            a.load_image(pb, bytes);
                            eprintln!("[annotator] loaded capture from {p}");
                        }
                        Err(e) => eprintln!("[annotator] failed to read {p}: {e}"),
                    }
                }
                a
            },
            rng: RngOverlay::new(),
            email: EmailOverlay::new(),
            notifs: NotificationOverlay::new(),
            lockscreen: LockscreenOverlay::new(),
        };

        let init_task = if matches!(app.active_overlay, Some(OverlayTarget::Launcher)) {
            iced::widget::text_input::focus("launcher_search")
                .map(Message::Launcher)
        } else if matches!(app.active_overlay, Some(OverlayTarget::Clipboard)) {
            Command::perform(
                ClipboardOverlay::reload(),
                |entries| Message::Clipboard(ClipboardMessage::Reloaded(entries)),
            )
        } else {
            Command::none()
        };
        (app, init_task)
    }

    fn style(&self, _theme: &Self::Theme) -> iced_layershell::Appearance {
        iced_layershell::Appearance {
            background_color: Color::TRANSPARENT,
            text_color: self.theme.text,
        }
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::WindowResized(w) => {
                self.window_w = w;
            }
            Message::ResizeNotifSurface(w, h) => {
                eprintln!("[resize] update caught ({}, {}) — try_into NOT called", w, h);
            }
            Message::PollNotifs => {
                let mtime = std::fs::metadata(ShellSettings::config_path())
                    .ok()
                    .and_then(|m| m.modified().ok());
                if mtime != self.settings_json_mtime {
                    self.settings_json_mtime = mtime;
                    self.settings = ShellSettings::load();
                    self.theme = ShellTheme::from_settings(&self.settings);
                }
                if !self.is_notif_surface
                    && !self.is_standalone
                    && self.settings.notif_screen_name != self.notif_screen_at_startup
                {
                    self.notif_screen_at_startup = self.settings.notif_screen_name.clone();
                    let _ = std::process::Command::new("pkill")
                        .args(["-f", "--", "moon-beauty-shell --notif-surface"])
                        .status();
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let exe = resolve_exe();
                    let _ = std::process::Command::new(exe).arg("--notif-surface").spawn();
                }
                if let Some(rx) = self.notif_rx.as_ref() {
                    while let Ok(n) = rx.try_recv() {
                        let is_test = n.app_name == "Diagnostics";
                        self.notifs.push_notification(
                            &n.app_name, &n.summary, &n.body, n.is_urgent, is_test, &self.settings,
                        );
                    }
                }
                if self.is_notif_surface {
                    let big_w = (self.settings.notif_card_width as i32
                        + 2 * self.settings.notif_margin_x + 100)
                        .max(200) as u32;
                    let big_h = (3 * self.settings.notif_card_height as i32
                        + 2 * self.settings.notif_margin_x + 100)
                        .max(300) as u32;
                    let want = if self.notifs.active_toasts.is_empty() {
                        (10u32, 10u32)
                    } else {
                        (big_w, big_h)
                    };
                    if want != self.notif_surface_size {
                        self.notif_surface_size = want;
                        return Command::done(Message::ResizeNotifSurface(want.0, want.1));
                    }
                }
            }
            Message::TickClock => {
                self.telemetry.bar_time = ClockBackend::get_bar_time();
            }
            Message::TickToasts => {
                self.notifs.tick_toasts(self.settings.notif_hold_duration_sec);
            }
            Message::TickHardware => {
                let bat = SystemCollector::collect_battery();
                self.telemetry.battery = bat;
                self.telemetry.net = self.system_collector.update_net();

                return Command::perform(
                    async move {
                        let cpu = SystemCollector::collect_cpu().await;
                        let ram = SystemCollector::collect_ram().await;
                        let gpu = SystemCollector::collect_gpu("card0", false, 0).await;
                        let audio = AudioBackend::get_sink().await;
                        let mic = AudioBackend::get_source().await;
                        let mpris = MprisBackend::poll().await;
                        (cpu, ram, gpu, audio, mic, mpris)
                    },
                    |(cpu, ram, gpu, audio, mic, mpris)| {
                        Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris)
                    },
                );
            }
            Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris) => {
                self.telemetry.cpu = cpu;
                self.telemetry.ram = ram;
                self.telemetry.gpu = gpu;
                self.telemetry.audio = audio;
                self.telemetry.mic = mic;
                self.telemetry.mpris = mpris;
            }
            Message::IpcReceived(cmd) => match cmd {
                IpcCommand::Toggle(target) => {
                    if matches!(target, OverlayTarget::Launcher | OverlayTarget::Clipboard | OverlayTarget::NotificationsHistory | OverlayTarget::Settings) {
                        eprintln!("[ipc] toggle {:?}", target);
                        self.toggle_standalone(target);
                    } else if self.active_overlay == Some(target.clone()) {
                        self.active_overlay = None;
                    } else {
                        self.active_overlay = Some(target);
                    }
                }
                IpcCommand::Open(target, arg) => {
                    if matches!(target, OverlayTarget::Launcher | OverlayTarget::Clipboard | OverlayTarget::NotificationsHistory | OverlayTarget::Settings) {
                        self.spawn_standalone(target);
                    } else if matches!(target, OverlayTarget::Annotator) {
                        // Annotator needs fullscreen + exclusive keyboard, which only
                        // the standalone (--annotator-session) process gets via its
                        // LayerShellSettings. Spawn a child with the captured path.
                        let exe = resolve_exe();
                        let mut cmd = std::process::Command::new(exe);
                        cmd.arg("--annotator-session");
                        if let Some(ref p) = arg {
                            cmd.env("MBS_ANNOTATOR_PATH", p);
                        }
                        match cmd.spawn() {
                            Ok(child) => {
                                eprintln!("[annotator] spawned pid={}", child.id());
                                self.standalone_child = Some((OverlayTarget::Annotator, child));
                            }
                            Err(e) => eprintln!("[annotator] spawn failed: {e}"),
                        }
                    } else {
                        self.active_overlay = Some(target);
                    }
                }
                IpcCommand::Close(target) => {
                    if self.active_overlay == Some(target) {
                        self.active_overlay = None;
                    }
                }
                IpcCommand::CloseAll => {
                    self.active_overlay = None;
                }
                IpcCommand::DismissLatestNotif => {
                    if let Some(first) = self.notifs.active_toasts.first().cloned() {
                        self.notifs.update(NotifMessage::DismissToast(first.id));
                    }
                }
                IpcCommand::JumpToLatestNotif => {
                    self.spawn_standalone(OverlayTarget::NotificationsHistory);
                }
                IpcCommand::ToggleHistory => {
                    self.toggle_standalone(OverlayTarget::NotificationsHistory);
                }
                IpcCommand::ReloadSettings => {
                    self.settings = ShellSettings::load();
                    self.theme = ShellTheme::from_settings(&self.settings);
                }
                IpcCommand::TestNotification => {
                    for (title, body) in [
                        ("System Diagnostics #1 (first)", "Verification toast successfully routed."),
                        ("System Diagnostics #2 (second)", "Background worker service nominal."),
                        ("System Diagnostics #3 (third)", "Telemetry stream established."),
                    ] {
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Diagnostics", title, body])
                            .spawn();
                    }
                }
                IpcCommand::IncomingNotif { .. } => {}
                IpcCommand::Ping => {}
            },
            Message::PollIpc => {
                while let Ok(cmd) = self.ipc_rx.try_recv() {
                    let _ = self.update(Message::IpcReceived(cmd));
                }
            }
            Message::TopBar(msg) => {
                if let TopBarMessage::Action(act) = msg {
                    match act {
                        CapsuleAction::OpenOverlay(id) => {
                            let mapped: Option<OverlayTarget> = match id.as_str() {
                                "calendar" | "weather" | "clock" => Some(OverlayTarget::Settings),
                                "alarm" => Some(OverlayTarget::Reminders),
                                "unified" | "net" | "cpu" | "ram" => Some(OverlayTarget::Settings),
                                "notify" => Some(OverlayTarget::NotificationsHistory),
                                "battery" => Some(OverlayTarget::Power),
                                _ => OverlayTarget::from_str(&id),
                            };
                            if let Some(target) = mapped {
                                if matches!(
                                    target,
                                    OverlayTarget::Launcher
                                        | OverlayTarget::Clipboard
                                        | OverlayTarget::NotificationsHistory
                                        | OverlayTarget::Settings
                                ) {
                                    self.spawn_standalone(target);
                                } else {
                                    self.active_overlay = Some(target);
                                }
                            }
                        }
                        CapsuleAction::TogglePlayPause => {
                            return Command::perform(MprisBackend::toggle_play_pause(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::NextTrack => {
                            return Command::perform(MprisBackend::next(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::PreviousTrack => {
                            return Command::perform(MprisBackend::previous(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::CycleAudioSink => {
                            return Command::perform(AudioBackend::cycle_sinks(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::ToggleSinkMute => {
                            return Command::perform(AudioBackend::toggle_sink_mute(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        CapsuleAction::ToggleSourceMute => {
                            return Command::perform(AudioBackend::toggle_source_mute(), |_| Message::TopBar(TopBarMessage::Noop));
                        }
                        _ => {}
                    }
                }
            }
            Message::Launcher(msg) => match msg {
                LauncherMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                LauncherMessage::ExecuteApp(cmd) => {
                    if let Some(target_str) = cmd.strip_prefix("internal:") {
                        if let Some(target) = OverlayTarget::from_str(target_str) {
                            if self.is_standalone {
                                self.spawn_standalone(target);
                                std::process::exit(0);
                            } else {
                                self.active_overlay = Some(target);
                                return Command::none();
                            }
                        }
                    }
                    if self.is_standalone {
                        tokio::spawn(async move { AppLauncher::launch(&cmd).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { AppLauncher::launch(&cmd).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => return self.launcher.update(msg).map(Message::Launcher),
            },
            Message::Calc(msg) => match msg {
                CalcMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                CalcMessage::CopyResult(val) => {
                    if self.is_standalone {
                        tokio::spawn(async move { CalculatorOverlay::copy_to_clipboard(&val).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { CalculatorOverlay::copy_to_clipboard(&val).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => self.calc.update(msg),
            },
            Message::Clipboard(msg) => match msg {
                ClipboardMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                ClipboardMessage::CopyEntry(id) => {
                    if self.is_standalone {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async move { ClipboardOverlay::copy(id).await; });
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { ClipboardOverlay::copy(id).await; }, |_| Message::DismissActiveOverlay);
                }
                ClipboardMessage::DeleteEntry(id) => {
                    return Command::perform(async move { ClipboardOverlay::delete(id).await; }, |_| Message::DismissActiveOverlay);
                }
                ClipboardMessage::WipeAll => {
                    return Command::perform(async move { ClipboardOverlay::wipe().await; }, |_| Message::DismissActiveOverlay);
                }
                ClipboardMessage::SearchChanged(q) => {
                    self.clipboard.search_query = q;
                }
                ClipboardMessage::Reloaded(entries) => {
                    self.clipboard.entries = entries;
                }
                ClipboardMessage::SelectEntry(id) => {
                    self.clipboard.selected_id = Some(id);
                    return Command::perform(
                        ClipboardOverlay::load_preview(id),
                        |(text, image_path)| Message::Clipboard(ClipboardMessage::PreviewLoaded { text, image_path }),
                    );
                }
                ClipboardMessage::PreviewLoaded { text, image_path } => {
                    self.clipboard.preview_text = text;
                    self.clipboard.preview_image_path = image_path;
                }
                _ => {}
            },
            Message::Notes(msg) => match msg {
                NotesMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.notes.update(msg),
            },
            Message::Pass(msg) => match msg {
                PassMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                PassMessage::CopyKey(k) => {
                    if self.is_standalone {
                        tokio::spawn(async move { PassOverlay::copy_password(&k).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { PassOverlay::copy_password(&k).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => self.pass.update(msg),
            },
            Message::Power(msg) => match msg {
                PowerMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => {
                    self.power.update(msg);
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
            },
            Message::Todo(msg) => match msg {
                TodoMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.todo.update(msg),
            },
            Message::Reminders(msg) => match msg {
                ReminderMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.reminders.update(msg),
            },
            Message::Dict(msg) => match msg {
                DictMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                DictMessage::SearchWord => {
                    let w = self.dict.word_query.clone();
                    return Command::perform(async move { (w.clone(), DictionaryOverlay::lookup_word(&w).await) }, |(w, defs)| {
                        Message::Dict(DictMessage::DefinitionsLoaded(w, defs))
                    });
                }
                _ => self.dict.update(msg),
            },
            Message::Unicode(msg) => match msg {
                UnicodeMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                UnicodeMessage::CopySymbol(s) => {
                    if self.is_standalone {
                        tokio::spawn(async move { UnicodeOverlay::copy(s).await; });
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                    return Command::perform(async move { UnicodeOverlay::copy(s).await; }, |_| Message::DismissActiveOverlay);
                }
                _ => self.unicode.update(msg),
            },
            Message::Web(msg) => match msg {
                WebSearchMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                WebSearchMessage::ExecuteSearch => {
                    if let Some(url) = WebSearchOverlay::build_url(&self.web.query) {
                        if self.is_standalone {
                            tokio::spawn(async move { WebSearchOverlay::launch_browser(&url).await; });
                            std::process::exit(0);
                        }
                        self.active_overlay = None;
                        return Command::perform(async move { WebSearchOverlay::launch_browser(&url).await; }, |_| Message::DismissActiveOverlay);
                    }
                }
                _ => self.web.update(msg),
            },
            Message::Annotator(msg) => match msg {
                AnnotatorMessage::ToggleReveal => {
                    self.annotator.reveal_mode = !self.annotator.reveal_mode;
                    if !self.annotator.reveal_mode {
                        self.annotator.status = "Draw · Enter saves & closes · ESC cancels".into();
                    } else if self.annotator.reveal_bytes.is_some() {
                        self.annotator.status = "Reveal: watermark amplified".into();
                    } else if let Some(source) = self.annotator.source_path.clone() {
                        match std::fs::read(&source)
                            .map_err(|e| e.to_string())
                            .and_then(|b| plate_watermark::reveal(&b))
                        {
                            Ok(bytes) => {
                                self.annotator.reveal_handle = Some(iced::widget::image::Handle::from_bytes(bytes.clone()));
                                self.annotator.reveal_bytes = Some(bytes);
                                self.annotator.status = "Reveal: watermark amplified".into();
                            }
                            Err(e) => {
                                self.annotator.status = format!("Reveal failed: {e}");
                                self.annotator.reveal_mode = false;
                            }
                        }
                    } else {
                        self.annotator.status = "Reveal: no source image".into();
                        self.annotator.reveal_mode = false;
                    }
                }
                AnnotatorMessage::Save => {
                    let source = self.annotator.source_path.clone();
                    let annotations = self.annotator.annotations.clone();
                    let watermark = if self.annotator.watermark_input.is_empty() {
                        None
                    } else {
                        Some(self.annotator.watermark_input.clone())
                    };
                    match source {
                        Some(p) => match save_annotated_image(p, annotations, watermark) {
                            Ok(paths) => {
                                eprintln!("[annotator] saved {} file(s)", paths.len());
                                if self.is_standalone { std::process::exit(0); }
                                self.active_overlay = None;
                            }
                            Err(e) => self.annotator.status = format!("Save failed: {e}"),
                        },
                        None => self.annotator.status = "Save failed: no source image".into(),
                    }
                }
                AnnotatorMessage::Cancel => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                AnnotatorMessage::Saved(_) | AnnotatorMessage::RevealLoaded(_) => {}
                other => self.annotator.update(other),
            }
            Message::Amogus(msg) => match msg {
                AmogusMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                AmogusMessage::TriggerScan => {
                    self.amogus.update(AmogusMessage::TriggerScan);
                    return Command::perform(AmogusOverlay::run_scan(), |cols| Message::Amogus(AmogusMessage::ScanFinished(cols)));
                }
                _ => self.amogus.update(msg),
            },
            Message::Rng(msg) => match msg {
                RngMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.rng.update(msg),
            },
            Message::Email(msg) => match msg {
                EmailMessage::Dismiss => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.email.update(msg),
            },
            Message::Notif(msg) => match msg {
                NotifMessage::DismissDrawer => {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.notifs.update(msg),
            },
            Message::Settings(msg) => {
                let is_test = matches!(msg, SettingsMessage::TestNotification);

                if is_test {
                    for (title, body) in [
                        ("System Diagnostics #1 (first)", "Verification toast successfully routed."),
                        ("System Diagnostics #2 (second)", "Background worker service nominal."),
                        ("System Diagnostics #3 (third)", "Telemetry stream established."),
                    ] {
                        let _ = std::process::Command::new("notify-send")
                            .args(["-a", "Diagnostics", title, body])
                            .spawn();
                    }
                } else if matches!(msg, SettingsMessage::Dismiss) {
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                } else {
                    self.settings_ui.update(msg, &mut self.settings);
                    self.theme = ShellTheme::from_settings(&self.settings);

                    if self.is_standalone {
                        tokio::spawn(async {
                            let _ = send_client_command(IpcCommand::ReloadSettings).await;
                        });
                    }
                }
            }
            Message::Lock(msg) => match msg {
                LockMessage::SubmitUnlock => {
                    let pass = self.lockscreen.password_buffer.clone();
                    return Command::perform(async move { LockscreenOverlay::authenticate_pam(&pass).await }, |ok| {
                        if ok { Message::Lock(LockMessage::UnlockSuccess) } else { Message::Lock(LockMessage::UnlockFailed) }
                    });
                }
                LockMessage::UnlockSuccess => {
                    self.lockscreen.update(msg);
                    if self.is_standalone { std::process::exit(0); }
                    self.active_overlay = None;
                }
                _ => self.lockscreen.update(msg),
            },
            Message::DismissActiveOverlay => {
                if self.is_standalone {
                    std::thread::spawn(|| {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        std::process::exit(0);
                    });
                    return Command::none();
                }
                self.active_overlay = None;
            }
            Message::CloseShell => {
                std::process::exit(0);
            }
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let clock_sub = iced::time::every(std::time::Duration::from_millis(1000)).map(|_| Message::TickClock);
        let toast_sub = if self.is_notif_surface && !self.notifs.active_toasts.is_empty() {
            iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::TickToasts)
        } else {
            iced::Subscription::none()
        };
        let hw_sub = iced::time::every(std::time::Duration::from_millis(self.settings.hardware_poll_interval_ms)).map(|_| Message::TickHardware);

        let esc_sub = iced::keyboard::on_key_press(|key, _modifiers| {
            if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                Some(Message::DismissActiveOverlay)
            } else {
                None
            }
        });

        // Enter → save when annotator overlay is active.
        let enter_sub = if matches!(self.active_overlay, Some(OverlayTarget::Annotator)) {
            iced::keyboard::on_key_press(|key, _mods| match key {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                    Some(Message::Annotator(AnnotatorMessage::Save))
                }
                _ => None,
            })
        } else {
            iced::Subscription::none()
        };

        let ipc_sub = if self.is_standalone || self.is_notif_surface {
            iced::Subscription::none()
        } else {
            iced::time::every(std::time::Duration::from_millis(50))
                .map(|_| Message::PollIpc)
        };

        let notif_sub = if self.is_notif_surface {
            iced::time::every(std::time::Duration::from_millis(50))
                .map(|_| Message::PollNotifs)
        } else if !self.is_standalone {
            iced::time::every(std::time::Duration::from_millis(500))
                .map(|_| Message::PollNotifs)
        } else {
            iced::Subscription::none()
        };
        let resize_sub = iced::window::resize_events()
            .map(|(_id, size)| Message::WindowResized(size.width));
        iced::Subscription::batch(vec![clock_sub, toast_sub, hw_sub, esc_sub,
            enter_sub, ipc_sub, notif_sub, resize_sub])
    }

    fn view(&self) -> Element<'_, Message> {
        if self.is_notif_surface {
            let is_right_corner = self.settings.notif_corner.contains("right");
            let is_top_corner = self.settings.notif_corner.contains("top");
            let pad_top = if is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
            let pad_bottom = if !is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
            let pad_right = if is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };
            let pad_left = if !is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };
            return container(self.notifs.view_toasts(&self.theme, &self.settings, self.window_w).map(Message::Notif))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(if is_right_corner {
                    iced::alignment::Horizontal::Right
                } else {
                    iced::alignment::Horizontal::Left
                })
                .align_y(if is_top_corner {
                    iced::alignment::Vertical::Top
                } else {
                    iced::alignment::Vertical::Bottom
                })
                .padding(Padding { top: pad_top, right: pad_right, bottom: pad_bottom, left: pad_left })
                .into();
        }
        let is_right_corner = self.settings.notif_corner.contains("right");
        let is_top_corner = self.settings.notif_corner.contains("top");

        let pad_top = if is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
        let pad_bottom = if !is_top_corner { self.settings.notif_baseline_y as f32 } else { 0.0 };
        let pad_right = if is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };
        let pad_left = if !is_right_corner { self.settings.notif_margin_x.max(16) as f32 } else { 0.0 };

        let notifs_view = container(self.notifs.view_toasts(&self.theme, &self.settings, self.window_w).map(Message::Notif))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(if is_right_corner { iced::alignment::Horizontal::Right } else { iced::alignment::Horizontal::Left })
            .align_y(if is_top_corner { iced::alignment::Vertical::Top } else { iced::alignment::Vertical::Bottom })
            .padding(Padding {
                top: pad_top,
                right: pad_right,
                bottom: pad_bottom,
                left: pad_left,
            });

        let overlay_element: Option<Element<'_, Message>> = match &self.active_overlay {
            Some(OverlayTarget::Launcher) => Some(self.launcher.view(&self.theme, &self.settings).map(Message::Launcher)),
            Some(OverlayTarget::Calc) => Some(self.calc.view(&self.theme, &self.settings).map(Message::Calc)),
            Some(OverlayTarget::Clipboard) => Some(self.clipboard.view(&self.theme, &self.settings).map(Message::Clipboard)),
            Some(OverlayTarget::Notes) => Some(self.notes.view(&self.theme).map(Message::Notes)),
            Some(OverlayTarget::Pass) => Some(self.pass.view(&self.theme).map(Message::Pass)),
            Some(OverlayTarget::Power) => Some(self.power.view(&self.theme).map(Message::Power)),
            Some(OverlayTarget::Todo) => Some(self.todo.view(&self.theme).map(Message::Todo)),
            Some(OverlayTarget::Reminders) => Some(self.reminders.view(&self.theme).map(Message::Reminders)),
            Some(OverlayTarget::Dictionary) => Some(self.dict.view(&self.theme).map(Message::Dict)),
            Some(OverlayTarget::Unicode) => Some(self.unicode.view(&self.theme).map(Message::Unicode)),
            Some(OverlayTarget::Web) => Some(self.web.view(&self.theme).map(Message::Web)),
            Some(OverlayTarget::Amogus) => Some(self.amogus.view(&self.theme).map(Message::Amogus)),
            Some(OverlayTarget::Rng) => Some(self.rng.view(&self.theme).map(Message::Rng)),
            Some(OverlayTarget::Email) => Some(self.email.view(&self.theme).map(Message::Email)),
            Some(OverlayTarget::NotificationsHistory) => Some(self.notifs.view_history(&self.theme).map(Message::Notif)),
            Some(OverlayTarget::Settings) => Some(self.settings_ui.view(&self.settings, &self.theme).map(Message::Settings)),
            Some(OverlayTarget::Lockscreen) => Some(self.lockscreen.view(&self.theme).map(Message::Lock)),
            Some(OverlayTarget::Annotator) => Some(self.annotator.view().map(Message::Annotator)),
            None => None,
        };

        if self.is_standalone {
            if let Some(overlay) = overlay_element {
                return Stack::new().push(overlay).push(notifs_view).into();
            } else {
                std::process::exit(0);
            }
        }

        let bar_view = TopBar::view(&self.settings, &self.theme, &self.telemetry).map(Message::TopBar);
        let _ = &notifs_view;
        let base_content = Stack::new().push(bar_view);

        if let Some(overlay) = overlay_element {
            Stack::new().push(base_content).push(overlay).into()
        } else {
            base_content.into()
        }
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}
