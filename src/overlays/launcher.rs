use iced::widget::{button, column, container, row, scrollable, text, text_input, Row, Stack};
use iced::{Alignment, Element, Length, Padding};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use super::dictionary::{DictionaryOverlay, MeaningEntry};
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::{shell_font, TextExt};
use crate::libraries::backend::{poll_track, MprisBackend, MprisTrack, MpdBackend};
use crate::libraries::backend::music::MusicSources;

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LauncherMessage {
    SearchChanged(String),
    ExecuteApp(String),
    Dismiss,
    Refocus,
    SelectNext,
    SelectPrev,
    SelectDown,
    SelectUp,
    DictLookupReady(String, Vec<MeaningEntry>),
    MusicLoaded(Option<MprisTrack>),
    MusicPlayPause,
    MusicNext,
    MusicPrev,
    MusicSeek(f64),
    MusicSetVolume(u8),
    MusicTick,
    MusicCycle,
    MusicOpenFolder(Option<String>),
    MusicTrashConfirm,
    MusicTrashExecute(Option<String>),
    MusicSearchChanged(String),
    MusicSearchSubmit,
    MusicSearchResults(Vec<(String, String)>),
    MusicPlaySong(String),
    MusicSelectNext,
    MusicSelectPrev,
    MusicPlaySelected,
    MusicToggleSource(usize),
    MusicTriggerAction,
    EnterPressed,
}

pub const LAUNCHER_SEARCH_ID: &str = "launcher_search";
pub const MUSIC_SEARCH_ID: &str = "mpd_search";
pub const FOCUS_DUMMY_ID: &str = "focus_dummy";

