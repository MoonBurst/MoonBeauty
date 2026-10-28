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
                    .args(["-KILL", "-f", "--", "--lockscreen-instance"])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn spawn_standalone(&mut self, target: OverlayTarget) {
        self.kill_standalone();
        let exe = resolve_exe();
        let tag = target.as_arg();
        let log_path = format!("/tmp/mbs-standalone-{}.log", tag);
        let stderr_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&log_path)
            .ok();
        let stdout_file = stderr_file.as_ref().and_then(|f| f.try_clone().ok());

        let mut cmd = std::process::Command::new(exe);
        cmd.arg(format!("--{}", tag));
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
                    log_path
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
