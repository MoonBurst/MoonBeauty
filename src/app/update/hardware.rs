use super::*;
use moonbeauty::bar::weather::WeatherData;
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
        // #3: update_net() blocks on TcpStream::connect_timeout
        // (ping check) and on an `ss` subprocess. Both used to
        // run on the UI thread before this Command was even
        // constructed, freezing the shell for up to ~450 ms
        // every ping-cache cycle. Moving them into the async
        // task on a dedicated blocking thread eliminates the
        // stall.
        let net_collector = self.system_collector.clone();

        return Command::perform(
            async move {
                let (cpu, ram) = SystemCollector::collect_all().await;
                let gpu = SystemCollector::collect_gpu_by_card_id(&gpu_id).await;
                let audio = AudioBackend::get_sink().await;
                let mic = AudioBackend::get_source().await;
                // Finding 74: MPD-priority-over-MPRIS. Prefer the
                // dedicated music daemon over incidental browser
                // media when both are Playing.
                let (mpris, _source) = moonbeauty::libraries::backend::poll_track().await;
                // #3: blocking net poll on a dedicated thread,
                // not the async executor or the UI thread.
                let net =
                    tokio::task::spawn_blocking(move || net_collector.lock().unwrap().update_net())
                        .await
                        .unwrap_or_default();
                (cpu, ram, gpu, audio, mic, mpris, net)
            },
            |(cpu, ram, gpu, audio, mic, mpris, net)| {
                Message::MetricsCollected(cpu, ram, gpu, audio, mic, mpris, net)
            },
        );
    }

    pub(crate) fn handle_weather_loaded(
        &mut self,
        w: Option<moonbeauty::bar::weather::WeatherData>,
    ) -> Command<Message> {
        self.telemetry.weather = w;
        Command::none()
    }

    pub(crate) fn handle_tick_weather(&mut self) -> Command<Message> {
        return Command::perform(
            async { moonbeauty::bar::weather::WeatherBackend::fetch().await.ok() },
            Message::WeatherLoaded,
        );
    }

    pub(crate) fn handle_tick_clock(&mut self) -> Command<Message> {
        self.telemetry.bar_time = ClockBackend::get_bar_time();
        return self.check_reminder_alerts();
    }

    pub(crate) fn handle_idle_tick(&mut self) -> Command<Message> {
        // Reap self-terminating tooltip children. Tooltips
        // self-exit via process::exit(0) on AUTO_MS timeout
        // (20s weather, 25s tray) or user dismissal. Without
        // this try_wait, Child's Drop does NOT reap (Rust
        // docs: no Drop impl for Child), so each self-exit
        // leaves a zombie accumulating over hours of use.
        // try_wait also clears the handle so the next toggle
        // starts from a clean slate rather than take()-ing a
        // dead child.
        if let Some(c) = self.weather_tooltip_child.as_mut() {
            if matches!(c.try_wait(), Ok(Some(_))) {
                self.weather_tooltip_child = None;
            }
        }
        let c1 = self.update(Message::TickClock);
        let c2 = self.update(Message::TickHardware);
        let c3 = self.update(Message::PollNotifs);
        return Command::batch(vec![c1, c2, c3]);
    }
}
