use super::*;

impl MoonBeautyShellApp {
    pub fn close_tray(&mut self) {
        eprintln!(
            "[close-tray] invoked: expanded={} width={:.1} target_set={} (settings.tray_width)",
            self.tray_expanded, self.tray_anim_width, self.settings.tray_width
        );
        if self.tray_expanded {
            self.tray_expanded = false;
            self.tray_anim_start_width = self.tray_anim_width;
            self.tray_anim_started_at = std::time::Instant::now();
            self.tray_target_width = self.settings.tray_width as f32;
            self.tray_anim_duration_ms = 180;
        }
        // Invalidate any pending timeout timers
        self.tray_timeout_gen = self.tray_timeout_gen.wrapping_add(1);
    }

    pub fn standalone_running(&mut self) -> Option<OverlayTarget> {
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

    pub fn kill_standalone(&mut self) {
        if let Some((target, mut child)) = self.standalone_child.take() {
            // For lockscreen, the coordinator normally kills its own
            // children on clean exit — but SIGKILL bypasses that path,
            // leaving orphan `--lockscreen-instance` children holding
            // dead sockets. Explicitly pkill them so the fresh coordinator
            // binds cleanly without competing for the same outputs.
            if matches!(target, OverlayTarget::Lockscreen) {
                let _ = std::process::Command::new("pkill")
                    .args([
                        "-KILL",
                        "-f",
                        "--",
                        "(^|/)(moonbeauty|moon-beauty-shell) --lockscreen-instance( .*)?$",
                    ])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Toggle the tooltip for `id` (bar capsule click handler).
    ///
    /// Extracted from the TopBar `OpenOverlay` arm (tracker item
    /// `topbar-sub-extraction.md`, patch 1 of 3). Handles four
    /// concerns that were inline:
    ///   1. Toggle-off: kill existing tooltip if already open.
    ///   2. Positioning: cursor position (if recent click) vs
    ///      capsule_center_x. Right-side capsules use center (see
    ///      Finding 78 comment preserved below).
    ///   3. Slant selection: from settings.slant_mode plus
    ///      left/center/right membership (center uses keystone).
    ///   4. Spawn: SpawnRequest with per-capsule tooltip_bias,
    ///      height from compute_height (derived from telemetry),
    ///      output from settings.bar_monitor or MBS_OUTPUT/primary.
    ///
    /// The id clone at the top is paid once per user click (sub-Hz
    /// even with rapid clicking). Preserving `id` as String inside
    /// the body keeps the extracted logic byte-for-byte identical
    /// to the inline version; no per-site &str/String juggling.
    pub fn toggle_tooltip(&mut self, id: &str) -> Command<Message> {
        let id = id.to_string();
        // Recent click => use cursor position for dynamic placement.
        // Fallback to capsule_center_x if no click in last 5s (e.g. IPC trigger).
        let recent_click =
            self.last_cursor_time.elapsed() < std::time::Duration::from_secs(5);
        // Toggle off if already open
        let running = self
            .weather_tooltip_child
            .as_mut()
            .and_then(|c| c.try_wait().ok())
            .is_some_and(|status| status.is_none());
        if running {
            eprintln!("[tooltip] Already open; toggling off");
            if let Some(mut c) = self.weather_tooltip_child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            return Command::none();
        }
        self.weather_tooltip_child = None;

        let target_output = if self.settings.notif_screen_name != "Auto"
            && self.settings.notif_screen_name != "Auto (Primary)"
            && !self.settings.notif_screen_name.is_empty()
        {
            Some(self.settings.notif_screen_name.as_str())
        } else {
            None
        };
        let sw = get_screen_width_for(target_output);
        let in_left = self.settings.bar_left.contains(&id);
        let in_center = self.settings.bar_center.contains(&id);
        let in_right = self.settings.bar_right.contains(&id);
        // Finding 78: right-side capsules use capsule_center_x, not
        // last_cursor_x. Right-anchored math is
        //   margin_right = sw - cx - half_cap + adjust_right
        // which assumes cx = capsule center. If cx = click_x (near
        // capsule right edge), tooltip lands half_cap - adjust past
        // capsule_right — off-screen at larger font sizes. Left-anchored
        // works because the clamp catches the (cx - half_cap) overshoot.
        let cx = if id == "clock" {
            capsule_center_x(&self.settings, sw, &id)
        } else if recent_click && self.last_cursor_x > 0.0 && in_left {
            self.last_cursor_x as i32
        } else {
            capsule_center_x(&self.settings, sw, &id)
        };
        let (slant_l, slant_r): (&str, &str) =
            match self.settings.slant_mode.as_str() {
                "left" => ("L", "L"),
                "right" => ("R", "R"),
                _ => {
                    if in_center {
                        let vis: Vec<&String> = self
                            .settings
                            .bar_center
                            .iter()
                            .filter(|x| {
                                self.settings
                                    .visible_capsules
                                    .get(*x)
                                    .copied()
                                    .unwrap_or(true)
                            })
                            .collect();
                        let k = vis.len() / 2;
                        match vis.iter().position(|x| **x == id) {
                            Some(i) if i == k => ("L", "R"),
                            Some(i) if i < k => ("L", "L"),
                            Some(_) => ("R", "R"),
                            None => ("L", "R"),
                        }
                    } else if in_right {
                        ("R", "R")
                    } else {
                        ("L", "L")
                    }
                }
            };
        let cap_w = capsule_actual_width(&id);
        let grow_right = if in_left {
            true
        } else if in_right {
            false
        } else {
            cx < (sw as i32) / 2
        };
        let (tip_w, _tip_h) = self.settings.tooltip_size(&id);
        let tip_output = if self.settings.bar_monitor != "All"
            && !self.settings.bar_monitor.is_empty()
        {
            Some(self.settings.bar_monitor.clone())
        } else {
            std::env::var("MBS_OUTPUT")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(resolve_primary_output)
        };

        // Finding 76: per-capsule tooltip bias.
        let (bias_l, bias_r) =
            moonbeauty::bar::capsules::tooltip_bias(
                &id,
                &self.theme,
            );
        let req = moonbeauty::bar::tooltip_spawn::SpawnRequest {
            id: &id,
            exe: resolve_exe(),
            output: tip_output.as_deref(),
            tooltip_w: tip_w,
            tooltip_h:
                moonbeauty::bar::tooltip_spawn::compute_height(
                    &id,
                    &self.settings,
                    &self.telemetry,
                ),
            drop_ms: self.settings.tooltip_drop_ms,
            expand_ms: self.settings.tooltip_expand_ms,
            capsule_cx: cx,
            capsule_w: cap_w,
            screen_w: sw,
            slant_l,
            slant_r,
            grow_right,
            offset_left_extra: bias_l,
            offset_right_extra: bias_r,
        };

        match moonbeauty::bar::tooltip_spawn::spawn(&req) {
            Ok(c) => {
                eprintln!("[ipc] spawned tooltip pid={}", c.id());
                self.weather_tooltip_child = Some(c);
            }
            Err(e) => eprintln!("[tooltip] Failed to spawn: {e}"),
        }
        Command::none()
    }

    /// Handle a bar capsule action (bar-side click handler dispatch).
    ///
    /// Extracted from the TopBar `Action` arm (tracker item
    /// `topbar-sub-extraction.md`, patch 2 of 3). Contains every
    /// CapsuleAction variant handler: tooltip open (via
    /// toggle_tooltip), music control, audio mute/cycle, notification
    /// DND, tooltip cancel, tray expand/activate/context-menu, GPU
    /// cycle. Arms that mutate state without returning a Command
    /// fall through to `Command::none()`.
    pub fn handle_capsule_action(
        &mut self,
        act: CapsuleAction,
    ) -> Command<Message> {
        match act {
            CapsuleAction::OpenOverlay(id) => {
                eprintln!("[topbar-action] OpenOverlay: id = {:?}", id);

                // 1. ALARM — standalone reminders overlay.
                // Handled BEFORE the tooltip-open/toggle block so:
                //   (a) an open tooltip can't hijack the click,
                //   (b) id="alarm" never enters the tooltip spawn
                //       selection and falls through to --sys-tooltip
                //       (which defaults unknown targets to RAM).
                if id == "alarm" {
                    if let Some(mut c) = self.weather_tooltip_child.take() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    self.toggle_standalone(OverlayTarget::Reminders);
                    return Command::none();
                }

                // 2. DROPDOWN TOOLTIPS (CPU, GPU, RAM, Tray)
                // Helper handles toggle-off-if-open, cursor-vs-
                // capsule-center positioning, slant selection,
                // per-capsule bias, spawn, and error logging.
                // See app::helpers::toggle_tooltip.
                if matches!(
                    id.as_str(),
                    "weather"
                        | "cpu"
                        | "gpu"
                        | "ram"
                        | "calendar"
                        | "music"
                        | "unified"
                        | "net"
                        | "custom"
                        | "clock"
                ) {
                    return self.toggle_tooltip(&id);
                }
                // 2. FULLSCREEN / STANDALONE OVERLAYS (Settings, History, Power)
                let mapped: Option<OverlayTarget> = match id.as_str() {
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
            CapsuleAction::TogglePlayPause => return self.music_toggle_play_pause(),
            CapsuleAction::NextTrack => return self.music_next_track(),
            CapsuleAction::PreviousTrack => return self.music_previous_track(),
            CapsuleAction::CycleAudioSink => {
                return Command::perform(AudioBackend::cycle_sinks(), |_| {
                    Message::TopBar(TopBarMessage::Noop)
                });
            }
            CapsuleAction::ToggleSinkMute => {
                return Command::perform(AudioBackend::toggle_sink_mute(), |_| {
                    Message::TopBar(TopBarMessage::Noop)
                });
            }
            CapsuleAction::ToggleNotificationDnd => return self.toggle_notification_dnd(),
            CapsuleAction::CancelTooltip(id) => {
                eprintln!("[topbar] CancelTooltip: {}", id);
                if let Some(mut c) = self.weather_tooltip_child.take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                // Right-click on alarm capsule: wipe active timers
                if id == "alarm" {
                    moonbeauty::overlays::reminders::save_reminders(&[]);
                    eprintln!("[topbar] cleared active timers");
                }
            }
            CapsuleAction::ToggleSourceMute => {
                return Command::perform(AudioBackend::toggle_source_mute(), |_| {
                    Message::TopBar(TopBarMessage::Noop)
                });
            }
            CapsuleAction::ToggleTrayExpanded => {
                if !self.tray_expanded {
                    // OPENING
                    self.tray_timeout_gen = self.tray_timeout_gen.wrapping_add(1);
                    let _my_gen = self.tray_timeout_gen;

                    self.tray_opened_at = std::time::Instant::now();
                    self.tray_expanded = true;
                    self.tray_anim_start_width = self.tray_anim_width;
                    self.tray_anim_started_at = std::time::Instant::now();
                    let expanded_w = moonbeauty::bar::capsules::compute_expanded_visible_width(
                        &self.settings, self.tray_items.len()
                    );
                    self.tray_target_width = expanded_w;
                    self.tray_anim_duration_ms = 220;

                    // Auto-close is handled by the tray_watchdog_sub
                    // subscription (see subscription()), which polls
                    // tray_opened_at.elapsed() every 500ms while
                    // tray_expanded is true. No per-open timer needed.
                    return Command::perform(
                        moonbeauty::bar::tray::watcher::fetch_tray_items(),
                        Message::TrayItemsLoaded,
                    );
                } else {
                    // CLOSING — bump gen to invalidate any pending timer
                    self.close_tray();
                }
            }
            CapsuleAction::TrayActivate(item) => {
                self.close_tray();
                return Command::perform(
                    async move {
                        let _ =
                            moonbeauty::bar::tray::watcher::activate_tray_item(
                                &item.bus, &item.path,
                            )
                            .await;
                    },
                    |_| Message::TopBar(TopBarMessage::Noop),
                );
            }
            CapsuleAction::TrayContextMenu(item) => {
                let item_clone = item.clone();
                return Command::perform(
                    async move {
                        if let Some((menu_path, items)) =
                            moonbeauty::bar::tray::watcher::get_menu_tree(
                                &item_clone.bus,
                                &item_clone.path,
                            )
                            .await
                        {
                            let json = serde_json::to_string(&items).unwrap();
                            let tmp_path =
                                moonbeauty::libraries::process::mbs_unique_tmp(
                                    "mbs-tray-menu",
                                    "json",
                                );
                            if let Err(e) =
                                moonbeauty::libraries::process::private_write_new(
                                    &tmp_path,
                                    json.as_bytes(),
                                )
                            {
                                eprintln!("[tray-menu] write failed: {e}");
                                return;
                            }
                            let exe = resolve_exe();
                            let h = (items.len() as u32 * 36).clamp(100, 600) + 40;
                            let _ = std::process::Command::new(exe)
                                .args([
                                    "--tray-context-menu",
                                    tmp_path.to_str().unwrap_or(""),
                                    &item_clone.bus,
                                    &menu_path,
                                    "250",
                                    &h.to_string(),
                                ])
                                .spawn();
                        }
                    },
                    |_| Message::TopBar(TopBarMessage::Noop),
                );
            }
            CapsuleAction::CycleGpuTarget => {
                let gpus = &self.detected_gpus;
                if !gpus.is_empty() {
                    let current = &self.settings.gpu_selected_id;
                    let next_idx = gpus
                        .iter()
                        .position(|g| &g.card_id == current)
                        .map(|i| (i + 1) % gpus.len())
                        .unwrap_or(0);
                    self.settings.gpu_selected_id = gpus[next_idx].card_id.clone();
                    let _ = self.settings.save();
                    eprintln!(
                        "[bar] right-click GPU: switched to {}",
                        self.settings.gpu_selected_id
                    );
                    return Command::done(Message::TickHardware);
                }
            }
            _ => {}
        }
        Command::none()
    }

    pub fn spawn_standalone(&mut self, target: OverlayTarget) {
        self.kill_standalone();
        moonbeauty::libraries::process::prune_standalone_logs();
        let exe = resolve_exe();
        let tag = target.as_arg();
        let log_path =
            moonbeauty::libraries::process::mbs_unique_tmp("mbs-standalone", "log");
        let stderr_file = moonbeauty::libraries::process::private_create_new(&log_path).ok();
        let stdout_file = stderr_file.as_ref().and_then(|f| f.try_clone().ok());

        let mut cmd = std::process::Command::new(exe);
        if matches!(target, OverlayTarget::Lockscreen) {
            // Lockscreen is not a normal overlay. --overlay spawns a
            // standalone shell that renders a UI, but the lockscreen UI
            // is only useful when a coordinator exists to receive key
            // events and run authenticate(). main.rs only starts the
            // coordinator when `--lockscreen` is present (bare flag,
            // not `--overlay lockscreen`). Passing --overlay therefore
            // spawned a LockscreenOverlay with ipc_tx = None: the UI
            // rendered, but every Enter was silently dropped in
            // forward_key() because there was no coordinator to submit
            // to. The correct spawn is the coordinator, which then
            // spawns per-output --lockscreen-instance children that
            // connect to the coordinator socket.
            cmd.arg("--lockscreen");
        } else {
            cmd.arg("--overlay");
            cmd.arg(tag);
        }
        if let Some(sf) = stderr_file {
            cmd.stderr(sf);
        }
        if let Some(of) = stdout_file {
            cmd.stdout(of);
        }

        match cmd.spawn() {
            Ok(c) => {
                eprintln!(
                    "[ipc] spawned {:?} pid={} (log: {})",
                    target,
                    c.id(),
                    log_path.display()
                );
                self.standalone_child = Some((target, c));
            }
            Err(e) => eprintln!("[ipc] spawn {:?} failed: {e}", target),
        }
    }

    pub fn toggle_standalone(&mut self, target: OverlayTarget) {
        match self.standalone_running() {
            Some(ref t) if t == &target => self.kill_standalone(),
            _ => self.spawn_standalone(target),
        }
    }

    pub fn compute_toast_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        let s = &self.settings;
        let is_right = s.notif_corner.contains("right");
        let is_top = s.notif_corner.contains("top");

        let card_w = s.notif_card_width as i32;
        let card_h = s.notif_card_height as i32;
        let margin_x = s.notif_margin_x.max(16);
        let baseline = s.notif_baseline_y;
        let surface_w = (s.notif_card_width as i32 + 2 * s.notif_margin_x + 100).max(200);

        let pad_top = if is_top { baseline } else { 0 };
        let pad_left = if is_right { 0 } else { margin_x };
        let pad_right = if is_right { margin_x } else { 0 };

        let peek = s.notif_stack_overlap.abs().max(4);
        let drop_dist = s.notif_drop_distance_px as i32;
        let fixed_base = drop_dist + 4 * peek;

        let x_surface = if is_right {
            surface_w - pad_right - card_w
        } else {
            pad_left
        };

        let n = self.notifs.active_toasts.len();
        let mut rects = Vec::with_capacity(n);
        for i in 0..n {
            let y_surface = pad_top + fixed_base - (i as i32) * peek;
            rects.push((x_surface, y_surface, card_w, card_h));
        }
        rects
    }
}

/// Run a standalone-overlay copy to completion, then exit the process.
///
/// The `if self.is_standalone` branches in update handlers need to
/// perform async work (wl-copy, pass -c) BEFORE exiting, otherwise
/// the copy is lost. Historically they used
/// `Runtime::new().unwrap()`, which panics on rare failures (FD
/// exhaustion, fork-without-exec state from a signal handler). That
/// is a hard crash for what is meant to be a graceful "copy and
/// close" operation.
///
/// On runtime creation failure: log a diagnostic and exit anyway.
/// The copy is best-effort; the overlay still closes. A panic here
/// is worse than a silent skip — it takes down the whole process
/// including any state the user was in the middle of.
///
/// Prefer `if let Ok(rt)` pattern used in bar/tray/context.rs, but
/// codified here so every copy site shares the same fallback
/// behavior and diagnostic tag.
pub(crate) fn run_then_exit<F>(future: F) -> !
where
    F: std::future::Future<Output = ()>,
{
    match tokio::runtime::Runtime::new() {
        Ok(rt) => rt.block_on(future),
        Err(e) => eprintln!(
            "[standalone-copy] failed to create tokio runtime: {e} \
             (copy skipped, exiting anyway)"
        ),
    }
    std::process::exit(0)
}
