use super::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_tray_watchdog(&mut self) -> Command<Message> {
        // 1) Timeout check
        if self.tray_expanded
            && self.settings.tray_timeout_sec > 0
            && self.tray_opened_at.elapsed().as_secs() >= self.settings.tray_timeout_sec as u64
        {
            eprintln!(
                "[tray-watchdog] elapsed={}s >= timeout={}s, closing (settings.tray_width={})",
                self.tray_opened_at.elapsed().as_secs(),
                self.settings.tray_timeout_sec,
                self.settings.tray_width
            );
            self.close_tray();
        }

        // 2) Animation step. Note: the completion check is OUTSIDE
        //    the `elapsed < duration` guard so the snap-to-target
        //    ALWAYS runs on the first tick past duration. Previous
        //    version froze at width=89.7 because once elapsed
        //    reached 180ms, the guard `elapsed < duration` became
        //    false and the inner `if t >= 1.0 { snap }` never ran.
        if (self.tray_anim_width - self.tray_target_width).abs() > 0.01 {
            let elapsed_ms = self.tray_anim_started_at.elapsed().as_millis() as u64;
            let dur = self.tray_anim_duration_ms.max(1);
            let t = (elapsed_ms as f32 / dur as f32).clamp(0.0, 1.0);
            let expanding = self.tray_target_width > self.tray_anim_start_width;
            let eased = if expanding {
                1.0 - (1.0 - t).powi(3)
            } else {
                t * t * t
            };
            let delta = self.tray_target_width - self.tray_anim_start_width;
            self.tray_anim_width = self.tray_anim_start_width + delta * eased;
            // Force-snap when past duration OR t is at ceiling.
            if elapsed_ms >= dur || t >= 1.0 {
                self.tray_anim_width = self.tray_target_width;
            }
            moonbeauty::dbg_log!(
                "[tray-anim] elapsed={}ms t={:.2} width={:.1} target={:.1}",
                elapsed_ms,
                t,
                self.tray_anim_width,
                self.tray_target_width
            );
        }

        Command::none()
    }

    pub(crate) fn handle_tray_anim(&mut self) -> Command<Message> {
        let elapsed_ms = self.tray_anim_started_at.elapsed().as_millis() as f32;
        let t = (elapsed_ms / self.tray_anim_duration_ms.max(1) as f32).clamp(0.0, 1.0);

        // Expand: ease-out-cubic (fast start, smooth settle)
        // Collapse: ease-in-cubic (slow start, snap finish)
        let expanding = self.tray_target_width > self.tray_anim_start_width;
        let eased = if expanding {
            1.0 - (1.0 - t).powi(3)
        } else {
            t * t * t
        };

        let delta = self.tray_target_width - self.tray_anim_start_width;
        self.tray_anim_width = self.tray_anim_start_width + delta * eased;

        if t >= 1.0 {
            self.tray_anim_width = self.tray_target_width;
        }

        Command::none()
    }

    pub(crate) fn handle_fetch_tray_items(&mut self) -> Command<Message> {
        // Background refresh so tray_items is populated before
        // the user ever clicks the tray capsule. Eliminates the
        // 0-item → N-item re-animation on first open.
        return Command::perform(
            moonbeauty::bar::tray::watcher::fetch_tray_items(),
            Message::TrayItemsLoaded,
        );
    }

    pub(crate) fn handle_tray_items_loaded(
        &mut self,
        items: Vec<moonbeauty::bar::tray::TrayItem>,
    ) -> Command<Message> {
        self.tray_items = items;
        if self.tray_expanded {
            let new_target = moonbeauty::bar::capsules::compute_expanded_visible_width(
                &self.settings,
                self.tray_items.len(),
            );
            // Re-anchor if target changed mid-animation
            if (new_target - self.tray_target_width).abs() > 1.0 {
                self.tray_anim_start_width = self.tray_anim_width;
                self.tray_anim_started_at = std::time::Instant::now();
                self.tray_target_width = new_target;
            }
        }

        Command::none()
    }
}
