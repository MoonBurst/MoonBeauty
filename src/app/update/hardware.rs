use super::*;
use moonbeauty::libraries::backend::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_metrics_collected(
        &mut self,
        cpu: CpuMetrics,
        ram: RamMetrics,
        gpu: GpuMetrics,
        audio: AudioStatus,
        mic: AudioStatus,
        mpris: Option<MprisTrack>,
        net: NetMetrics,
    ) -> Command<Message> {
        self.telemetry.cpu = cpu;
        self.telemetry.ram = ram;
        self.telemetry.gpu = gpu;
        self.telemetry.audio = audio;
        self.telemetry.mic = mic;
        self.telemetry.mpris = mpris;
        self.telemetry.net = net;
        Command::none()
    }

    pub(crate) fn handle_tick_hardware(&mut self) -> Command<Message> {
        self.telemetry.gpu_selected_idx = self
            .detected_gpus
            .iter()
            .position(|g| g.card_id == self.settings.gpu_selected_id)
            .unwrap_or(0);
        let bat = SystemCollector::collect_battery();
        self.telemetry.battery = bat;

        let gpu_id = self.settings.gpu_selected_id.clone();
        // update_net() blocks on TcpStream::connect_timeout and an `ss` subprocess,
        // so run it inside the async task on a blocking thread instead of the UI thread.
        let net_collector = self.system_collector.clone();

        Command::perform(
            async move {
                let (cpu, ram) = SystemCollector::collect_all().await;
                let gpu = SystemCollector::collect_gpu_by_card_id(&gpu_id).await;
                let audio = AudioBackend::get_sink().await;
                let mic = AudioBackend::get_source().await;
                // Prefer the dedicated music daemon over incidental browser media when both are playing.
                let (mpris, _source) = moonbeauty::libraries::backend::poll_track().await;
                // Blocking net poll on a dedicated thread, not the async executor or UI thread.
                let net =
                    tokio::task::spawn_blocking(move || net_collector.lock().unwrap().update_net())
                        .await
                        .unwrap_or_default();
                (cpu, ram, gpu, audio, mic, mpris, net)
            },
            |(cpu, ram, gpu, audio, mic, mpris, net)| {
                Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris, net)
            },
        )
    }

    pub(crate) fn handle_weather_loaded(
        &mut self,
        w: Option<moonbeauty::bar::weather::WeatherData>,
    ) -> Command<Message> {
        self.telemetry.weather = w;
        Command::none()
    }

    pub(crate) fn handle_tick_weather(&mut self) -> Command<Message> {
        Command::perform(
            async { moonbeauty::bar::weather::WeatherBackend::fetch().await.ok() },
            Message::WeatherLoaded,
        )
    }

    pub(crate) fn handle_tick_clock(&mut self) -> Command<Message> {
        self.telemetry.bar_time = ClockBackend::get_bar_time();
        if !self.is_standalone && !self.is_notif_surface {
            let (paused, count) = moonbeauty::overlays::notifications::read_bar_status();
            self.telemetry.notify_paused = paused;
            self.telemetry.notify_count = count;
        }
        self.check_reminder_alerts()
    }

    pub(crate) fn handle_idle_tick(&mut self) -> Command<Message> {
        // Reap zombie standalone overlays so self-exiting children don't
        // stay defunct until the next spawn of the same target.
        if let Some((_, c)) = self.standalone_child.as_mut() {
            if matches!(c.try_wait(), Ok(Some(_))) {
                self.standalone_child = None;
            }
        }
        let dead = match self.notif_surface_child.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(Some(_))),
            None => true,
        };
        if dead {
            self.notif_surface_child = crate::app::helpers::spawn_notif_surface_child();
        }
        let mtime = std::fs::metadata(ShellSettings::config_path())
            .ok()
            .and_then(|m| m.modified().ok());
        if mtime != self.settings_json_mtime {
            self.settings_json_mtime = mtime;
            let prev_screen = self.settings.notif_screen_name.clone();
            let prev_corner = self.settings.notif_corner.clone();
            self.settings = ShellSettings::load();
            self.theme = ShellTheme::from_settings(&self.settings);
            if self.settings.notif_screen_name != prev_screen
                || self.settings.notif_corner != prev_corner
            {
                self.notif_screen_at_startup = self.settings.notif_screen_name.clone();
                self.restart_notif_surface();
            }
        }
        let c1 = self.update(Message::TickClock);
        let c2 = self.update(Message::TickHardware);
        Command::batch(vec![c1, c2])
    }
}
