use chrono::Local;
use iced::Element;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use super::icon_resolve::{avatar_path_for, resolve_desktop_icon, resolve_icon_name};
use super::view::{history_icon_hex_size, history_row_height, HISTORY_ROW_GAP};
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::AngledHexGeometry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationToast {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub timestamp: String,
    pub is_read: bool,
    pub is_urgent: bool,
    #[serde(default)]
    pub is_resident: bool,
    pub border_color: String,
    #[serde(default = "default_toast_text_color")]
    pub text_color: String,
    #[serde(default)]
    pub avatar_path: Option<String>,
    #[serde(default)]
    pub desktop_entry: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip)]
    pub exit_started_at: Option<Instant>,
    #[serde(skip)]
    pub hold_reset_at: Option<Instant>,
    #[serde(skip)]
    pub slot_shift_start: Option<Instant>,
    #[serde(default)]
    pub tts_summary: String,
    #[serde(default)]
    pub tts_body: String,
    #[serde(default)]
    pub tts_readout: Option<String>,
    #[serde(default)]
    pub tts_command: Option<String>,
    #[serde(default)]
    pub tts_pending: bool,
    #[serde(default)]
    pub tts_spoken: bool,
    #[serde(default)]
    pub tts_read_body: bool,
    #[serde(default)]
    pub daemon_pid: u32,
    #[serde(default)]
    pub closed: bool,
}

fn default_toast_text_color() -> String {
    "#f7f700".to_string()
}

#[derive(Debug, Clone)]
pub enum NotifMessage {
    DismissToast(u32),
    ActivateToast(u32),
    InvokeToastAction(u32, String),
    FinishToastActivation { id: u32, dismiss: bool },
    HistoryActivateSelected,
    FinishHistoryActivation,
    ClearHistory,
    DeleteHistory(u32),
    HistorySelectNext,
    HistorySelectPrev,
    HistoryDeleteSelected,
    HistoryScrollTo(f32),
    ToggleHistoryDrawer,
    DismissDrawer,
}

pub struct NotificationOverlay {
    /// Icon resolution cache. Keys are "app_name|desktop_entry" (empty
    /// when desktop_entry is None). First toast per app pays the
    /// stat() lookup cost; subsequent toasts reuse the cached result.
    /// Bounded at 200 entries.
    pub icon_cache: std::collections::HashMap<String, Option<String>>,
    pub active_toasts: Vec<NotificationToast>,
    /// Overflow queue. When `active_toasts` hits the visible limit (4),
    /// new toasts land here instead of evicting the oldest. `tick_toasts`
    /// pulls from the front of this queue whenever an active toast
    /// expires or is dismissed.
    pub pending_toasts: VecDeque<NotificationToast>,
    pub history: Vec<NotificationToast>,
    pub show_history: bool,
    pub stack_target_count: usize,
    pub stack_from_count: usize,
    pub stack_anim_start: Instant,
    pub selected_history_id: Option<u32>,
    pub last_saved: Instant,
    pub history_dirty: bool,
    pub state_reader: super::state::NotifStateReader,
    history_file_sig: Option<(Option<std::time::SystemTime>, u64, u64)>,
    logged_icons: Mutex<HashSet<u32>>,
}

impl Default for NotificationOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationOverlay {
    pub(super) fn should_log_icon(&self, id: u32) -> bool {
        let mut guard = match self.logged_icons.lock() {
            Ok(g) => g,
            Err(_) => return true,
        };
        guard.insert(id)
    }

    fn history_file_sig() -> Option<(Option<std::time::SystemTime>, u64, u64)> {
        use std::os::unix::fs::MetadataExt as _;
        let meta = std::fs::metadata(Self::history_path()).ok()?;
        Some((meta.modified().ok(), meta.ino(), meta.len()))
    }

    pub fn sync_history_from_disk_if_changed(&mut self) {
        if cfg!(test) {
            return;
        }
        let sig = Self::history_file_sig();
        if sig != self.history_file_sig {
            self.history = Self::load_history();
            self.history_file_sig = sig;
        }
    }