pub struct AppLauncher {
    pub search_query: String,
    pub all_apps: Vec<AppEntry>,
    pub filtered_apps: Vec<AppEntry>,
    pub focus_requested: bool,
    pub selected_index: usize,
    pub(super) dict_cache: std::collections::HashMap<String, Vec<MeaningEntry>>,
    pub(super) dict_pending: Option<String>,
    pub music_state: Option<MprisTrack>,
    pub music_search_query: String,
    pub music_search_results: Vec<(String, String)>,
    pub music_selected_idx: usize,
    pub music_sources: MusicSources,
    pub trash_confirm: bool,
    pub music_focus_idx: usize,
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLauncher {
    pub fn new_empty() -> Self {
        Self {
            search_query: String::new(),
            all_apps: Vec::new(),
            filtered_apps: Vec::new(),
            focus_requested: false,
            selected_index: 0,
            dict_cache: std::collections::HashMap::new(),
            dict_pending: None,
            music_state: None,
            music_search_query: String::new(),
            music_search_results: Vec::new(),
            music_selected_idx: 0,
            music_sources: MusicSources::load(),
            trash_confirm: false,
            music_focus_idx: 0,
        }
    }

    pub fn new() -> Self {
        let mut all_apps = vec![
            AppEntry { name: "Settings & Options".into(), exec: "internal:settings".into(), icon: "⚙".into(), icon_path: None },
            AppEntry { name: "Clipboard Manager".into(), exec: "internal:clip".into(), icon: "📋".into(), icon_path: None },
            AppEntry { name: "Quick Notes".into(), exec: "internal:notes".into(), icon: "📝".into(), icon_path: None },
            AppEntry { name: "Password Store".into(), exec: "internal:pass".into(), icon: "🔑".into(), icon_path: None },
            AppEntry { name: "Power & Session".into(), exec: "internal:power".into(), icon: "⚡".into(), icon_path: None },
            AppEntry { name: "Tasks & Todos".into(), exec: "internal:todo".into(), icon: "✅".into(), icon_path: None },
            AppEntry { name: "Reminders & Alarms".into(), exec: "internal:reminders".into(), icon: "⏰".into(), icon_path: None },
            AppEntry { name: "Unicode Glyphs".into(), exec: "internal:unicode".into(), icon: "🔣".into(), icon_path: None },
            AppEntry { name: "Web Search".into(), exec: "internal:web".into(), icon: "🌐".into(), icon_path: None },
            AppEntry { name: "Screenshot".into(), exec: "moonbeauty --screenshot".into(), icon: "📷".into(), icon_path: None },
        ];
        all_apps.extend(Self::scan_desktop_files());

        Self {
            search_query: String::new(),
            filtered_apps: all_apps.clone(),
            all_apps,
            focus_requested: true,
            selected_index: 0,
            dict_cache: DictionaryOverlay::load_cache(),
            dict_pending: None,
            music_state: None,
            music_search_query: String::new(),
            music_search_results: Vec::new(),
            music_selected_idx: 0,
            music_sources: MusicSources::load(),
            trash_confirm: false,
            music_focus_idx: 0,
        }
    }

    pub fn scan_desktop_files() -> Vec<AppEntry> {
        let mut apps = Vec::new();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());

        let mut dirs: Vec<PathBuf> = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/run/current-system/sw/share/applications"),
            PathBuf::from("/etc/profiles/per-user").join(&user).join("share/applications"),
            home.join(".nix-profile/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            home.join(".local/share/flatpak/exports/share/applications"),
            PathBuf::from("/run/flatpak/exports/share/applications"),
            PathBuf::from("/var/lib/snapd/desktop/applications"),
            home.join(".local/share/applications"),
        ];

        if let Ok(xdg) = std::env::var("XDG_DATA_DIRS") {
            for d in xdg.split(':') {
                if !d.is_empty() { dirs.push(PathBuf::from(d).join("applications")); }
            }
        }

        let mut seen = std::collections::HashSet::new();
        for dir in &dirs {
            if !dir.exists() { continue; }
            let entries = match fs::read_dir(dir) { Ok(e) => e, Err(_) => continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("desktop") { continue; }
                let Ok(content) = fs::read_to_string(&path) else { continue; };
                let mut name: Option<String> = None;
                let mut exec: Option<String> = None;
                let mut icon_name: Option<String> = None;
                let mut nodisplay = false;
                let mut hidden_flag = false;
                let mut in_main_section = false;

                for line in content.lines() {
                    let line = line.trim();
                    if line.starts_with('[') {
                        in_main_section = line.eq_ignore_ascii_case("[Desktop Entry]");
                        continue;
                    }
                    if !in_main_section { continue; }
                    if let Some(v) = line.strip_prefix("Name=") {
                        if name.is_none() { name = Some(v.trim().chars().take(256).collect::<String>()); }
                    } else if let Some(v) = line.strip_prefix("Exec=") {
                        if exec.is_none() { exec = Some(Self::strip_field_codes(v.trim()).trim().chars().take(4096).collect::<String>()); }
                    } else if let Some(v) = line.strip_prefix("Icon=") {
                        if icon_name.is_none() { icon_name = Some(v.trim().chars().take(128).collect::<String>()); }
                    } else if line.eq_ignore_ascii_case("NoDisplay=true") {
                        nodisplay = true;
                    } else if line.eq_ignore_ascii_case("Hidden=true") {
                        hidden_flag = true;
                    }
                }

                if nodisplay || hidden_flag { continue; }
                if let (Some(n), Some(e)) = (name.clone(), exec.clone()) {
                    if e.is_empty() { continue; }
                    let key = format!("{}|{}", n.to_lowercase(), e.to_lowercase());
                    if !seen.contains(&key) {
                        seen.insert(key);
                        let icon_path = icon_name.as_deref().and_then(Self::resolve_icon);
                        apps.push(AppEntry { name: n, exec: e, icon: "🚀".to_string(), icon_path });
                    }
                }
            }
        }
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    fn resolve_icon(name: &str) -> Option<String> {
        crate::libraries::process::resolve_icon_path(name).map(|p| p.to_string_lossy().into_owned())
    }

    pub fn update(&mut self, message: LauncherMessage) -> iced::Task<LauncherMessage> {
        match message {
            LauncherMessage::Refocus => return iced::widget::operation::focus(LAUNCHER_SEARCH_ID),
            LauncherMessage::EnterPressed => {
                if self.search_query.trim().to_lowercase() == "music" {
                    if self.music_focus_idx == 0 {
                        return self.update(LauncherMessage::MusicPlaySelected);
                    } else {
                        return self.update(LauncherMessage::MusicTriggerAction);
                    }
                } else if let Some(selected) = self.filtered_apps.get(self.selected_index) {
                    return self.update(LauncherMessage::ExecuteApp(selected.exec.clone()));
                }
                return iced::Task::none();
            }
            LauncherMessage::MusicTriggerAction => {
                match self.music_focus_idx {
                    3 => return self.update(LauncherMessage::MusicPrev),
                    4 => return self.update(LauncherMessage::MusicPlayPause),
                    5 => return self.update(LauncherMessage::MusicNext),
                    6 => return self.update(LauncherMessage::MusicCycle),
                    7 => {
                        if let Some(m) = &self.music_state {
                            return self.update(LauncherMessage::MusicOpenFolder(m.url.clone()));
                        }
                    }
                    8 => {
                        if self.trash_confirm {
                            if let Some(m) = &self.music_state {
                                return self.update(LauncherMessage::MusicTrashExecute(m.url.clone()));
                            }
                        } else {
                            return self.update(LauncherMessage::MusicTrashConfirm);
                        }
                    }
                    9 => return self.update(LauncherMessage::MusicToggleSource(0)),
                    10 => return self.update(LauncherMessage::MusicToggleSource(1)),
                    11 => return self.update(LauncherMessage::MusicToggleSource(2)),
                    12 => return self.update(LauncherMessage::MusicPlaySelected),
                    _ => {}
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectNext => {
                if self.search_query.trim().to_lowercase() == "music" {
                    if self.music_focus_idx == 0 {
                        return iced::Task::none(); // Let text_input handle Right arrow
                    } else if self.music_focus_idx == 1 {
                        if let Some(m) = &self.music_state {
                            return self.update(LauncherMessage::MusicSeek((m.position_sec + 5.0).min(m.length_sec)));
                        }
                    } else if self.music_focus_idx == 2 {
                        if let Some(m) = &self.music_state {
                            return self.update(LauncherMessage::MusicSetVolume((m.volume_pct + 5).min(100)));
                        }
                    } else {
                        self.music_focus_idx = match self.music_focus_idx {
                            3 | 4 | 5 | 6 => (self.music_focus_idx + 1).min(6),
                            7 | 8 => (self.music_focus_idx + 1).min(8),
                            9 | 10 | 11 => (self.music_focus_idx + 1).min(11),
                            _ => self.music_focus_idx,
                        };
                        return iced::widget::operation::focus(FOCUS_DUMMY_ID);
                    }
                } else if !self.filtered_apps.is_empty() {
                    self.selected_index = (self.selected_index + 1) % self.filtered_apps.len();
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectPrev => {
                if self.search_query.trim().to_lowercase() == "music" {
                    if self.music_focus_idx == 0 {
                        return iced::Task::none(); // Let text_input handle Left arrow
                    } else if self.music_focus_idx == 1 {
                        if let Some(m) = &self.music_state {
                            return self.update(LauncherMessage::MusicSeek((m.position_sec - 5.0).max(0.0)));
                        }
                    } else if self.music_focus_idx == 2 {
                        if let Some(m) = &self.music_state {
                            return self.update(LauncherMessage::MusicSetVolume(m.volume_pct.saturating_sub(5)));
                        }
                    } else {
                        self.music_focus_idx = match self.music_focus_idx {
                            3 | 4 | 5 | 6 => (self.music_focus_idx - 1).max(3),
                            7 | 8 => (self.music_focus_idx - 1).max(7),
                            9 | 10 | 11 => (self.music_focus_idx - 1).max(9),
                            _ => self.music_focus_idx,
                        };
                        return iced::widget::operation::focus(FOCUS_DUMMY_ID);
                    }
                } else if !self.filtered_apps.is_empty() {
                    self.selected_index = (self.selected_index + self.filtered_apps.len() - 1) % self.filtered_apps.len();
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectDown => {
                if self.search_query.trim().to_lowercase() == "music" {
                    if self.music_focus_idx == 12 && self.music_selected_idx < self.music_search_results.len().saturating_sub(1) {
                        self.music_selected_idx += 1;
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("music_scroll"),
                            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: (self.music_selected_idx as f32 * 28.0 - 50.0).max(0.0) },
                        );
                    } else if self.music_focus_idx < 12 {
                        self.music_focus_idx = match self.music_focus_idx {
                            0 => 1,
                            1 => 2,
                            2 => 4, // Volume -> Play
                            3 | 4 | 5 | 6 => 7, // Controls -> Folder
                            7 | 8 => 9, // Act -> Local
                            9 | 10 | 11 => 12, // Sources -> Results
                            _ => 12,
                        };
                        if self.music_focus_idx == 0 { return iced::widget::operation::focus(MUSIC_SEARCH_ID); }
                        return iced::widget::operation::focus(FOCUS_DUMMY_ID);
                    }
                    return iced::Task::none();
                } else if !self.filtered_apps.is_empty() {
                    let math_count = self.filtered_apps.iter().filter(|a| a.exec.starts_with("math:")).count();
                    let step = if math_count > 0 && self.selected_index < math_count { 3 } else { 1 };
                    self.selected_index = (self.selected_index + step).min(self.filtered_apps.len() - 1);
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectUp => {
                if self.search_query.trim().to_lowercase() == "music" {
                    if self.music_focus_idx == 12 && self.music_selected_idx > 0 {
                        self.music_selected_idx -= 1;
                        return iced::widget::operation::scroll_to(
                            iced::widget::Id::new("music_scroll"),
                            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: (self.music_selected_idx as f32 * 28.0 - 50.0).max(0.0) },
                        );
                    } else if self.music_focus_idx > 0 {
                        self.music_focus_idx = match self.music_focus_idx {
                            0 => 0,
                            1 => 0,
                            2 => 1,
                            3 | 4 | 5 | 6 => 2, // Controls -> Volume
                            7 | 8 => 4, // Act -> Play
                            9 | 10 | 11 => 7, // Sources -> Folder
                            12 => 11, // Results -> Browser
                            _ => 0,
                        };
                        if self.music_focus_idx == 0 { return iced::widget::operation::focus(MUSIC_SEARCH_ID); }
                        return iced::widget::operation::focus(FOCUS_DUMMY_ID);
                    }
                    return iced::Task::none();
                } else if !self.filtered_apps.is_empty() {
                    let math_count = self.filtered_apps.iter().filter(|a| a.exec.starts_with("math:")).count();
                    let step = if math_count > 0 && self.selected_index < math_count { 3 } else { 1 };
                    self.selected_index = self.selected_index.saturating_sub(step);
                }
                return iced::Task::none();
            }
            LauncherMessage::DictLookupReady(word, defs) => {
                self.dict_cache.insert(word.clone(), defs);
                if self.dict_pending.as_deref() == Some(&word) { self.dict_pending = None; }
                let q = self.search_query.clone();
                return self.update(LauncherMessage::SearchChanged(q));
            }
            LauncherMessage::SearchChanged(q) => {
                self.search_query = q.clone();
                self.selected_index = 0;
                let lower = q.trim().to_lowercase();
                
                if lower == "music" {
                    self.music_focus_idx = 0;
                    let mut tasks = vec![iced::widget::operation::focus(MUSIC_SEARCH_ID)];
                    if self.music_state.is_none() {
                        tasks.push(iced::Task::perform(async { poll_track().await.0 }, LauncherMessage::MusicLoaded));
                    }
                    return iced::Task::batch(tasks);
                }

                let mut filtered: Vec<AppEntry> = if lower.is_empty() {
                    self.all_apps.clone()
                } else {
                    let mut f: Vec<AppEntry> = self.all_apps.iter()
                        .filter(|a| a.name.to_lowercase().contains(&lower) || a.exec.to_lowercase().contains(&lower))
                        .cloned().collect();
                    if let Some(alias) = Self::routing_alias(&lower) {
                        if !f.iter().any(|a| a.exec == alias.exec) { f.insert(0, alias); }
                    }
                    f
                };

                if let Some(dict_entries) = self.compute_dict_entries(&q) {
                    let mut dict_items = dict_entries;
                    dict_items.append(&mut filtered);
                    filtered = dict_items;
                    if let Some(word) = self.dict_word_to_lookup(&q) {
                        self.dict_pending = Some(word.clone());
                        self.filtered_apps = filtered;
                        let w1 = word.clone();
                        return iced::Task::perform(async move { DictionaryOverlay::lookup_word(&w1).await }, move |defs| LauncherMessage::DictLookupReady(word.clone(), defs));
                    }
                }

                if let Some(pass_entries) = Self::compute_pass_entries(&q) {
                    let mut pass_items: Vec<AppEntry> = pass_entries.into_iter().map(|(name, exec)| AppEntry { name, exec, icon: ">".to_string(), icon_path: None }).collect();
                    pass_items.append(&mut filtered);
                    filtered = pass_items;
                }

                let conversions = Self::compute_multi_conversions(&q);
                if !conversions.is_empty() {
                    let mut math_entries: Vec<AppEntry> = conversions.into_iter().map(|(label, exec)| AppEntry { name: label, exec, icon: "=".to_string(), icon_path: None }).collect();
                    math_entries.append(&mut filtered);
                    filtered = math_entries;
                }

                self.filtered_apps = filtered;
                return iced::Task::none();
            }
            LauncherMessage::MusicLoaded(state) => {
                self.music_state = state;
                return iced::Task::none();
            }
            LauncherMessage::MusicTick => {
                if self.search_query.trim().to_lowercase() == "music" {
                    return iced::Task::perform(async { poll_track().await.0 }, LauncherMessage::MusicLoaded);
                }
                return iced::Task::none();
            }
            LauncherMessage::MusicPlayPause => {
                return iced::Task::perform(async {
                    if let Ok(exe) = std::env::current_exe() { let _ = tokio::process::Command::new(exe).arg("--music-play-pause").status().await; }
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    poll_track().await.0
                }, LauncherMessage::MusicLoaded);
            }
            LauncherMessage::MusicNext => {
                return iced::Task::perform(async {
                    if let Ok(exe) = std::env::current_exe() { let _ = tokio::process::Command::new(exe).arg("--music-next").status().await; }
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    poll_track().await.0
                }, LauncherMessage::MusicLoaded);
            }
            LauncherMessage::MusicPrev => {
                return iced::Task::perform(async {
                    if let Ok(exe) = std::env::current_exe() { let _ = tokio::process::Command::new(exe).arg("--music-previous").status().await; }
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    poll_track().await.0
                }, LauncherMessage::MusicLoaded);
            }
            LauncherMessage::MusicCycle => {
                return iced::Task::perform(async {
                    if let Ok(exe) = std::env::current_exe() { let _ = tokio::process::Command::new(exe).arg("--music-cycle").status().await; }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    poll_track().await.0
                }, LauncherMessage::MusicLoaded);
            }
            LauncherMessage::MusicSeek(secs) => {
                if let Some(m) = &mut self.music_state {
                    m.position_sec = secs;
                }
                return iced::Task::perform(async move { 
                    let _ = MprisBackend::set_position(secs).await; 
                    let _ = MpdBackend::seek(secs).await;
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    MprisBackend::clear_cache();
                    MpdBackend::clear_cache();
                    poll_track().await.0
                }, LauncherMessage::MusicLoaded);
            }
            LauncherMessage::MusicSetVolume(vol) => {
                if let Some(m) = &mut self.music_state {
                    m.volume_pct = vol;
                }
                return iced::Task::perform(async move { 
                    let _ = MprisBackend::set_volume(vol).await; 
                    let _ = MpdBackend::set_volume(vol).await;
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    MprisBackend::clear_cache();
                    MpdBackend::clear_cache();
                    poll_track().await.0
                }, LauncherMessage::MusicLoaded);
            }
            LauncherMessage::MusicOpenFolder(url) => return iced::Task::perform(async move { crate::libraries::backend::MprisBackend::open_track_folder(url.as_deref()).await; }, |_| LauncherMessage::MusicTick),
            LauncherMessage::MusicTrashConfirm => self.trash_confirm = true,
            LauncherMessage::MusicTrashExecute(url) => {
                self.trash_confirm = false;
                return iced::Task::perform(async move { crate::libraries::backend::MprisBackend::trash_current_track(url.as_deref()).await; }, |_| LauncherMessage::MusicTick);
            }
            LauncherMessage::MusicToggleSource(idx) => {
                match idx {
                    0 => self.music_sources.local = !self.music_sources.local,
                    1 => self.music_sources.spotify = !self.music_sources.spotify,
                    2 => self.music_sources.browser = !self.music_sources.browser,
                    _ => {}
                }
                self.music_sources.save();
            }
            LauncherMessage::MusicSearchChanged(q) => {
                self.music_search_query = q.clone();
                self.music_selected_idx = 0;
                if q.is_empty() { self.music_search_results.clear(); return iced::Task::none(); }
                return iced::Task::perform(async move { MpdBackend::search_songs(&q).await }, LauncherMessage::MusicSearchResults);
            }
            LauncherMessage::MusicSearchSubmit => return iced::Task::done(LauncherMessage::MusicPlaySelected),
            LauncherMessage::MusicSearchResults(res) => { self.music_search_results = res; self.music_selected_idx = 0; }
            LauncherMessage::MusicSelectNext => { if !self.music_search_results.is_empty() { self.music_selected_idx = (self.music_selected_idx + 1).min(self.music_search_results.len() - 1); } }
            LauncherMessage::MusicSelectPrev => { if !self.music_search_results.is_empty() { self.music_selected_idx = self.music_selected_idx.saturating_sub(1); } }
            LauncherMessage::MusicPlaySelected => {
                if self.music_search_results.get(self.music_selected_idx).is_some() {
                    let urls: Vec<String> = self.music_search_results.iter().map(|(_, u)| u.clone()).collect();
                    let idx = self.music_selected_idx;
                    return iced::Task::perform(async move {
                        MpdBackend::play_queue(&urls, idx).await;
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        poll_track().await.0
                    }, LauncherMessage::MusicLoaded);
                }
            }
            LauncherMessage::MusicPlaySong(url) => {
                if let Some(idx) = self.music_search_results.iter().position(|(_, u)| u == &url) {
                    let urls: Vec<String> = self.music_search_results.iter().map(|(_, u)| u.clone()).collect();
                    return iced::Task::perform(async move {
                        MpdBackend::play_queue(&urls, idx).await;
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        poll_track().await.0
                    }, LauncherMessage::MusicLoaded);
                }
            }
            _ => {}
        }
        iced::Task::none()
    }

    fn routing_alias(query: &str) -> Option<AppEntry> {
        let (name, exec) = match query {
            "cfg" | "set" | "settings" | "config" | "prefs" | "preferences" | "options" | "settings & options" | "preference" | "configuration" => ("Settings & Options", "internal:settings"),
            "clip" | "clipboard" | "clipboard manager" | "clipboard history" | "paste" | "pastebin" => ("Clipboard Manager", "internal:clip"),
            "todo" | "td" | "task" | "tasks" | "tasks & todos" | "task list" => ("Tasks & Todos", "internal:todo"),
            "note" | "notes" | "quick notes" | "quicknote" => ("Quick Notes", "internal:notes"),
            "power" | "pwr" | "session" | "power & session" | "logout" | "reboot" | "shutdown" | "lock" | "lockscreen" => ("Power & Session", "internal:power"),
            "remind" | "reminder" | "reminders" | "alarm" | "alarms" | "reminders & alarms" | "timer" | "timers" => ("Reminders & Alarms", "internal:reminders"),
            "unicode" | "glyph" | "glyphs" | "symbol" | "symbols" | "emoji" | "emojis" | "unicode glyphs" => ("Unicode", "internal:unicode"),
            "email" | "mail" | "em" | "mail client" | "inbox" => ("Email", "internal:email"),
            "rng" | "dice" | "roll" | "random" | "rand" | "d20" => ("RNG / Dice", "internal:rng"),
            "web" | "search" | "web search" | "google" | "ddg" | "duckduckgo" => ("Web Search", "internal:web"),
            "amogus" | "amongus" | "among us" | "sus" => ("Among Us", "internal:amogus"),
            "shot" | "screenshot" | "capture" => ("Screenshot", "moonbeauty --screenshot"),
            _ => return None,
        };
        Some(AppEntry { name: name.into(), exec: exec.into(), icon: ">".into(), icon_path: None })
    }

    #[cfg(test)]
    pub(crate) fn parse_exec_for_test(exec: &str) -> Option<(String, Vec<String>)> { Self::parse_exec(exec) }
    #[cfg(test)]
    pub(crate) fn strip_field_codes_for_test(raw: &str) -> String { Self::strip_field_codes(raw) }

    fn strip_field_codes(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut it = raw.chars().peekable();
        while let Some(c) = it.next() {
            if c == '%' {
                match it.next() {
                    Some('%') => out.push('%'),
                    Some(_) => {} 
                    None => out.push('%'),
                }
            } else { out.push(c); }
        }
        out
    }

    fn parse_exec(exec: &str) -> Option<(String, Vec<String>)> {
        let mut tokens: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut it = exec.chars().peekable();
        while let Some(c) = it.next() {
            match (c, in_single, in_double) {
                ('\\', false, false) => {
                    if let Some(&n) = it.peek() { cur.push(n); it.next(); } else { cur.push('\\'); }
                }
                ('\'', false, false) => in_single = true,
                ('\'', true, _) => in_single = false,
                ('"', false, false) => in_double = true,
                ('"', _, true) => in_double = false,
                (' ', false, false) | ('\t', false, false) => {
                    if !cur.is_empty() { tokens.push(std::mem::take(&mut cur)); }
                }
                _ => cur.push(c),
            }
        }
        if in_single || in_double { return None; }
        if !cur.is_empty() { tokens.push(cur); }
        let mut tok_it = tokens.into_iter();
        let prog = tok_it.next()?;
        Some((prog, tok_it.collect()))
    }

    pub async fn launch(exec: &str) {
        if exec.is_empty() { return; }
        let Some((prog, args)) = Self::parse_exec(exec) else {
            eprintln!("[launcher] refusing to launch: unparseable Exec {:?}", exec);
            return;
        };
        let _ = Command::new("systemd-run").args(["--user", "--collect", "--quiet", "--slice=app.slice", "--"]).arg(&prog).args(&args).spawn();
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, LauncherMessage> {
        let (win_w, win_h) = settings.get_window_size("launcher", 840, 650);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let pad_top = 18.0_f32;
        let pad_bottom = 16.0_f32;

        let search_inward = if dy > pad_top { (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0 } else { 16.0 };
        let list_top_inset = if dy > 72.0 { (dx * (1.0 - (72.0 / dy))).max(0.0) + 16.0 } else { 16.0 };
        let list_bottom_inset = if dy > 45.0 { (dx * (1.0 - (45.0 / dy))).max(0.0) + 16.0 } else { 16.0 };
        let list_side_inset = list_top_inset.max(list_bottom_inset).max(20.0);
        let footer_inward = if dy > pad_bottom + 8.0 { (dx * (1.0 - ((pad_bottom + 8.0) / dy))).max(0.0) + 16.0 } else { 16.0 };

        let input = text_input("Search applications or settings...", &self.search_query)
            .id(LAUNCHER_SEARCH_ID)
            .font(shell_font(iced::font::Weight::Normal))
            .on_input(LauncherMessage::SearchChanged)
            .on_submit(LauncherMessage::EnterPressed)
            .padding(14)
            .size((theme.font_size_overlay) as f32)
            .width(Length::Fill)
            .style(move |_t, status| {
                let border_color = match status {
                    text_input::Status::Focused { .. } => theme.focus,
                    text_input::Status::Hovered => theme.border,
                    _ => theme.border,
                };
                let border_width = match status {
                    text_input::Status::Focused { .. } => theme.control_border_width + 0.5,
                    _ => theme.control_border_width,
                };
                text_input::Style {
                    background: iced::Background::Color(theme.surface_alt),
                    border: iced::Border { color: border_color, width: border_width, radius: 8.0.into() },
                    icon: theme.muted, placeholder: theme.muted, value: theme.text, selection: theme.focus,
                }
            });

        let input_row = container(input).width(Length::Fill).padding(Padding { top: 0.0, right: search_inward, bottom: 0.0, left: search_inward });

        let bottom_footer = container(
            text("↑↓ Select  •  Tab Focus  •  Enter Activate  •  ESC Dismiss")
                .size(11).shell().color(theme.muted).align_x(Alignment::Center),
        ).width(Length::Fill).padding(Padding { top: 0.0, right: footer_inward, bottom: 0.0, left: footer_inward });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        if self.search_query.trim().to_lowercase() == "music" {
            let music_ui: Element<'_, LauncherMessage> = if let Some(m) = &self.music_state {
                let title = if m.title.is_empty() { "No Track" } else { &m.title };
                let artist = if m.artist.is_empty() { "No Artist" } else { &m.artist };
                let playing_btn = if m.is_playing { "Pause" } else { "Play" };

                let focus_border = |idx: usize| {
                    if self.music_focus_idx == idx { theme.focus } else { theme.border }
                };
                let focus_bg = |idx: usize| {
                    if self.music_focus_idx == idx { theme.surface_alt } else { theme.background }
                };

                let seek_bar = iced::widget::slider(0.0..=m.length_sec.max(1.0), m.position_sec, LauncherMessage::MusicSeek)
                    .style(move |_t, _s| iced::widget::slider::Style {
                        rail: iced::widget::slider::Rail { backgrounds: (iced::Background::Color(theme.text), iced::Background::Color(theme.surface_alt)), width: 2.0, border: iced::Border { color: focus_border(1), width: 2.0, radius: 2.0.into() } },
                        handle: iced::widget::slider::Handle { shape: iced::widget::slider::HandleShape::Rectangle { width: 10, border_radius: 0.0.into() }, background: iced::Background::Color(theme.text), border_color: focus_border(1), border_width: 2.0 },
                    });

                let vol_bar = iced::widget::slider(0..=100, m.volume_pct, LauncherMessage::MusicSetVolume)
                    .style(move |_t, _s| iced::widget::slider::Style {
                        rail: iced::widget::slider::Rail { backgrounds: (iced::Background::Color(theme.text), iced::Background::Color(theme.surface_alt)), width: 2.0, border: iced::Border { color: focus_border(2), width: 2.0, radius: 2.0.into() } },
                        handle: iced::widget::slider::Handle { shape: iced::widget::slider::HandleShape::Rectangle { width: 10, border_radius: 0.0.into() }, background: iced::Background::Color(theme.text), border_color: focus_border(2), border_width: 2.0 },
                    });

                let btn_style = move |idx: usize| {
                    move |_t: &iced::Theme, _s: button::Status| button::Style {
                        background: Some(iced::Background::Color(focus_bg(idx))),
                        border: iced::Border { color: focus_border(idx), width: 2.0, radius: 4.0.into() },
                        text_color: theme.text, ..Default::default()
                    }
                };

                let controls = row![
                    button(text("< Prev").size(14)).on_press(LauncherMessage::MusicPrev).padding([6, 12]).width(Length::Fill).style(btn_style(3)),
                    button(text(playing_btn).size(14)).on_press(LauncherMessage::MusicPlayPause).padding([6, 12]).width(Length::Fill).style(btn_style(4)),
                    button(text("Next >").size(14)).on_press(LauncherMessage::MusicNext).padding([6, 12]).width(Length::Fill).style(btn_style(5)),
                    button(text("Cycle").size(14)).on_press(LauncherMessage::MusicCycle).padding([6, 12]).width(Length::Fill).style(btn_style(6)),
                ].spacing(10).width(Length::Fill);

                let act_row = row![
                    button(text("Folder").size(14)).on_press(LauncherMessage::MusicOpenFolder(m.url.clone())).padding([6, 12]).width(Length::Fill).style(btn_style(7)),
                    button(text(if self.trash_confirm { "Confirm?" } else { "Trash" }).size(14).color(if self.trash_confirm { theme.danger } else { theme.text }))
                        .on_press(if self.trash_confirm { LauncherMessage::MusicTrashExecute(m.url.clone()) } else { LauncherMessage::MusicTrashConfirm })
                        .padding([6, 12]).width(Length::Fill).style(btn_style(8)),
                ].spacing(10).width(Length::Fill);

                let src_btn = |label: &'static str, idx: usize| -> Element<'_, LauncherMessage> {
                    let active = match idx { 0 => self.music_sources.local, 1 => self.music_sources.spotify, 2 => self.music_sources.browser, _ => false };
                    let focused = self.music_focus_idx == 9 + idx;
                    
                    let bg = if focused {
                        theme.surface_alt // Focused background overrides active state
                    } else if active {
                        theme.text // Active background
                    } else {
                        theme.background // Inactive background
                    };
                    let fg = if focused {
                        theme.text
                    } else if active {
                        theme.background
                    } else {
                        theme.text
                    };
                    
                    button(text(label).size(12).bold().color(fg))
                        .on_press(LauncherMessage::MusicToggleSource(idx)).padding([4, 10]).width(Length::Fill)
                        .style(move |_t, _s| button::Style { 
                            background: Some(iced::Background::Color(bg)), 
                            border: iced::Border { color: if focused { theme.focus } else { theme.border }, width: if focused { 3.0 } else { 1.0 }, radius: 4.0.into() }, 
                            text_color: fg, 
                            ..Default::default() 
                        }).into()
                };

                let src_row = row![
                    text("Sources:").size(12).bold().color(theme.text),
                    iced::widget::space::horizontal(),
                    src_btn("Local", 0), src_btn("Spotify", 1), src_btn("Browser", 2)
                ].spacing(10).align_y(Alignment::Center).width(Length::Fill);

                let search_input = text_input("Search MPD...", &self.music_search_query)
                    .id(MUSIC_SEARCH_ID)
                    .on_input(LauncherMessage::MusicSearchChanged)
                    .on_submit(LauncherMessage::MusicSearchSubmit)
                    .padding(8).size(14)
                    .style(move |_t, _s| text_input::Style {
                        background: iced::Background::Color(focus_bg(0)),
                        border: iced::Border { color: focus_border(0), width: 2.0, radius: 4.0.into() },
                        icon: theme.text, placeholder: theme.text, value: theme.text, selection: theme.text,
                    });

                let mut results_col = column![].spacing(2);
                for (i, (title, url)) in self.music_search_results.iter().enumerate() {
                    let url_clone = url.clone();
                    let is_sel = i == self.music_selected_idx;
                    let bg = if is_sel { theme.text } else { theme.background };
                    let fg = if is_sel { theme.background } else { theme.text };
                    
                    let btn = button(text(title).size(12).color(fg)).on_press(LauncherMessage::MusicPlaySong(url_clone)).padding(6).width(Length::Fill)
                        .style(move |_t, _s| button::Style { 
                            background: Some(iced::Background::Color(bg)), 
                            border: iced::Border { color: if is_sel && self.music_focus_idx == 12 { theme.focus } else { theme.text }, width: if is_sel && self.music_focus_idx == 12 { 2.0 } else { 1.0 }, radius: 4.0.into() }, 
                            ..Default::default() 
                        });
                    results_col = results_col.push(btn);
                }
                
                let scroll_results = scrollable(results_col).id(iced::widget::Id::new("music_scroll")).height(Length::Fill);

                container(
                    column![
                        search_input,
                        scroll_results,
                        text(format!("{} - {}", title, artist)).size(18).bold().color(theme.text).align_x(Alignment::Center),
                        column![
                            row![text("0:00").size(12).color(theme.text), seek_bar, text(format!("{}:{:02}", (m.length_sec as u32)/60, (m.length_sec as u32)%60)).size(12).color(theme.text)].spacing(10).align_y(Alignment::Center),
                            row![text("VOL").size(12).bold().color(theme.text), vol_bar, text(format!("{}%", m.volume_pct)).size(12).color(theme.text)].spacing(10).align_y(Alignment::Center),
                        ].spacing(10),
                        controls,
                        act_row,
                        src_row,
                        text_input("", "").id(FOCUS_DUMMY_ID).width(Length::Fixed(0.0)),
                    ].spacing(14).align_x(Alignment::Center).width(Length::Fill).height(Length::Fill)
                ).padding(20).width(Length::Fill).height(Length::Fill).into()
            } else {
                container(text("Loading music...").size(20).color(theme.muted)).center_x(Length::Fill).padding(40).into()
            };

            let music_list = container(music_ui).width(Length::Fill).height(Length::Fill).padding(Padding { top: 10.0, right: list_side_inset, bottom: 0.0, left: list_side_inset });

            let card_content = container(
                column![input_row, music_list, bottom_footer].spacing(12).width(Length::Fill).height(Length::Fill),
            )
            .padding(Padding { top: pad_top, right: 0.0, bottom: pad_bottom, left: 0.0 })
            .width(Length::Fixed(win_w)).height(Length::Fixed(win_h));

            let card = Stack::new().push(card_bg.view()).push(card_content);
            return container(card).center_x(Length::Fill).center_y(Length::Fill).into();
        }

        let math_count = self.filtered_apps.iter().filter(|a| a.exec.starts_with("math:")).count();
        let math_items: Vec<&AppEntry> = self.filtered_apps.iter().filter(|a| a.exec.starts_with("math:")).take(40).collect();
        let app_items: Vec<&AppEntry> = self.filtered_apps.iter().filter(|a| !a.exec.starts_with("math:")).take(40).collect();

        let mut content_col = column![].spacing(10).width(Length::Fill);

        if !math_items.is_empty() {
            let tile_h = 76.0_f32;
            let content_w = win_w - list_side_inset * 2.0;
            let tile_w = ((content_w - 12.0) / 3.0).max(60.0);
            let mut grid_col = column![].spacing(6).width(Length::Fill);
            for (chunk_idx, chunk) in math_items.chunks(3).enumerate() {
                let mut grid_row = Row::new().spacing(6).width(Length::Fill);
                for (rel_i, item) in chunk.iter().enumerate() {
                    let global_idx = chunk_idx * 3 + rel_i;
                    let is_selected = global_idx == self.selected_index;
                    let exec_cmd = item.exec.clone();
                    let copy_val = item.exec.strip_prefix("math:").unwrap_or("").to_string();
                    let input_part = if item.name.starts_with("= ") {
                        let q = self.search_query.clone();
                        if q.chars().count() > 20 { q.chars().take(17).collect::<String>() + "..." } else { q }
                    } else { item.name.split(" = ").next().unwrap_or("").to_string() };

                    let bg = if is_selected { theme.surface_alt } else { theme.background };
                    let border_c = if is_selected { theme.text } else { theme.border };

                    let tile_bg = ShapeBox::new().shape(ShapeType::Hexagon { cut: 8 }).background(bg).border(border_c, theme.control_border_width).size(Length::Fixed(tile_w), Length::Fixed(tile_h));
                    let tile_content = container(
                        column![
                            text(copy_val).size((theme.font_size_overlay + 1) as f32).bold().color(theme.text).align_x(Alignment::Center).wrapping(iced::widget::text::Wrapping::None),
                            text(input_part).size((theme.font_size_normal.saturating_sub(2)) as f32).color(theme.muted).align_x(Alignment::Center).wrapping(iced::widget::text::Wrapping::None),
                        ].spacing(2).align_x(Alignment::Center),
                    ).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).padding([0, 20]);

                    let tile_inner = container(Stack::new().push(tile_bg.view()).push(tile_content)).width(Length::Fixed(tile_w)).height(Length::Fixed(tile_h)).clip(true);
                    let tile = button(tile_inner).on_press(LauncherMessage::ExecuteApp(exec_cmd)).padding(0).width(Length::Fixed(tile_w)).height(Length::Fixed(tile_h))
                        .style(|_t, _s| button::Style { background: None, ..Default::default() });
                    grid_row = grid_row.push(tile);
                }
                for _ in chunk.len()..3 { grid_row = grid_row.push(container("").width(Length::Fixed(tile_w)).height(Length::Fixed(tile_h))); }
                grid_col = grid_col.push(grid_row);
            }
            content_col = content_col.push(grid_col);
        }

        if !app_items.is_empty() {
            let gap = 12.0;
            let inner_pad_x = 20.0;
            let inner_pad_y = 10.0;
            let icon_size = settings.launcher_icon_size as f32;
            let icon_hex_size = icon_size * 1.40;
            let icon_hex_cut: u16 = ((icon_hex_size * 0.10) as u16).max(8);
            let row_h = icon_hex_size + inner_pad_y * 2.0;
            let row_w = win_w - list_side_inset * 2.0;
            let mut grid_col = column![].spacing(gap);

            for (idx_offset, app) in app_items.iter().enumerate() {
                let idx = math_count + idx_offset;
                let exec_cmd = app.exec.clone();
                let is_selected = idx == self.selected_index;

                let row_bg = ShapeBox::new()
                    .shape(ShapeType::Hexagon { cut: 18 })
                    .background(if is_selected { theme.surface_alt } else { theme.background })
                    .border(if is_selected { theme.text } else { theme.border }, theme.control_border_width)
                    .size(Length::Fixed(row_w), Length::Fixed(row_h));

                let icon_elem: Element<'_, LauncherMessage> = if let Some(ref path) = app.icon_path {
                    if path.ends_with(".svg") {
                        iced::widget::svg(iced::widget::svg::Handle::from_path(path.clone())).width(Length::Fixed(icon_size)).height(Length::Fixed(icon_size)).into()
                    } else {
                        iced::widget::image(iced::widget::image::Handle::from_path(path.clone())).width(Length::Fixed(icon_size)).height(Length::Fixed(icon_size)).content_fit(iced::ContentFit::Contain).into()
                    }
                } else {
                    container(text(&app.icon).size(64).color(theme.text)).width(Length::Fixed(icon_size)).height(Length::Fixed(icon_size)).center_x(Length::Fill).center_y(Length::Fill).into()
                };

                let icon_bg = ShapeBox::new().shape(ShapeType::Hexagon { cut: icon_hex_cut }).background(theme.background)
                    .border(if is_selected { theme.text } else { theme.border }, theme.control_border_width).size(Length::Fixed(icon_hex_size), Length::Fixed(icon_hex_size));
                let icon_stack = Stack::new().push(icon_bg.view()).push(container(icon_elem).width(Length::Fixed(icon_hex_size)).height(Length::Fixed(icon_hex_size)).center_x(Length::Fill).center_y(Length::Fill));

                let name_txt = text(&app.name).size(18.0).bold().color(if is_selected { theme.text_strong } else { theme.text });
                let exec_txt = text(&app.exec).size(12.0).font(iced::Font::MONOSPACE).color(theme.muted);
                let info_col = column![name_txt, exec_txt].spacing(4).align_x(Alignment::Start).width(Length::Fill);
                let row_inner = row![icon_stack, container(info_col).width(Length::Fill).padding(iced::Padding { top: 0.0, right: 20.0, bottom: 0.0, left: 20.0 })].spacing(0).align_y(iced::alignment::Vertical::Center).width(Length::Fill);
                let row_content = container(row_inner).width(Length::Fixed(row_w)).height(Length::Fixed(row_h)).padding(iced::Padding { top: inner_pad_y, right: inner_pad_x, bottom: inner_pad_y, left: inner_pad_x });
                let row_stack = Stack::new().push(row_bg.view()).push(row_content);
                let row_btn = button(row_stack).on_press(LauncherMessage::ExecuteApp(exec_cmd)).padding(0).width(Length::Fixed(row_w)).height(Length::Fixed(row_h))
                    .style(|_t, _s| button::Style { background: None, ..Default::default() });
                grid_col = grid_col.push(row_btn);
            }
            content_col = content_col.push(grid_col);
        }

        if math_items.is_empty() && app_items.is_empty() {
            content_col = content_col.push(container(text("No results").size((theme.font_size_overlay) as f32).color(theme.muted)).width(Length::Fill).center_x(Length::Fill).padding(20));
        }

        let list_container = container(scrollable(content_col).id(iced::widget::Id::new("launcher_scroll")).height(Length::Fill).width(Length::Fill))
            .width(Length::Fill).height(Length::Fill).padding(Padding { top: 0.0, right: list_side_inset, bottom: 0.0, left: list_side_inset });

        let card_content = container(
            column![input_row, list_container, bottom_footer,].spacing(12).width(Length::Fill).height(Length::Fill),
        ).padding(Padding { top: pad_top, right: 0.0, bottom: pad_bottom, left: 0.0 }).width(Length::Fixed(win_w)).height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);
        container(card).center_x(Length::Fill).center_y(Length::Fill).into()
    }
}

#[cfg(test)]
mod parse_exec_tests {
    use super::AppLauncher;
    #[test]
    fn simple_command() { let (prog, args) = AppLauncher::parse_exec_for_test("firefox").unwrap(); assert_eq!(prog, "firefox"); assert!(args.is_empty()); }
    #[test]
    fn command_with_args() { let (prog, args) = AppLauncher::parse_exec_for_test("firefox --new-window").unwrap(); assert_eq!(prog, "firefox"); assert_eq!(args, vec!["--new-window"]); }
    #[test]
    fn double_quoted_argument() { let (prog, args) = AppLauncher::parse_exec_for_test(r#"foo "bar baz""#).unwrap(); assert_eq!(prog, "foo"); assert_eq!(args, vec!["bar baz"]); }
    #[test]
    fn single_quoted_argument() { let (prog, args) = AppLauncher::parse_exec_for_test("foo 'bar baz'").unwrap(); assert_eq!(prog, "foo"); assert_eq!(args, vec!["bar baz"]); }
    #[test]
    fn shell_metachars_are_literal_args() { let (prog, args) = AppLauncher::parse_exec_for_test("foo; rm -rf ~").unwrap(); assert_eq!(prog, "foo;"); assert_eq!(args, vec!["rm", "-rf", "~"]); }
    #[test]
    fn dollar_command_substitution_literal() { let (prog, args) = AppLauncher::parse_exec_for_test("firefox $(id)").unwrap(); assert_eq!(prog, "firefox"); assert_eq!(args, vec!["$(id)"]); }
    #[test]
    fn unbalanced_quote_rejected() { assert!(AppLauncher::parse_exec_for_test(r#"foo "bar"#).is_none()); assert!(AppLauncher::parse_exec_for_test("foo 'bar").is_none()); }
    #[test]
    fn empty_returns_none() { assert!(AppLauncher::parse_exec_for_test("").is_none()); assert!(AppLauncher::parse_exec_for_test("   ").is_none()); }
    #[test]
    fn backslash_escape() { let (prog, args) = AppLauncher::parse_exec_for_test(r"foo\\bar").unwrap(); assert_eq!(prog, "foo\\bar"); assert!(args.is_empty()); }
    #[test]
    fn percent_codes_stripped() { assert_eq!(AppLauncher::strip_field_codes_for_test("firefox %u"), "firefox "); assert_eq!(AppLauncher::strip_field_codes_for_test("app %f --flag"), "app  --flag"); assert_eq!(AppLauncher::strip_field_codes_for_test("echo 100%%"), "echo 100%"); }
}
