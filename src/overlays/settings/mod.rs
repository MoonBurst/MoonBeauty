use iced::widget::{button, column, container, row, scrollable, slider, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::libraries::backend::system::{GpuInfo, SystemCollector};
use crate::libraries::config::settings::{
    ContentRule, FriendProfile, ScreenshotSaveFormat, ShellSettings,
};

/// Debug log — gated on MBS_DEBUG or MBS_SETTINGS_DEBUG so release builds
/// stay silent. Uses a persistent file handle (opened once) instead of
/// opening/writing/closing per call.
/// Minimum tooltip size per module id. Mirrors the per-row minimums that
/// used to be inlined in `render_row` — shared with the drag handler so the
/// mapping `x_norm → w` matches the slider range that existed before.
fn min_size_for(id: &str) -> (u32, u32) {
    match id {
        "alarm" => (400, 280),
        "calendar" => (380, 260),
        "music" => (380, 280),
        "unified" => (380, 260),
        "net" => (380, 260),
        _ => (280, 200),
    }
}

fn mbs_dbg(msg: &str) {
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};

    if std::env::var_os("MBS_DEBUG").is_none() && std::env::var_os("MBS_SETTINGS_DEBUG").is_none() {
        return;
    }

    static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
    let file = LOG.get_or_init(|| {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/mbs-settings-dbg.log")
            .expect("open /tmp/mbs-settings-dbg.log");
        Mutex::new(f)
    });
    if let Ok(mut f) = file.lock() {
        let _ = writeln!(*f, "{}", msg);
    }
}
use crate::libraries::config::theme::{hex_to_color, ShellTheme};
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

mod notifications;
mod colors_styles;
mod hardware;
mod bar_layout;
mod overlays_windows;
mod ipc_commands;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Notifications,
    ColorsStyles,
    Hardware,
    BarLayout,
    OverlaysWindows,
    IpcCommands,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    SelectTab(SettingsTab),

    // Sliders
    BarHeight(u32),
    GlobalFontSize(u16),
    OverlayFontSize(u16),
    TooltipFontSize(u16),
    SlantWidth(u32),
    TooltipDropMs(u32),
    TooltipExpandMs(u32),
    TooltipWidthPx(u32),
    TooltipHeightPx(u32),
    TooltipPauseMs(u32),
    TooltipOffsetLeft(i32),
    TooltipOffsetRight(i32),
    TooltipOffsetLeftReset,
    TooltipOffsetRightReset,
    BorderWidth(u32),
    ControlBorderWidth(u32),
    CapsuleSpacing(i32),
    CapsuleInternalPadding(u32),
    TrayWidth(u32),
    TrayIconSize(u32),
    LauncherIconSize(u32),
    ClipboardThumbnailSize(u32),
    TrayTimeout(u32),
    HardwarePollInterval(u64),

    // Bar Geometry
    SelectBarShape(String),
    SelectModuleShape(String),
    SelectSlantMode(String),

    // Notifications
    ToggleNotifications,
    SelectNotifCorner(String),
    SelectNotifShape(String),
    SelectNotifScreen(String),
    NotifHoldDuration(u64),
    NotifVolume(u32),
    NotifBaselineY(i32),
    NotifMarginX(i32),
    NotifStackOverlap(i32),
    NotifCardWidth(u32),
    NotifCardHeight(u32),
    NotifSlideIn(u32),
    NotifDropPause(u32),
    NotifDropDistance(u32),
    NotifSummarySize(u16),
    NotifBodySize(u16),
    NotifIconSize(u32),
    NotifExcludedChanged(String),
    CpuTempWarn(u32),
    CpuTempDanger(u32),
    CpuUsageWarn(u32),
    CpuUsageDanger(u32),
    RamWarnGib(u32),
    RamDangerGib(u32),

    // Custom Pings
    SelectProfile(usize),
    ToggleProfileSound(usize),
    TestProfileSound(usize),
    TestProfileTts(usize),
    ToggleProfileTts(usize),
    ProfileSummaryChanged(usize, String),
    ProfileColorChanged(usize, String),
    ProfileSoundPathChanged(usize, String),
    ProfileTtsTemplateChanged(usize, String),
    DeleteProfile(usize),
    AddNewProfile,
    PickAvatar(usize),
    AvatarUpdated,

    // Body Rules
    AddContentRule(usize),
    DeleteContentRule(usize, usize),
    RulePatternChanged(usize, usize, String),
    RuleReadoutChanged(usize, usize, String),

    // Colors
    ColorChanged(String, String),

    // GPU
    GpuTempWarn(u32),
    GpuTempDanger(u32),
    GpuVramWarn(u32),
    GpuVramDanger(u32),

    // Bar Layout
    ToggleCapsuleVisible(String),
    MoveCapsuleTo(String, String),
    MoveCapsuleUp(String),
    MoveCapsuleDown(String),

    // Window Sizing & Geometry
    OverlayWidth(u32),
    OverlayHeight(u32),
    WindowWidth(String, u32),
    WindowHeight(String, u32),
    AttachmentDownloadDirChanged(String),
    OverlayCut(u32),
    OverlayAngle(u32),
    ResetTooltipSize(String),
    SizeDrag {
        id: String,
        x_norm: f32,
        y_norm: f32,
    },
    SizeCommit {
        id: String,
    },
    AlarmSoundChanged(String),
    AlarmSoundDurationChanged(u32),
    AlarmSoundVolumeChanged(u32),
    TestAlarmSound,
    ImportAlarmSound,

    SelectGpuId(String),
    ScreenshotSaveFormat(ScreenshotSaveFormat),
    Gpu1TempWarn(u32),
    Gpu1TempDanger(u32),
    Gpu1VramWarn(u32),
    Gpu1VramDanger(u32),

    SaveSettings,
    TestNotification,
    CopyIpc(String),
    // ToggleEditModule removed (problem 09): handler set
    // self.editing_module, but view() never read it and nothing
    // emitted the message.
    SelectBarMonitor(String),
    Dismiss,
}