    pub fn new() -> Self {
        let history = Self::load_history();
        let history_file_sig = Self::history_file_sig();
        let selected_history_id = history.first().map(|t| t.id);
        Self {
            icon_cache: std::collections::HashMap::new(),
            active_toasts: Vec::new(),
            pending_toasts: VecDeque::new(),
            history,
            show_history: false,
            stack_target_count: 0,
            stack_from_count: 0,
            stack_anim_start: Instant::now(),
            selected_history_id,
            last_saved: Instant::now(),
            history_dirty: false,
            state_reader: super::state::NotifStateReader::new(),
            history_file_sig,
            logged_icons: Mutex::new(HashSet::new()),
        }
    }

    pub fn empty() -> Self {
        Self {
            icon_cache: std::collections::HashMap::new(),
            active_toasts: Vec::new(),
            pending_toasts: VecDeque::new(),
            history: Vec::new(),
            show_history: false,
            stack_target_count: 0,
            stack_from_count: 0,
            stack_anim_start: Instant::now(),
            selected_history_id: None,
            last_saved: Instant::now(),
            history_dirty: false,
            state_reader: super::state::NotifStateReader::new(),
            history_file_sig: None,
            logged_icons: Mutex::new(HashSet::new()),
        }
    }

    fn history_path() -> PathBuf {
        // Keep notification history out of world-readable /tmp (may contain OTPs/URLs).
        dirs::cache_dir()
            .expect("XDG_CACHE_HOME or HOME must be set; refusing to persist notification history to /tmp")
            .join("moonbeauty_notification_history.json")
    }

    /// Compute the pixel offset needed to scroll the given history
    /// item into view, using the actual computed row heights. Result is
    /// fed directly into iced's `scroll_to` AbsoluteOffset.
    ///
    /// Returns None if no item is selected. Result is measured from the
    /// top of the scrollable's content; render_history uses the same
    /// row geometry so the two always agree.
    pub fn scroll_offset_for_selected(
        &self,
        theme: &ShellTheme,
        settings: &crate::libraries::config::settings::ShellSettings,
    ) -> Option<f32> {
        let id = self.selected_history_id?;

        let (win_w, _) = settings.get_window_size("history", 960, 720);
        let hex = AngledHexGeometry::new(win_w, win_w, theme.overlay_cut, theme.overlay_angle);
        let header_inset = hex.inset_at(20.0, 16.0);
        let body_inset = header_inset + 4.0;
        let row_w = win_w - body_inset * 2.0;
        let fs = theme.font_size_overlay as f32;
        let fs_sm = fs * 0.85;
        let icon_size = settings.launcher_icon_size as f32;

        let mut y: f32 = 0.0;
        for toast in self.history.iter().rev() {
            if toast.id == id {
                // Show a bit of context above the selected row.
                let context = (history_icon_hex_size(icon_size)).max(80.0);
                return Some((y - context).max(0.0));
            }
            y += history_row_height(toast, row_w, fs, fs_sm, icon_size) + HISTORY_ROW_GAP;
        }
        Some(y)
    }

    pub fn load_history() -> Vec<NotificationToast> {
        let path = Self::history_path();
        // Pre-check file size to avoid OOM on oversized history files.
        const MAX_HISTORY_BYTES: u64 = 8 * 1024 * 1024;
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > MAX_HISTORY_BYTES => {
                eprintln!(
                    "[notif] history file {} bytes exceeds {} cap, resetting",
                    m.len(),
                    MAX_HISTORY_BYTES
                );
                let _ = std::fs::remove_file(&path);
                return Vec::new();
            }
            Ok(_) => {}
            Err(_) => return Vec::new(),
        }
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(mut items) = serde_json::from_str::<Vec<NotificationToast>>(&raw) {
                    // Backfill missing avatar_path entries on load.
                    let mut migrated = false;
                    let mut still_missing = 0;
                    for item in items.iter_mut() {
                        if item.avatar_path.is_some() {
                            continue;
                        }
                        let lower = item.app_name.to_lowercase();
                        let variants = [
                            lower.clone(),
                            lower.replace(' ', "-"),
                            lower.replace(' ', "_"),
                            lower.split_whitespace().next().unwrap_or("").to_string(),
                        ];

                        let mut resolved =
                            item.desktop_entry.as_deref().and_then(resolve_desktop_icon);

                        if resolved.is_none() {
                            for v in &variants {
                                if v.is_empty() {
                                    continue;
                                }
                                if let Some(p) = resolve_icon_name(v) {
                                    resolved = Some(p);
                                    break;
                                }
                            }
                        }

                        if let Some(p) = resolved {
                            eprintln!("[notif-migrate] id={} {} → {}", item.id, item.app_name, p);
                            item.avatar_path = Some(p);
                            migrated = true;
                        } else {
                            still_missing += 1;
                        }
                    }
                    if still_missing > 0 && std::env::var_os("MBS_DEBUG").is_some() {
                        eprintln!(
                            "[notif-migrate] {} entries still have no resolvable icon",
                            still_missing
                        );
                    }
                    if migrated {
                        if let Ok(data) = serde_json::to_string_pretty(&items) {
                            let tmp = path.with_extension("tmp");
                            if fs::write(&tmp, data).is_ok() {
                                let _ = fs::rename(tmp, &path);
                            }
                        }
                    }
                    return items;
                }
            }
        }
        Vec::new()
    }

    pub fn mark_history_dirty(&mut self) {
        self.history_dirty = true;
    }

    pub fn flush_history(&mut self) {
        if self.history_dirty && self.last_saved.elapsed().as_millis() > 2000 {
            self.history_dirty = false;
            self.last_saved = Instant::now();
            self.save_history_immediate();
        }
    }

    fn save_history_immediate(&mut self) {
        if cfg!(test) {
            return;
        }
        let path = Self::history_path();
        if let Ok(data) = serde_json::to_string_pretty(&self.history) {
            let tmp = path.with_extension("tmp");
            // Write at 0600 (history may contain OTPs and private URLs).
            if crate::libraries::process::private_write_file(&tmp, data.as_bytes()).is_ok()
                && fs::rename(tmp, &path).is_ok()
            {
                self.history_file_sig = Self::history_file_sig();
            }
        }
    }

    pub fn push_to_history(&mut self, item: NotificationToast) {
        self.sync_history_from_disk_if_changed();
        self.history.insert(0, item);
        if self.history.len() > 80 {
            self.history.truncate(80);
        }
        self.mark_history_dirty();
    }

    pub fn oldest_active_toast(&self) -> Option<NotificationToast> {
        self.active_toasts
            .iter()
            .find(|t| t.exit_started_at.is_none())
            .cloned()
    }

    pub fn push_notification(
        &mut self,
        id: u32,
        app_name: &str,
        summary: &str,
        body: &str,
        is_urgent: bool,
        is_resident: bool,
        is_test: bool,
        desktop_entry: Option<&str>,
        actions: Vec<String>,
        settings: &ShellSettings,
    ) {
        // Payloads are pre-capped at NotifServer::notify ingress.
        self.active_toasts.retain(|t| t.id != id);
        self.pending_toasts.retain(|t| t.id != id);

        let matched_profile = settings.friend_profiles.iter().find(|p| {
            summary.to_lowercase().contains(&p.summary.to_lowercase())
                || app_name.to_lowercase().contains(&p.name.to_lowercase())
        });

        if let Some(p) = matched_profile {
            if p.sound && !is_test {
                if let Some(path) = &p.sound_path {
                    Self::play_sound(path.clone(), settings.notif_volume);
                }
            }
        }

        let border_color = if is_urgent {
            "#ff0000".to_string()
        } else if let Some(p) = matched_profile {
            p.color.clone()
        } else {
            settings.palette.base05.clone()
        };

        let text_color = if is_urgent {
            settings.palette.base05.clone()
        } else if let Some(p) = matched_profile {
            p.color.clone()
        } else {
            settings.palette.base05.clone()
        };
        // Icon resolution cache: first toast per (app_name, desktop_entry)
        // pays the stat() cost on NixOS; subsequent toasts are instant.
        let avatar_path: Option<String> = if let Some(p) = matched_profile {
            // Profile matches are highly specific. Resolve them every time.
            // If we cache them under "app_name", Bob will inherit Alice's avatar!
            avatar_path_for(&p.name)
                .or_else(|| desktop_entry.and_then(resolve_desktop_icon))
                .or_else(|| resolve_icon_name(&app_name.to_lowercase().replace(' ', "-")))
        } else {
            let cache_key = format!("{}|{}", app_name, desktop_entry.unwrap_or(""));
            if let Some(cached) = self.icon_cache.get(&cache_key) {
                cached.clone()
            } else {
                let resolved = if let Some(p) = desktop_entry.and_then(resolve_desktop_icon) {
                    Some(p)
                } else {
                    let lower = app_name.to_lowercase();
                    let variants = [
                        lower.clone(),
                        lower.replace(' ', "-"),
                        lower.replace(' ', "_"),
                        lower.split_whitespace().next().unwrap_or("").to_string(),
                    ];
                    variants
                        .iter()
                        .filter(|v| !v.is_empty())
                        .find_map(|v| resolve_icon_name(v))
                };
                // Bounded cache: evict oldest when full
                if self.icon_cache.len() >= 200 {
                    if let Some(k) = self.icon_cache.keys().next().cloned() {
                        self.icon_cache.remove(&k);
                    }
                }
                self.icon_cache.insert(cache_key, resolved.clone());
                resolved
            }
        };

        let item = NotificationToast {
            id,
            app_name: app_name.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            timestamp: Local::now().format("%I:%M %p").to_string(),
            is_read: false,
            is_urgent,
            is_resident,
            border_color,
            text_color,
            avatar_path,
            desktop_entry: desktop_entry.map(|s| s.to_string()),
            actions,
            created_at: Instant::now(),
            exit_started_at: None,
            hold_reset_at: None,
            slot_shift_start: None,
            tts_summary: String::new(),
            tts_body: String::new(),
            tts_readout: None,
            tts_command: None,
            tts_pending: false,
            tts_spoken: false,
            tts_read_body: false,
            daemon_pid: std::process::id(),
            closed: false,
        };

        let tts_pending = !is_test && matched_profile.map(|p| p.tts).unwrap_or(false);
        let tts_read_body = matched_profile.map(|p| p.read_body).unwrap_or(false);
        let mut tts_readout = matched_profile.and_then(|p| p.default_readout.clone());

        // Apply custom content rules if they match the summary or body
        if let Some(p) = matched_profile {
            for rule in &p.content_rules {
                let pat = rule.pattern.to_lowercase();
                if summary.to_lowercase().contains(&pat) || body.to_lowercase().contains(&pat) {
                    tts_readout = Some(rule.readout.clone());
                    break;
                }
            }
        }
        let tts_command = settings.tts_command.clone();
        eprintln!(
            "[tts-defer] app={:?} pending={} summary={:?}",
            app_name, tts_pending, summary,
        );

        let mut item = item;
        item.tts_summary = summary.to_string();
        item.tts_body = body.to_string();
        item.tts_readout = tts_readout;
        item.tts_command = tts_command;
        item.tts_pending = tts_pending;
        item.tts_read_body = tts_read_body;

        if super::state::dnd_active() {
            eprintln!(
                "[dnd] holding notification back while paused: {:?}",
                summary
            );
            self.push_to_history(item.clone());
            let mut held = super::state::load_held_notifs();
            held.push(item);
            super::state::save_held_notifs(&held);
            eprintln!("[dnd] total notifications held: {}", held.len());
            return;
        }

        // Route to active if there's room, else to the pending queue.
        // 4 concurrent toasts is the visible limit; the 5th+ wait their
        // turn and are pulled in by `tick_toasts` when a slot frees.
        const MAX_PENDING_TOASTS: usize = 50;
        if self.active_toasts.len() < 4 {
            self.active_toasts.push(item.clone());
        } else {
            if self.pending_toasts.len() >= MAX_PENDING_TOASTS {
                self.pending_toasts.pop_front();
            }
            self.pending_toasts.push_back(item.clone());
            eprintln!(
                "[queue] overflow: {:?} queued (pending={})",
                summary,
                self.pending_toasts.len()
            );
        }

        self.push_to_history(item);
        self.save_history_immediate();
    }

    pub fn play_sound(path: String, volume: u32) {
        std::thread::spawn(move || {
            if let Ok((mut child, _tool)) =
                crate::libraries::backend::audio::spawn_audio_player(&path, volume)
            {
                let _ = child.wait();
            }
        });
    }

    pub fn speak_tts(
        app_name: &str,
        summary: &str,
        body: &str,
        custom_readout: Option<&str>,
        tts_command: Option<String>,
        volume: u32,
        read_body: bool,
    ) {
        super::tts::speak_tts(
            app_name,
            summary,
            body,
            custom_readout,
            tts_command,
            volume,
            read_body,
        );
    }

    pub fn tick_toasts(&mut self, hold_duration_sec: u64, volume: u32) {
        let hold_limit = std::time::Duration::from_secs(hold_duration_sec.max(1));
        let exit_duration = std::time::Duration::from_millis(300);
        let now = Instant::now();

        let any_exiting = self
            .active_toasts
            .iter()
            .any(|t| t.exit_started_at.is_some());
        if !any_exiting {
            let oldest_expired = self
                .active_toasts
                .iter_mut()
                .filter(|t| {
                    let hold_ref = t.hold_reset_at.unwrap_or(t.created_at);
                    !t.is_urgent && t.exit_started_at.is_none() && hold_ref.elapsed() >= hold_limit
                })
                .min_by_key(|t| t.hold_reset_at.unwrap_or(t.created_at));
            if let Some(t) = oldest_expired {
                t.exit_started_at = Some(now);
            }
        }

        let mut removed_any = false;
        self.active_toasts.retain(|t| {
            // Do not skip is_urgent here so explicitly dismissed urgent toasts
            // finish their exit animation and get removed.
            match t.exit_started_at {
                Some(start) => {
                    if start.elapsed() < exit_duration {
                        true
                    } else {
                        removed_any = true;
                        false
                    }
                }
                None => true,
            }
        });

        if removed_any {
            let now = Instant::now();
            for t in self.active_toasts.iter_mut() {
                t.hold_reset_at = Some(now);
                t.exit_started_at = None;
                t.slot_shift_start = Some(now);
            }

            // Fill freed slots from the pending queue. Reset timers so the
            // newly-promoted toast starts its full hold duration and its
            // slide-in animation from the current instant, not from when
            // it was originally queued.
            while self.active_toasts.len() < 4 {
                let Some(mut next) = self.pending_toasts.pop_front() else {
                    break;
                };
                next.created_at = Instant::now();
                next.hold_reset_at = Some(Instant::now());
                next.exit_started_at = None;
                next.slot_shift_start = Some(Instant::now());
                eprintln!(
                    "[queue] promoting queued toast to active: {:?}",
                    next.summary
                );
                self.active_toasts.push(next);
            }
        }

        let front_id = self
            .active_toasts
            .iter()
            .filter(|t| t.exit_started_at.is_none())
            .min_by_key(|t| t.created_at)
            .map(|t| t.id);
        if let Some(fid) = front_id {
            let payload = if let Some(t) = self.active_toasts.iter_mut().find(|t| t.id == fid) {
                if t.tts_pending && !t.tts_spoken {
                    t.tts_spoken = true;
                    Some((
                        t.app_name.clone(),
                        t.tts_summary.clone(),
                        t.tts_body.clone(),
                        t.tts_readout.clone(),
                        t.tts_command.clone(),
                        t.tts_read_body,
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((a, s, b, r, c, read_body)) = payload {
                crate::dbg_log!("[tts-fire] id={} app={:?}", fid, a);
                Self::speak_tts(&a, &s, &b, r.as_deref(), c, volume, read_body);
            }
        }

        let new_count = self.active_toasts.len();
        if new_count != self.stack_target_count {
            self.stack_from_count = self.stack_target_count;
            self.stack_target_count = new_count;
            self.stack_anim_start = Instant::now();
        }
    }

    pub fn dismiss_toast_ui(&mut self, id: u32) {
        let now = Instant::now();
        for t in self.active_toasts.iter_mut() {
            if t.id == id && t.exit_started_at.is_none() {
                t.exit_started_at = Some(now);
            }
        }
    }

    pub fn mark_history_closed(&mut self, id: u32) {
        self.sync_history_from_disk_if_changed();
        let pid = std::process::id();
        let mut changed = false;
        for h in self.history.iter_mut() {
            if h.id == id && h.daemon_pid == pid && !h.closed {
                h.closed = true;
                changed = true;
            }
        }
        if changed {
            self.save_history_immediate();
        }
    }

    pub fn update(&mut self, message: NotifMessage) {
        match message {
            NotifMessage::DismissToast(id) => {
                self.dismiss_toast_ui(id);
            }
            NotifMessage::ActivateToast(id)
            | NotifMessage::InvokeToastAction(id, _) => {
                self.dismiss_toast_ui(id);
            }
            NotifMessage::FinishToastActivation { id, dismiss } => {
                if dismiss {
                    self.dismiss_toast_ui(id);
                }
            }
            NotifMessage::ClearHistory => {
                self.history.clear();
                self.pending_toasts.clear();
                self.selected_history_id = None;
                self.mark_history_dirty();
                self.save_history_immediate();
                if let Ok(mut g) = self.logged_icons.lock() {
                    g.clear();
                }
            }
            NotifMessage::DeleteHistory(id) => {
                self.history.retain(|t| t.id != id);
                if self.selected_history_id == Some(id) {
                    self.selected_history_id = None;
                }
                self.mark_history_dirty();
                self.save_history_immediate();
            }
            NotifMessage::HistorySelectNext => {
                if self.history.is_empty() {
                    return;
                }
                // history is stored newest-first (insert at index 0).
                // Visual order is reversed (oldest top, newest bottom).
                // Moving "down" visually = older index in the array.
                let current_pos = self
                    .selected_history_id
                    .and_then(|id| self.history.iter().position(|t| t.id == id));
                self.selected_history_id = Some(match current_pos {
                    None => self.history[0].id,
                    Some(i) => self.history[i.saturating_sub(1)].id,
                });
            }
            NotifMessage::HistorySelectPrev => {
                if self.history.is_empty() {
                    return;
                }
                let current_pos = self
                    .selected_history_id
                    .and_then(|id| self.history.iter().position(|t| t.id == id));
                self.selected_history_id = Some(match current_pos {
                    None => self.history[self.history.len() - 1].id,
                    Some(i) => self.history[(i + 1).min(self.history.len() - 1)].id,
                });
            }
            NotifMessage::HistoryDeleteSelected => {
                if let Some(id) = self.selected_history_id {
                    let pos = self.history.iter().position(|t| t.id == id);
                    self.history.retain(|t| t.id != id);
                    self.mark_history_dirty();
                    self.save_history_immediate();

                    self.selected_history_id = if self.history.is_empty() {
                        None
                    } else {
                        let new_pos = pos.map(|p| p.min(self.history.len() - 1)).unwrap_or(0);
                        Some(self.history[new_pos].id)
                    };
                }
            }
            NotifMessage::HistoryScrollTo(_)
            | NotifMessage::HistoryActivateSelected
            | NotifMessage::FinishHistoryActivation => {}
            NotifMessage::ToggleHistoryDrawer => {
                self.show_history = !self.show_history;
            }
            NotifMessage::DismissDrawer => {
                self.show_history = false;
            }
        }
    }

    pub fn view_toasts<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &'a ShellSettings,
    ) -> Element<'a, NotifMessage> {
        super::view::render_toasts(self, theme, settings)
    }

    pub fn view_history<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, NotifMessage> {
        super::view::render_history(self, theme, settings)
    }
}

#[cfg(test)]
mod tests {
    use super::{NotificationOverlay, NotificationToast};
    use std::time::Instant;

    fn toast(id: u32) -> NotificationToast {
        NotificationToast {
            id,
            app_name: "TestApp".into(),
            summary: format!("Summary {}", id),
            body: "Body".into(),
            timestamp: "12:00 PM".into(),
            is_read: false,
            is_urgent: false,
            is_resident: false,
            border_color: "#f7f700".into(),
            text_color: "#f7f700".into(),
            avatar_path: None,
            desktop_entry: None,
            actions: Vec::new(),
            created_at: Instant::now(),
            exit_started_at: None,
            hold_reset_at: None,
            slot_shift_start: None,
            tts_summary: String::new(),
            tts_body: String::new(),
            tts_readout: None,
            tts_command: None,
            tts_pending: false,
            tts_spoken: false,
            tts_read_body: false,
            daemon_pid: 0,
            closed: false,
        }
    }

    #[test]
    fn push_to_history_truncates_at_80() {
        let mut ov = NotificationOverlay::empty();
        for id in 1..=85 {
            ov.push_to_history(toast(id));
        }
        assert_eq!(ov.history.len(), 80);
        assert_eq!(ov.history.first().unwrap().id, 85);
        assert_eq!(ov.history.last().unwrap().id, 6);
        assert!(ov.history_dirty);
    }

    #[test]
    fn oldest_active_toast_returns_front_non_exiting_toast() {
        let mut ov = NotificationOverlay::empty();
        ov.active_toasts.push(toast(10));
        ov.active_toasts.push(toast(20));
        ov.active_toasts.push(toast(30));
        assert_eq!(ov.oldest_active_toast().unwrap().id, 10);

        ov.active_toasts[0].exit_started_at = Some(Instant::now());
        assert_eq!(ov.oldest_active_toast().unwrap().id, 20);
    }
}