pub struct SettingsOverlay {
    pub active_tab: SettingsTab,
    pub selected_screen: String,
    pub selected_profile: usize,
    last_save: std::time::Instant,
    // editing_module removed (problem 09)
    preview_child: Option<std::process::Child>,
    last_preview: std::time::Instant,
    pub detected_gpus: Vec<GpuInfo>,
}

impl SettingsOverlay {
    pub fn new(_settings: &ShellSettings) -> Self {
        Self {
            active_tab: SettingsTab::Notifications,
            selected_screen: "Auto (Primary)".into(),
            selected_profile: 0,
            last_save: std::time::Instant::now(),
            preview_child: None,
            last_preview: std::time::Instant::now() - std::time::Duration::from_secs(60),
            detected_gpus: SystemCollector::list_gpus_sync(),
        }
    }

    pub fn empty() -> Self {
        Self {
            active_tab: SettingsTab::Notifications,
            selected_screen: "Auto (Primary)".into(),
            selected_profile: 0,
            last_save: std::time::Instant::now(),
            preview_child: None,
            last_preview: std::time::Instant::now() - std::time::Duration::from_secs(60),
            detected_gpus: Vec::new(),
        }
    }

    pub fn update(&mut self, message: SettingsMessage, settings: &mut ShellSettings) {
        mbs_dbg(&format!(
            "[dbg-update-entry] {:?}",
            std::mem::discriminant(&message)
        ));
        let should_persist = matches!(
            &message,
            SettingsMessage::Dismiss
                | SettingsMessage::SaveSettings
                | SettingsMessage::SelectTab(_)
                | SettingsMessage::ToggleNotifications
                | SettingsMessage::SelectNotifCorner(_)
                | SettingsMessage::SelectNotifShape(_)
                | SettingsMessage::SelectNotifScreen(_)
                | SettingsMessage::SelectBarShape(_)
                | SettingsMessage::SelectModuleShape(_)
                | SettingsMessage::SelectSlantMode(_)
                | SettingsMessage::AddNewProfile
                | SettingsMessage::DeleteProfile(_)
                | SettingsMessage::ToggleProfileSound(_)
                | SettingsMessage::ToggleProfileTts(_)
                | SettingsMessage::ProfileSummaryChanged(_, _)
                | SettingsMessage::ProfileColorChanged(_, _)
                | SettingsMessage::ProfileSoundPathChanged(_, _)
                | SettingsMessage::ProfileTtsTemplateChanged(_, _)
                | SettingsMessage::AddContentRule(_)
                | SettingsMessage::DeleteContentRule(_, _)
                | SettingsMessage::RulePatternChanged(_, _, _)
                | SettingsMessage::RuleReadoutChanged(_, _, _)
                | SettingsMessage::ToggleCapsuleVisible(_)
                | SettingsMessage::MoveCapsuleTo(_, _)
                | SettingsMessage::MoveCapsuleUp(_)
                | SettingsMessage::MoveCapsuleDown(_)
                | SettingsMessage::ColorChanged(_, _)
                | SettingsMessage::SelectGpuId(_)
                | SettingsMessage::TrayIconSize(_)
                | SettingsMessage::LauncherIconSize(_)
                | SettingsMessage::ClipboardThumbnailSize(_)
                | SettingsMessage::ScreenshotSaveFormat(_)
                | SettingsMessage::SizeCommit { .. }
        );
        match message {
            SettingsMessage::TrayIconSize(v) => settings.tray_icon_size = v.clamp(12, 48),
            SettingsMessage::LauncherIconSize(v) => settings.launcher_icon_size = v.clamp(48, 160),
            SettingsMessage::ClipboardThumbnailSize(v) => {
                settings.clipboard_thumbnail_size = v.clamp(32, 160)
            }
            SettingsMessage::SelectGpuId(id) => {
                settings.gpu_selected_id = id;
            }
            SettingsMessage::ScreenshotSaveFormat(fmt) => {
                settings.screenshot_save_format = fmt;
            }
            SettingsMessage::Gpu1TempWarn(v) => settings.gpu1.temp_warn = v,
            SettingsMessage::Gpu1TempDanger(v) => settings.gpu1.temp_danger = v,
            SettingsMessage::Gpu1VramWarn(v) => settings.gpu1.vram_warn = v,
            SettingsMessage::Gpu1VramDanger(v) => settings.gpu1.vram_danger = v,

            SettingsMessage::SelectBarMonitor(s) => {
                settings.bar_monitor = s.clone();
                let _ = settings.save();
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--restart-bar").spawn();
                }
            }
            SettingsMessage::SizeDrag { id, x_norm, y_norm } => {
                let (min_w, min_h) = min_size_for(&id);
                let max_w: u32 = 1200;
                let max_h: u32 = 1000;
                // Map x_norm linearly into [0, max], then clamp to [min, max].
                // This makes the fill's aspect ratio exactly w/h when the
                // track has the aspect ratio of (max_w, max_h).
                let new_w = ((x_norm * max_w as f32).round() as u32).clamp(min_w, max_w);
                let new_h = ((y_norm * max_h as f32).round() as u32).clamp(min_h, max_h);
                settings.tooltip_sizes.insert(id, (new_w, new_h));
            }
            SettingsMessage::SizeCommit { id } => {
                // Release → save happens via should_persist below.
                // Spawn the live preview here, one time, at the final size.
                let (w, h) = settings.tooltip_size(&id);
                self.spawn_tooltip_preview(&id, w, h, settings);
            }
            SettingsMessage::ResetTooltipSize(id) => {
                self.kill_preview();
                settings.tooltip_sizes.remove(&id);
            }
            SettingsMessage::SelectTab(t) => {
                self.kill_preview();
                self.active_tab = t;
                if t == SettingsTab::Hardware {
                    self.detected_gpus = SystemCollector::list_gpus_sync();
                }
            }

            SettingsMessage::BarHeight(v) => settings.bar_height = v.clamp(28, 90),
            SettingsMessage::GlobalFontSize(v) => settings.global_font_size = v.clamp(8, 32),
            SettingsMessage::OverlayFontSize(v) => settings.overlay_font_size = v.clamp(10, 36),
            SettingsMessage::TooltipFontSize(v) => settings.tooltip_font_size = v.clamp(8, 28),
            SettingsMessage::SlantWidth(v) => settings.slant_width = (v as f32).clamp(0.0, 40.0),
            SettingsMessage::TooltipDropMs(v) => settings.tooltip_drop_ms = v.clamp(50, 1000),
            SettingsMessage::TooltipExpandMs(v) => settings.tooltip_expand_ms = v.clamp(50, 1000),
            SettingsMessage::TooltipWidthPx(v) => settings.tooltip_width_px = v.clamp(200, 1000),
            SettingsMessage::TooltipHeightPx(v) => settings.tooltip_height_px = v.clamp(100, 1000),
            SettingsMessage::TooltipPauseMs(v) => settings.tooltip_pause_ms = v.clamp(0, 1000),
            SettingsMessage::TooltipOffsetLeft(v) => {
                settings.tooltip_offset_left = Some(v.clamp(-100, 200))
            }
            SettingsMessage::TooltipOffsetRight(v) => {
                settings.tooltip_offset_right = Some(v.clamp(-100, 200))
            }
            SettingsMessage::TooltipOffsetLeftReset => settings.tooltip_offset_left = None,
            SettingsMessage::TooltipOffsetRightReset => settings.tooltip_offset_right = None,
            SettingsMessage::BorderWidth(v) => {
                settings.global_border_width = (v as f32).clamp(1.0, 12.0)
            }
            SettingsMessage::ControlBorderWidth(v) => {
                settings.control_border_width = (v as f32).clamp(1.0, 8.0)
            }
            SettingsMessage::CapsuleSpacing(v) => settings.capsule_spacing = v.clamp(-50, 40),
            SettingsMessage::CapsuleInternalPadding(v) => {
                settings.capsule_internal_padding = v.clamp(0, 60)
            }
            SettingsMessage::TrayWidth(v) => settings.tray_width = v.clamp(50, 400),
            SettingsMessage::TrayTimeout(v) => settings.tray_timeout_sec = v.clamp(0, 30),
            SettingsMessage::HardwarePollInterval(v) => {
                settings.hardware_poll_interval_ms = v.clamp(500, 10000)
            }

            SettingsMessage::SelectBarShape(s) => settings.bar_shape = s,
            SettingsMessage::SelectModuleShape(s) => settings.module_shape = s,
            SettingsMessage::SelectSlantMode(m) => settings.slant_mode = m,

            SettingsMessage::ToggleNotifications => {
                settings.notifications_enabled = !settings.notifications_enabled
            }
            SettingsMessage::SelectNotifCorner(c) => settings.notif_corner = c,
            SettingsMessage::SelectNotifShape(s) => settings.notif_shape = s,
            SettingsMessage::SelectNotifScreen(s) => {
                self.selected_screen = s.clone();
                settings.notif_screen_name = s;
            }
            SettingsMessage::NotifHoldDuration(v) => {
                settings.notif_hold_duration_sec = v.clamp(1, 30)
            }
            SettingsMessage::NotifVolume(v) => settings.notif_volume = v.clamp(0, 100),
            SettingsMessage::NotifBaselineY(v) => settings.notif_baseline_y = v.clamp(20, 1200),
            SettingsMessage::NotifMarginX(v) => settings.notif_margin_x = v.clamp(0, 200),
            SettingsMessage::NotifStackOverlap(v) => {
                settings.notif_stack_overlap = v.clamp(-100, 100)
            }
            SettingsMessage::NotifCardWidth(v) => settings.notif_card_width = v.clamp(250, 800),
            SettingsMessage::NotifCardHeight(v) => settings.notif_card_height = v.clamp(80, 300),
            SettingsMessage::NotifSlideIn(v) => settings.notif_slide_in_ms = v.clamp(20, 1000),
            SettingsMessage::NotifDropPause(v) => settings.notif_drop_pause_ms = v.clamp(0, 500),
            SettingsMessage::NotifDropDistance(v) => {
                settings.notif_drop_distance_px = v.clamp(10, 500)
            }
            SettingsMessage::NotifSummarySize(v) => settings.notif_summary_size = v.clamp(10, 36),
            SettingsMessage::NotifBodySize(v) => settings.notif_body_size = v.clamp(8, 32),
            SettingsMessage::NotifIconSize(v) => settings.notif_icon_size = v.clamp(24, 96),
            SettingsMessage::NotifExcludedChanged(v) => settings.notif_excluded = v,
            SettingsMessage::CpuTempWarn(v) => settings.cpu_temp_warn = v,
            SettingsMessage::CpuTempDanger(v) => settings.cpu_temp_danger = v,
            SettingsMessage::CpuUsageWarn(v) => settings.cpu_usage_warn = v,
            SettingsMessage::CpuUsageDanger(v) => settings.cpu_usage_danger = v,
            SettingsMessage::RamWarnGib(v) => settings.ram_warn_gib = v,
            SettingsMessage::RamDangerGib(v) => settings.ram_danger_gib = v,

            SettingsMessage::SelectProfile(idx) => self.selected_profile = idx,
            SettingsMessage::TestProfileSound(_) | SettingsMessage::TestProfileTts(_) => {}
            SettingsMessage::ToggleProfileSound(idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.sound = !p.sound;
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::ToggleProfileTts(idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.tts = !p.tts;
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::ProfileSummaryChanged(idx, val) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.summary = val.clone();
                    p.name = val.to_lowercase().replace(' ', "_");
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::ProfileColorChanged(idx, col) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.color = col;
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::ProfileSoundPathChanged(idx, path) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.sound_path = if path.trim().is_empty() {
                        None
                    } else {
                        Some(path.trim().to_string())
                    };
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::ProfileTtsTemplateChanged(idx, tpl) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.default_readout = if tpl.trim().is_empty() {
                        None
                    } else {
                        Some(tpl.trim().to_string())
                    };
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::DeleteProfile(idx) => {
                if idx < settings.friend_profiles.len() {
                    settings.friend_profiles.remove(idx);
                    if self.selected_profile >= settings.friend_profiles.len() {
                        self.selected_profile = settings.friend_profiles.len().saturating_sub(1);
                    }
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::PickAvatar(_) | SettingsMessage::AvatarUpdated => {
                // Handled in app/update/settings.rs to allow async file dialogs.
            }
            SettingsMessage::AddNewProfile => {
                settings.friend_profiles.push(FriendProfile {
                    name: "new_ping".into(),
                    summary: "New Ping".into(),
                    color: "#38FEDC".into(),
                    sound: true,
                    sound_path: None,
                    tts: true,
                    default_readout: Some("Message from {name}: {body}".into()),
                    content_rules: vec![crate::libraries::config::settings::ContentRule {
                        pattern: "logged in".into(),
                        readout: "{name} has logged in".into(),
                        sound_path: None,
                    }],
                });
                self.selected_profile = settings.friend_profiles.len().saturating_sub(1);
                let _ = crate::libraries::config::settings::save_friend_profiles(
                    &settings.friend_profiles,
                );
            }

            SettingsMessage::AddContentRule(p_idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    p.content_rules.push(ContentRule {
                        pattern: "logged in".into(),
                        readout: "{name} has logged in".into(),
                        sound_path: None,
                    });
                    let _ = crate::libraries::config::settings::save_friend_profiles(
                        &settings.friend_profiles,
                    );
                }
            }
            SettingsMessage::DeleteContentRule(p_idx, r_idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    if r_idx < p.content_rules.len() {
                        p.content_rules.remove(r_idx);
                        let _ = crate::libraries::config::settings::save_friend_profiles(
                            &settings.friend_profiles,
                        );
                    }
                }
            }
            SettingsMessage::RulePatternChanged(p_idx, r_idx, val) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    if let Some(r) = p.content_rules.get_mut(r_idx) {
                        r.pattern = val;
                        let _ = crate::libraries::config::settings::save_friend_profiles(
                            &settings.friend_profiles,
                        );
                    }
                }
            }
            SettingsMessage::RuleReadoutChanged(p_idx, r_idx, val) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    if let Some(r) = p.content_rules.get_mut(r_idx) {
                        r.readout = val;
                        let _ = crate::libraries::config::settings::save_friend_profiles(
                            &settings.friend_profiles,
                        );
                    }
                }
            }

            SettingsMessage::ColorChanged(slot, hex) => match slot.as_str() {
                "base00" => settings.palette.base00 = hex,
                "base01" => settings.palette.base01 = hex,
                "base02" => settings.palette.base02 = hex,
                "base03" => settings.palette.base03 = hex,
                "base05" => settings.palette.base05 = hex,
                "base08" => settings.palette.base08 = hex,
                "base09" => settings.palette.base09 = hex,
                "base0c" => settings.palette.base0c = hex,
                "base0d" => settings.palette.base0d = hex,
                _ => {}
            },

            SettingsMessage::GpuTempWarn(v) => settings.gpu0.temp_warn = v,
            SettingsMessage::GpuTempDanger(v) => settings.gpu0.temp_danger = v,
            SettingsMessage::GpuVramWarn(v) => settings.gpu0.vram_warn = v,
            SettingsMessage::GpuVramDanger(v) => settings.gpu0.vram_danger = v,

            SettingsMessage::ToggleCapsuleVisible(id) => {
                let cur = settings.visible_capsules.get(&id).copied().unwrap_or(true);
                settings.visible_capsules.insert(id, !cur);
            }
            SettingsMessage::MoveCapsuleTo(id, target) => {
                settings.bar_left.retain(|x| x != &id);
                settings.bar_center.retain(|x| x != &id);
                settings.bar_right.retain(|x| x != &id);
                match target.as_str() {
                    "left" => {
                        settings.bar_left.push(id.clone());
                        settings.visible_capsules.insert(id, true);
                    }
                    "center" => {
                        settings.bar_center.push(id.clone());
                        settings.visible_capsules.insert(id, true);
                    }
                    "right" => {
                        settings.bar_right.push(id.clone());
                        settings.visible_capsules.insert(id, true);
                    }
                    "hidden" => {
                        settings.visible_capsules.insert(id, false);
                    }
                    _ => {}
                }
            }
            SettingsMessage::MoveCapsuleUp(id) => {
                let lists = [
                    &mut settings.bar_left,
                    &mut settings.bar_center,
                    &mut settings.bar_right,
                ];
                for list in lists {
                    if let Some(pos) = list.iter().position(|x| x == &id) {
                        if pos > 0 {
                            list.swap(pos, pos - 1);
                        }
                        break;
                    }
                }
            }
            SettingsMessage::MoveCapsuleDown(id) => {
                let lists = [
                    &mut settings.bar_left,
                    &mut settings.bar_center,
                    &mut settings.bar_right,
                ];
                for list in lists {
                    if let Some(pos) = list.iter().position(|x| x == &id) {
                        if pos < list.len() - 1 {
                            list.swap(pos, pos + 1);
                        }
                        break;
                    }
                }
            }

            SettingsMessage::OverlayWidth(v) => settings.overlay_width = v.clamp(400, 3000),
            SettingsMessage::OverlayHeight(v) => settings.overlay_height = v.clamp(250, 2000),
            SettingsMessage::WindowWidth(id, w) => {
                let entry = settings.standalone_windows.entry(id).or_insert(
                    crate::libraries::config::settings::WindowDim {
                        width: w,
                        height: 650,
                        field_height: None,
                        dismiss_on_blur: Some(true),
                        escape_closes: Some(true),
                    },
                );
                entry.width = w;
            }
            SettingsMessage::WindowHeight(id, h) => {
                let entry = settings.standalone_windows.entry(id).or_insert(
                    crate::libraries::config::settings::WindowDim {
                        width: 840,
                        height: h,
                        field_height: None,
                        dismiss_on_blur: Some(true),
                        escape_closes: Some(true),
                    },
                );
                entry.height = h;
            }
            SettingsMessage::AttachmentDownloadDirChanged(v) => {
                settings.attachment_download_dir = v
            }
            SettingsMessage::OverlayCut(v) => {
                settings.overlay_cut = v.clamp(0, 200);
            }
            SettingsMessage::OverlayAngle(v) => {
                settings.overlay_angle = v.clamp(5, 85);
            }
            SettingsMessage::TestNotification => {
                settings.friend_profiles =
                    crate::libraries::config::settings::load_friend_profiles();
            }
            // SettingsMessage::ToggleEditModule removed (problem 09)
            SettingsMessage::AlarmSoundChanged(v) => {
                settings.alarm_sound = if v.trim().is_empty() { None } else { Some(v) };
            }
            SettingsMessage::AlarmSoundDurationChanged(v) => {
                settings.alarm_sound_duration_sec = v.clamp(1, 60);
            }
            SettingsMessage::TestAlarmSound => {
                if let Some(sound) = &settings.alarm_sound {
                    crate::libraries::backend::play_alarm_sound(
                        sound,
                        settings.alarm_sound_volume,
                        settings.alarm_sound_duration_sec,
                    );
                }
            }
            SettingsMessage::SaveSettings => {}
            SettingsMessage::AlarmSoundVolumeChanged(v) => {
                settings.alarm_sound_volume = v.clamp(0, 100);
            }

            SettingsMessage::ImportAlarmSound => {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg("zenity --file-selection --title='Select alarm sound' 2>/dev/null || kdialog --getopenfilename . 2>/dev/null || yad --file 2>/dev/null")
                    .output();
                if let Ok(out) = output {
                    let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path_str.is_empty() {
                        let src = std::path::Path::new(&path_str);
                        if let Some(filename) = src.file_name().and_then(|f| f.to_str()) {
                            // Copy to XDG_DATA/moonbeauty/sounds — stable
                            // across cwd changes. The old `resources/` copy
                            // worked only when the bar was launched from the
                            // project root; systemd / nix develop / standalone
                            // children all fail that assumption.
                            let dest_dir = dirs::data_dir()
                                .map(|d| d.join("moonbeauty").join("sounds"))
                                .unwrap_or_else(|| {
                                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                        .join("resources")
                                });
                            let _ = std::fs::create_dir_all(&dest_dir);
                            let dest = dest_dir.join(filename);
                            match std::fs::copy(src, &dest) {
                                Ok(_) => {
                                    settings.alarm_sound = Some(filename.to_string());
                                    let _ = settings.save();
                                    eprintln!(
                                        "[alarm-sound] imported {} -> {}",
                                        filename,
                                        dest.display()
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "[alarm-sound] import failed (copy to {}): {e}",
                                        dest.display()
                                    );
                                }
                            }
                        }
                    }
                }
            }
            // CopyIpc is handled in app/update.rs (returns
            // iced::clipboard::write early). This arm only exists for
            // enum exhaustiveness when SettingsOverlay::update is
            // called directly.
            SettingsMessage::CopyIpc(_) => {}
            SettingsMessage::Dismiss => {
                self.kill_preview();
            }
        }

        // Commit to disk: immediately on explicit save-worthy events, or
        // 300ms after the last slider tick (rate-limited so a full slider
        // drag writes at most ~3×/sec instead of 60×/sec).
        let rate_limited = self.last_save.elapsed() >= std::time::Duration::from_millis(300);
        if should_persist || rate_limited {
            let _ = settings.save();
            self.last_save = std::time::Instant::now();
        }
    }

    fn spawn_tooltip_preview(&mut self, id: &str, w: u32, h: u32, settings: &ShellSettings) {
        // Debounce: skip if we spawned within last 80ms
        if self.last_preview.elapsed() < std::time::Duration::from_millis(80) {
            return;
        }
        self.last_preview = std::time::Instant::now();

        // Kill existing preview
        if let Some(mut c) = self.preview_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }

        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(_) => return,
        };

        let (arg, extra_env): (&str, Option<(&str, String)>) = match id {
            "weather" => ("--weather-tooltip", None),
            "tray" => ("--tray-popup", None),
            "alarm" => ("--alarm-tooltip", None),
            "cpu" | "gpu" | "ram" => ("--sys-tooltip", Some(("MBS_SYS_TARGET", id.to_string()))),
            "calendar" | "music" | "unified" | "net" | "custom" => {
                ("--info-tooltip", Some(("MBS_INFO_MODE", id.to_string())))
            }
            _ => return,
        };

        let id_owned = id.to_string();
        let (sl, sr, grow_right) = if settings.bar_left.contains(&id_owned) {
            ("L", "L", "1")
        } else if settings.bar_right.contains(&id_owned) {
            ("R", "R", "0")
        } else {
            ("L", "R", "1")
        };

        // Approximate bar position — settings doesn't know the exact capsule cx.
        let preview_cx = 400i32;
        let preview_sw = 1920i32;
        let preview_cap_w = 150i32;

        let mut cmd = std::process::Command::new(exe);
        cmd.arg(arg);
        cmd.env("MBS_TOOLTIP_W", w.to_string())
            .env("MBS_TOOLTIP_H", h.to_string())
            .env("MBS_TOOLTIP_DROP_MS", settings.tooltip_drop_ms.to_string())
            .env(
                "MBS_TOOLTIP_EXPAND_MS",
                settings.tooltip_expand_ms.to_string(),
            )
            .env(
                "MBS_TOOLTIP_PAUSE_MS",
                settings.tooltip_pause_ms.to_string(),
            )
            .env("MBS_CAPSULE_CX", preview_cx.to_string())
            .env("MBS_CAPSULE_W", preview_cap_w.to_string())
            .env("MBS_SCREEN_W", preview_sw.to_string())
            .env("MBS_SLANT_L", sl)
            .env("MBS_SLANT_R", sr)
            .env("MBS_GROW_RIGHT", grow_right)
            .env("MBS_OUTPUT", &settings.bar_monitor);

        if let Some((k, v)) = extra_env {
            cmd.env(k, v);
        }

        match cmd.spawn() {
            Ok(child) => {
                self.preview_child = Some(child);
            }
            Err(e) => eprintln!("[preview] spawn failed: {e}"),
        }
    }

    fn kill_preview(&mut self) {
        if let Some(mut c) = self.preview_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    pub fn view<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let (card_w, card_h) = settings.get_window_size("settings", 1260, 840);

        let cut_f = settings.overlay_cut as f32;
        let angle_deg = settings.overlay_angle as f32;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(card_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(card_h * 0.45).max(0.0);

        // Safe inward clearance so the header and tabs never clip over the top corners
        let header_inset = if dy > 22.0 {
            (dx * (1.0 - (22.0 / dy))).max(0.0) + 18.0
        } else {
            18.0
        };
        let tab_inset = if dy > 55.0 {
            (dx * (1.0 - (55.0 / dy))).max(0.0) + 14.0
        } else {
            14.0
        };
        let footer_inset = if dy > 24.0 {
            (dx * (1.0 - (24.0 / dy))).max(0.0) + 18.0
        } else {
            18.0
        };

        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };

        let tabs = [
            (SettingsTab::Notifications, "Notifications"),
            (SettingsTab::ColorsStyles, "Colors & Styles"),
            (SettingsTab::Hardware, "Hardware"),
            (SettingsTab::BarLayout, "Bar Layout & Slants"),
            (SettingsTab::OverlaysWindows, "Overlays & Windows"),
            (SettingsTab::IpcCommands, "Commands & Keybinds"),
        ];

        let mut tab_row = row![].spacing(6).align_y(Alignment::Center);
        for (tab, label) in tabs {
            let is_sel = self.active_tab == tab;
            let col = if is_sel { theme.background } else { theme.text };
            let bg_col = if is_sel {
                theme.text
            } else {
                Color::TRANSPARENT
            };

            let btn_item = button(
                text(label)
                    .size((scale_sz(0.82)) as f32)
                    .bold()
                    .color(col)
                    .align_x(Alignment::Center),
            )
            .on_press(SettingsMessage::SelectTab(tab))
            .padding([8, 14])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(bg_col)),
                border: iced::Border {
                    color: theme.text,
                    width: 1.5,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });

            tab_row = tab_row.push(btn_item);
        }

        let body_content: Element<'a, SettingsMessage> = match self.active_tab {
            SettingsTab::Notifications => self.view_notifications(settings, theme),

            SettingsTab::ColorsStyles => self.view_colors_styles(settings, theme),

            SettingsTab::Hardware => self.view_hardware(settings, theme),

            SettingsTab::BarLayout => self.view_bar_layout(settings, theme),

            SettingsTab::OverlaysWindows => self.view_overlays_windows(settings, theme),
            SettingsTab::IpcCommands => self.view_ipc_commands(settings, theme),
        };

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(card_w), Length::Fixed(card_h));

        let header_row = container(
            row![
                text("⚙ SYSTEM SETTINGS & PREFERENCES")
                    .size((scale_sz(1.15)) as f32)
                    .bold()
                    .color(theme.text),
                row![].width(Length::Fill),
                button(
                    text("✕")
                        .size((scale_sz(0.85)) as f32)
                        .bold()
                        .color(theme.danger)
                )
                .on_press(SettingsMessage::Dismiss)
                .padding([4, 10])
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                }),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let tab_container = container(tab_row).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: tab_inset,
            bottom: 0.0,
            left: tab_inset,
        });

        let footer_container = container(
            text("ESC or [✕] to dismiss  •  Changes update and save live in real time")
                .size((scale_sz(0.65)) as f32)
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: footer_inset,
            bottom: 0.0,
            left: footer_inset,
        });

        let card_content = container(
            column![
                header_row,
                tab_container,
                container(body_content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(8),
                footer_container,
            ]
            .spacing(12)
            .padding(20),
        )
        .width(Length::Fixed(card_w))
        .height(Length::Fixed(card_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }






}
