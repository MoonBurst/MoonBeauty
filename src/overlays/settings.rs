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

    fn view_notifications<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let (_, card_h) = settings.get_window_size("settings", 1260, 840);
        let mut pings_col = column![
            text("CUSTOM PINGS")
                .size((scale_sz(0.95)) as f32)
                .bold()
                .color(theme.success),
            row![
                text("ALL:")
                    .size((scale_sz(0.68)) as f32)
                    .bold()
                    .color(theme.text),
                text("Sound")
                    .size((scale_sz(0.68)) as f32)
                    .bold()
                    .color(theme.danger),
                text("TTS")
                    .size((scale_sz(0.68)) as f32)
                    .bold()
                    .color(theme.danger),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .width(Length::Fixed(270.0));

        let mut pings_list = column![].spacing(6);
        for (idx, p) in settings.friend_profiles.iter().enumerate() {
            let p_color = hex_to_color(&p.color);
            let p_name = p.summary.clone();
            let is_sound = p.sound;
            let is_tts = p.tts;
            let is_selected = idx == self.selected_profile;

            let sound_btn = button(text("Sound").size((scale_sz(0.6)) as f32).bold().color(
                if is_sound {
                    Color::BLACK
                } else {
                    Color::from_rgb8(255, 60, 60)
                },
            ))
            .on_press(SettingsMessage::ToggleProfileSound(idx))
            .padding([2, 6])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_sound {
                    Color::from_rgb8(0, 224, 0)
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_sound {
                        Color::from_rgb8(0, 224, 0)
                    } else {
                        Color::from_rgb8(60, 60, 80)
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            });

            let tts_btn = button(text("TTS").size((scale_sz(0.6)) as f32).bold().color(
                if is_tts {
                    Color::BLACK
                } else {
                    Color::from_rgb8(255, 60, 60)
                },
            ))
            .on_press(SettingsMessage::ToggleProfileTts(idx))
            .padding([2, 6])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_tts {
                    Color::from_rgb8(0, 224, 0)
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_tts {
                        Color::from_rgb8(0, 224, 0)
                    } else {
                        Color::from_rgb8(60, 60, 80)
                    },
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            });

            let swatch = container(text(""))
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(p_color)),
                    border: iced::Border {
                        color: Color::WHITE,
                        width: 1.0,
                        radius: 7.0.into(),
                    },
                    ..Default::default()
                });

            let capsule_content = row![
                swatch,
                text(p_name)
                    .size((scale_sz(0.75)) as f32)
                    .bold()
                    .color(if is_selected {
                        theme.text
                    } else {
                        Color::from_rgb8(200, 200, 200)
                    })
                    .width(Length::Fill),
                sound_btn,
                tts_btn,
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            let card_btn = button(capsule_content)
                .on_press(SettingsMessage::SelectProfile(idx))
                .padding([4, 10])
                .width(Length::Fill)
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(if is_selected {
                        Color::from_rgb8(34, 38, 50)
                    } else {
                        Color::from_rgb8(18, 20, 26)
                    })),
                    border: iced::Border {
                        color: p_color,
                        width: if is_selected { 2.0 } else { 1.5 },
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                });

            pings_list = pings_list.push(card_btn);
        }

        let add_btn = button(
            text("+ Add User / Word")
                .size((scale_sz(0.72)) as f32)
                .bold()
                .color(theme.success)
                .align_x(Alignment::Center),
        )
        .on_press(SettingsMessage::AddNewProfile)
        .padding([6, 12])
        .width(Length::Fill)
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
            border: iced::Border {
                color: theme.success,
                width: 1.5,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        pings_col = pings_col.push(scrollable(pings_list).height(Length::Fixed(560.0)));
        pings_col = pings_col.push(add_btn);

        let active_pill = button(
            text(if settings.notifications_enabled {
                "ON"
            } else {
                "OFF"
            })
            .size((scale_sz(0.7)) as f32)
            .bold()
            .color(Color::BLACK),
        )
        .on_press(SettingsMessage::ToggleNotifications)
        .padding([4, 14])
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(if settings.notifications_enabled {
                theme.success
            } else {
                theme.danger
            })),
            border: iced::Border {
                color: if settings.notifications_enabled {
                    theme.success
                } else {
                    theme.danger
                },
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });

        let test_btn = button(
            text("Test Notification")
                .size((scale_sz(0.72)) as f32)
                .bold()
                .color(theme.text),
        )
        .on_press(SettingsMessage::TestNotification)
        .padding([6, 16])
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::from_rgb8(20, 22, 28))),
            border: iced::Border {
                color: theme.text,
                width: 1.5,
                radius: 6.0.into(),
            },
            ..Default::default()
        });

        let screens = ["Auto (Primary)", "HDMI-A-2", "DP-2", "DP-1"];
        let mut scr_row = row![text("Screen:")
            .size((scale_sz(0.78)) as f32)
            .bold()
            .color(theme.text)]
        .spacing(6)
        .align_y(Alignment::Center);
        for s in screens {
            let is_s = self.selected_screen == s;
            let s_str = s.to_string();
            let scr_btn = button(text(s).size((scale_sz(0.65)) as f32).bold().color(if is_s {
                Color::BLACK
            } else {
                theme.text
            }))
            .on_press(SettingsMessage::SelectNotifScreen(s_str))
            .padding([3, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_s {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_s { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            scr_row = scr_row.push(scr_btn);
        }

        let corners = [
            ("top-right", "Top-R"),
            ("top-left", "Top-L"),
            ("bottom-right", "Bot-R"),
            ("bottom-left", "Bot-L"),
        ];
        let mut corner_row = row![text("Corner:")
            .size((scale_sz(0.78)) as f32)
            .bold()
            .color(theme.text)]
        .spacing(6)
        .align_y(Alignment::Center);
        for (c_id, c_lbl) in corners {
            let is_c = settings.notif_corner == c_id;
            let c_str = c_id.to_string();
            let c_btn = button(
                text(c_lbl)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(if is_c { Color::BLACK } else { theme.text }),
            )
            .on_press(SettingsMessage::SelectNotifCorner(c_str))
            .padding([3, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_c {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_c { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            corner_row = corner_row.push(c_btn);
        }

        let shapes = [
            ("round", "Round"),
            ("hex", "Hex"),
            ("slant", "Slant"),
            ("flat", "Flat"),
        ];
        let mut shape_row = row![text("Shape:")
            .size((scale_sz(0.78)) as f32)
            .bold()
            .color(theme.text)]
        .spacing(6)
        .align_y(Alignment::Center);
        for (s_id, s_lbl) in shapes {
            let is_sh = settings.notif_shape == s_id;
            let sh_str = s_id.to_string();
            let sh_btn = button(
                text(s_lbl)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(if is_sh { Color::BLACK } else { theme.text }),
            )
            .on_press(SettingsMessage::SelectNotifShape(sh_str))
            .padding([3, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_sh {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_sh { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            shape_row = shape_row.push(sh_btn);
        }

        let col_left_sliders = column![
            column![
                row![
                    text("Hold Duration")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}s", settings.notif_hold_duration_sec))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(1..=20, settings.notif_hold_duration_sec as u32, |v| {
                    SettingsMessage::NotifHoldDuration(v as u64)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Baseline Y Offset")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_baseline_y))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    20..=800,
                    settings.notif_baseline_y,
                    SettingsMessage::NotifBaselineY
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Card Stack Overlap")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_stack_overlap))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    -100..=100,
                    settings.notif_stack_overlap,
                    SettingsMessage::NotifStackOverlap
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Notification Card Height")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_card_height))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    90..=240,
                    settings.notif_card_height,
                    SettingsMessage::NotifCardHeight
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Drop Pause")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}ms", settings.notif_drop_pause_ms))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    0..=400,
                    settings.notif_drop_pause_ms,
                    SettingsMessage::NotifDropPause
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Summary Font Size")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_summary_size))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(10..=36, settings.notif_summary_size as u32, |v| {
                    SettingsMessage::NotifSummarySize(v as u16)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .width(Length::FillPortion(1));

        let col_right_sliders = column![
            column![
                row![
                    text("Sound Volume")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}%", settings.notif_volume))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(0..=100, settings.notif_volume, SettingsMessage::NotifVolume)
                    .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Margin X Offset")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_margin_x))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    0..=150,
                    settings.notif_margin_x,
                    SettingsMessage::NotifMarginX
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Notification Card Width")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_card_width))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    250..=650,
                    settings.notif_card_width,
                    SettingsMessage::NotifCardWidth
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Slide-In Time")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}ms", settings.notif_slide_in_ms))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    50..=600,
                    settings.notif_slide_in_ms,
                    SettingsMessage::NotifSlideIn
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Drop Travel Distance")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_drop_distance_px))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    10..=400,
                    settings.notif_drop_distance_px,
                    SettingsMessage::NotifDropDistance
                )
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Body Font Size")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_body_size))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(8..=32, settings.notif_body_size as u32, |v| {
                    SettingsMessage::NotifBodySize(v as u16)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
            column![
                row![
                    text("Notification Icon Size")
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.notif_icon_size))
                        .size((scale_sz(0.7)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    24..=96,
                    settings.notif_icon_size,
                    SettingsMessage::NotifIconSize
                )
                .width(Length::Fill),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .width(Length::FillPortion(1));

        let profile_inspector: Element<'a, SettingsMessage> = if let Some(p) =
            settings.friend_profiles.get(self.selected_profile)
        {
            let sel_idx = self.selected_profile;
            let p_summary = p.summary.clone();
            let p_sound = p.sound_path.clone().unwrap_or_default();
            let p_tts = p
                .default_readout
                .clone()
                .unwrap_or_else(|| "Message from {name}".into());

            let colors = [
                "#38FEDC", "#758593", "#8be9fd", "#ED54BA", "#F5F557", "#132ED1", "#C51111",
                "#EF7D0D", "#117F2D", "#D6E0F0", "#FFFFFF",
            ];
            let mut swatch_row = row![text("Tag Color:")
                .size((scale_sz(0.72)) as f32)
                .bold()
                .color(theme.text)]
            .spacing(6)
            .align_y(Alignment::Center);
            for c_hex in colors {
                let c_val = hex_to_color(c_hex);
                let hex_str = c_hex.to_string();
                let is_active = p.color.eq_ignore_ascii_case(c_hex);
                let s_btn = button(
                    container(text(""))
                        .width(Length::Fixed(16.0))
                        .height(Length::Fixed(16.0))
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(c_val)),
                            border: iced::Border {
                                color: if is_active {
                                    Color::WHITE
                                } else {
                                    Color::TRANSPARENT
                                },
                                width: if is_active { 2.0 } else { 0.0 },
                                radius: 8.0.into(),
                            },
                            ..Default::default()
                        }),
                )
                .on_press(SettingsMessage::ProfileColorChanged(sel_idx, hex_str))
                .padding(2)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });

                swatch_row = swatch_row.push(s_btn);
            }

            let mut rules_col = column![row![
                text(format!("BODY RULES ({})", p.content_rules.len()))
                    .size((scale_sz(0.72)) as f32)
                    .bold()
                    .color(theme.success),
                row![].width(Length::Fill),
                button(
                    text("+ Add Body Rule")
                        .size((scale_sz(0.68)) as f32)
                        .bold()
                        .color(theme.success)
                )
                .on_press(SettingsMessage::AddContentRule(sel_idx))
                .padding([3, 8])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgb8(20, 25, 20))),
                    border: iced::Border {
                        color: theme.success,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                }),
            ]
            .align_y(Alignment::Center),]
            .spacing(6);

            for (r_idx, r) in p.content_rules.iter().enumerate() {
                let pat = r.pattern.clone();
                let rd = r.readout.clone();
                rules_col = rules_col.push(
                    row![
                        text("When contains:")
                            .size((scale_sz(0.65)) as f32)
                            .color(theme.muted),
                        text_input("pattern", &pat)
                            .on_input(move |v| SettingsMessage::RulePatternChanged(
                                sel_idx, r_idx, v
                            ))
                            .padding(4)
                            .size((scale_sz(0.68)) as f32)
                            .width(Length::Fixed(140.0)),
                        text("TTS:")
                            .size((scale_sz(0.65)) as f32)
                            .color(theme.muted),
                        text_input("readout", &rd)
                            .on_input(move |v| SettingsMessage::RuleReadoutChanged(
                                sel_idx, r_idx, v
                            ))
                            .padding(4)
                            .size((scale_sz(0.68)) as f32)
                            .width(Length::Fill),
                        button(text("✕").size(10).color(theme.danger))
                            .on_press(SettingsMessage::DeleteContentRule(sel_idx, r_idx))
                            .padding([3, 6])
                            .style(|_t, _s| button::Style {
                                background: None,
                                ..Default::default()
                            }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }

            column![
                row![
                    text(format!("CONFIGURING PING: {}", p.summary.to_uppercase()))
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.success),
                    row![].width(Length::Fill),
                    button(
                        text("Delete Rule")
                            .size((scale_sz(0.68)) as f32)
                            .bold()
                            .color(theme.danger)
                    )
                    .on_press(SettingsMessage::DeleteProfile(sel_idx))
                    .padding([3, 10])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb8(30, 15, 15))),
                        border: iced::Border {
                            color: theme.danger,
                            width: 1.0,
                            radius: 4.0.into()
                        },
                        ..Default::default()
                    }),
                ]
                .align_y(Alignment::Center),
                row![
                    text("Match Tag:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    text_input("Name/Tag", &p_summary)
                        .on_input(move |v| SettingsMessage::ProfileSummaryChanged(sel_idx, v))
                        .padding(5)
                        .size((scale_sz(0.72)) as f32),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Base Sound:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    text_input("Sound Path (opt)", &p_sound)
                        .on_input(move |v| SettingsMessage::ProfileSoundPathChanged(sel_idx, v))
                        .padding(5)
                        .size((scale_sz(0.72)) as f32),
                    button(text("▶").size((scale_sz(0.7)) as f32).color(theme.success))
                        .on_press(SettingsMessage::TestProfileSound(sel_idx))
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
                            border: iced::Border {
                                color: theme.success,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                    button(text("✕").size(10).color(theme.danger))
                        .on_press(SettingsMessage::ProfileSoundPathChanged(sel_idx, "".into()))
                        .padding([4, 8])
                        .style(|_t, _s| button::Style {
                            background: None,
                            ..Default::default()
                        }),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Default TTS:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    text_input("Message from {name}", &p_tts)
                        .on_input(move |v| SettingsMessage::ProfileTtsTemplateChanged(sel_idx, v))
                        .padding(5)
                        .size((scale_sz(0.72)) as f32),
                    button(text("▶").size((scale_sz(0.7)) as f32).color(theme.success))
                        .on_press(SettingsMessage::TestProfileTts(sel_idx))
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
                            border: iced::Border {
                                color: theme.success,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                row![
                    text("Avatar Image:")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                        .width(Length::Fixed(100.0)),
                    button(
                        container(
                            text("📁 Pick Image & Auto-Crop")
                                .size((scale_sz(0.72)) as f32)
                                .color(Color::BLACK)
                        )
                        .padding([5, 10])
                    )
                    .on_press(SettingsMessage::PickAvatar(sel_idx))
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.text)),
                        border: iced::Border {
                            color: theme.text,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                swatch_row,
                rules_col,
            ]
            .spacing(8)
            .into()
        } else {
            column![].into()
        };

        let controls_col = column![
            row![
                text("NOTIFICATION CONTROLS")
                    .size((scale_sz(0.95)) as f32)
                    .bold()
                    .color(theme.success),
                row![].width(Length::Fill),
                test_btn,
            ]
            .align_y(Alignment::Center),
            row![
                text("Notifications Active")
                    .size((scale_sz(0.8)) as f32)
                    .bold()
                    .color(theme.text),
                row![].width(Length::Fill),
                active_pill,
            ]
            .align_y(Alignment::Center),
            scr_row,
            corner_row,
            shape_row,
            row![col_left_sliders, col_right_sliders].spacing(16),
            column![
                text("Excluded Apps (comma separated):")
                    .size((scale_sz(0.72)) as f32)
                    .bold()
                    .color(theme.text),
                text_input("greenclip, copyq", &settings.notif_excluded)
                    .on_input(SettingsMessage::NotifExcludedChanged)
                    .padding(6)
                    .size((scale_sz(0.75)) as f32),
            ]
            .spacing(4),
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(theme.border)),
                    ..Default::default()
                }),
            profile_inspector,
        ]
        .spacing(10)
        .width(Length::Fill);

        let right_scrollable = scrollable(container(controls_col).padding(Padding {
            top: 0.0,
            right: 28.0,
            bottom: 12.0,
            left: 0.0,
        }))
        .height(Length::Fixed(card_h - 180.0))
        .width(Length::Fill);

        row![pings_col, right_scrollable].spacing(24).into()
    }

    fn view_colors_styles<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let slots = [
            (
                "base00",
                "Primary Background (base00)",
                &settings.palette.base00,
            ),
            (
                "base01",
                "Surface Alternate (base01)",
                &settings.palette.base01,
            ),
            (
                "base02",
                "Selection / Row Highlight (base02)",
                &settings.palette.base02,
            ),
            (
                "base03",
                "Outer Bar & Card Border (base03)",
                &settings.palette.base03,
            ),
            (
                "base05",
                "Primary Accent / Text (base05)",
                &settings.palette.base05,
            ),
            (
                "base08",
                "Danger / Alert Red (base08)",
                &settings.palette.base08,
            ),
            (
                "base09",
                "Warning / Orange (base09)",
                &settings.palette.base09,
            ),
            (
                "base0c",
                "Success / Green (base0C)",
                &settings.palette.base0c,
            ),
            (
                "base0d",
                "Focus / Blue Accent (base0D)",
                &settings.palette.base0d,
            ),
        ];

        let mut col = column![
            text("🎨 PALETTE & SYSTEM ACCENT COLORS")
                .size((scale_sz(0.95)) as f32)
                .bold()
                .color(theme.success),
            text("Click on color swatches to apply presets, or type any valid hex color:")
                .size((scale_sz(0.72)) as f32)
                .color(theme.muted),
        ]
        .spacing(10);

        let preset_colors = [
            "#0f0f0f", "#003399", "#f7f700", "#ff5555", "#04f100", "#675ddb", "#fabd2f", "#38fedc",
            "#ffffff",
        ];

        for (slot, name, current_hex) in slots {
            let slot_clone = slot.to_string();
            let swatch_col = hex_to_color(current_hex);

            let swatch_box = container(text(""))
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Background::Color(swatch_col)),
                    border: iced::Border {
                        color: Color::WHITE,
                        width: 1.5,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });

            let input = text_input("#ffffff", current_hex)
                .on_input(move |h| SettingsMessage::ColorChanged(slot_clone.clone(), h))
                .padding(5)
                .size((scale_sz(0.72)) as f32)
                .width(Length::Fixed(110.0));

            let mut swatches_row = row![swatch_box, input]
                .spacing(8)
                .align_y(Alignment::Center);

            for p_hex in preset_colors {
                let p_val = hex_to_color(p_hex);
                let p_str = p_hex.to_string();
                let s_clone = slot.to_string();
                let p_btn = button(
                    container(text(""))
                        .width(Length::Fixed(14.0))
                        .height(Length::Fixed(14.0))
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(p_val)),
                            border: iced::Border {
                                radius: 3.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                )
                .on_press(SettingsMessage::ColorChanged(s_clone, p_str))
                .padding(2)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                });
                swatches_row = swatches_row.push(p_btn);
            }

            col = col.push(
                row![
                    swatches_row,
                    text(name)
                        .size((scale_sz(0.75)) as f32)
                        .bold()
                        .color(theme.text),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            );
        }

        col = col.push(
            column![
                row![
                    text("Window & Card Border Width")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.global_border_width as u32))
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    1..=10,
                    settings.global_border_width as u32,
                    SettingsMessage::BorderWidth
                )
                .width(Length::Fill),
            ]
            .spacing(3),
        );
        col = col.push(
            column![
                row![
                    text("Control / Button Border Width")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}px", settings.control_border_width as u32))
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(
                    1..=8,
                    settings.control_border_width as u32,
                    SettingsMessage::ControlBorderWidth
                )
                .width(Length::Fill),
            ]
            .spacing(3),
        );

        col.into()
    }

    fn view_hardware<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let mut gpu_col = column![text("🎮 GPU ALERT THRESHOLDS & MONITORING")
            .size((scale_sz(0.95)) as f32)
            .bold()
            .color(theme.success),]
        .spacing(12)
        .width(Length::Fill);

        gpu_col = gpu_col.push(
            text("CPU & RAM WARNINGS")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.success),
        );

        gpu_col = gpu_col.push(
            row![
                column![
                    row![
                        text("CPU Temp Warning (°C)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}°C", settings.cpu_temp_warn))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        40..=95,
                        settings.cpu_temp_warn,
                        SettingsMessage::CpuTempWarn
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("CPU Temp Danger (°C)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}°C", settings.cpu_temp_danger))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        50..=105,
                        settings.cpu_temp_danger,
                        SettingsMessage::CpuTempDanger
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
        );

        gpu_col = gpu_col.push(
            row![
                column![
                    row![
                        text("CPU Usage Warning (%)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}%", settings.cpu_usage_warn))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        10..=99,
                        settings.cpu_usage_warn,
                        SettingsMessage::CpuUsageWarn
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("CPU Usage Danger (%)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}%", settings.cpu_usage_danger))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        20..=100,
                        settings.cpu_usage_danger,
                        SettingsMessage::CpuUsageDanger
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
        );

        gpu_col = gpu_col.push(
            row![
                column![
                    row![
                        text("RAM Remaining Warning (GiB)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{} GiB", settings.ram_warn_gib))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(1..=32, settings.ram_warn_gib, SettingsMessage::RamWarnGib)
                        .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("RAM Remaining Danger (GiB)")
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{} GiB", settings.ram_danger_gib))
                            .size((scale_sz(0.72)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        1..=32,
                        settings.ram_danger_gib,
                        SettingsMessage::RamDangerGib
                    )
                    .width(Length::Fill),
                ]
                .spacing(2)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
        );

        gpu_col = gpu_col.push(
            text("Active GPU (shown in top bar)")
                .size((scale_sz(0.8)) as f32)
                .bold()
                .color(theme.muted),
        );

        let mut selector_row: iced::widget::Row<'a, SettingsMessage> = iced::widget::Row::new()
            .spacing(8)
            .align_y(Alignment::Center);

        if self.detected_gpus.is_empty() {
            selector_row = selector_row.push(
                text("No GPUs detected")
                    .size((scale_sz(0.72)) as f32)
                    .color(theme.muted),
            );
        } else {
            for gpu in &self.detected_gpus {
                let is_sel = settings.gpu_selected_id == gpu.card_id;
                let card_id = gpu.card_id.clone();
                let label = gpu.name.clone();
                let btn = button(
                    text(label)
                        .size((scale_sz(0.68)) as f32)
                        .bold()
                        .color(if is_sel { Color::BLACK } else { theme.text }),
                )
                .on_press(SettingsMessage::SelectGpuId(card_id))
                .padding([4, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(if is_sel {
                        theme.text
                    } else {
                        Color::TRANSPARENT
                    })),
                    border: iced::Border {
                        color: if is_sel { theme.text } else { theme.border },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });
                selector_row = selector_row.push(btn);
            }
        }
        gpu_col = gpu_col.push(selector_row);

        for idx in 0usize..2 {
            let th = if idx == 0 {
                &settings.gpu0
            } else {
                &settings.gpu1
            };
            let gpu_name = self.detected_gpus.get(idx).map(|g| g.name.clone());
            let section_label = match gpu_name {
                Some(n) => format!("── GPU {}: {} ──", idx, n),
                None => format!("── GPU {} (not detected) ──", idx),
            };

            gpu_col = gpu_col.push(
                text(section_label)
                    .size((scale_sz(0.85)) as f32)
                    .bold()
                    .color(theme.success),
            );

            let tw_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuTempWarn
            } else {
                SettingsMessage::Gpu1TempWarn
            };
            let td_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuTempDanger
            } else {
                SettingsMessage::Gpu1TempDanger
            };
            let vw_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuVramWarn
            } else {
                SettingsMessage::Gpu1VramWarn
            };
            let vd_fn: fn(u32) -> SettingsMessage = if idx == 0 {
                SettingsMessage::GpuVramDanger
            } else {
                SettingsMessage::Gpu1VramDanger
            };

            gpu_col = gpu_col.push(
                row![
                    column![
                        row![
                            text("Temp Warning (°C)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{}°C", th.temp_warn))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(40..=95, th.temp_warn, tw_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                    column![
                        row![
                            text("Temp Danger (°C)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{}°C", th.temp_danger))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(50..=105, th.temp_danger, td_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                ]
                .spacing(16),
            );

            gpu_col = gpu_col.push(
                row![
                    column![
                        row![
                            text("Free VRAM Warning (GiB)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{} GiB", th.vram_warn))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(1..=16, th.vram_warn, vw_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                    column![
                        row![
                            text("Free VRAM Danger (GiB)")
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text),
                            row![].width(Length::Fill),
                            text(format!("{} GiB", th.vram_danger))
                                .size((scale_sz(0.72)) as f32)
                                .bold()
                                .color(theme.text)
                        ],
                        slider(0..=8, th.vram_danger, vd_fn).width(Length::Fill),
                    ]
                    .spacing(2)
                    .width(Length::FillPortion(1)),
                ]
                .spacing(16),
            );
        }

        gpu_col = gpu_col.push(
            column![
                row![
                    text("Hardware Polling Interval (ms)")
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(format!("{}ms", settings.hardware_poll_interval_ms))
                        .size((scale_sz(0.72)) as f32)
                        .bold()
                        .color(theme.text)
                ],
                slider(500..=5000, settings.hardware_poll_interval_ms as u32, |v| {
                    SettingsMessage::HardwarePollInterval(v as u64)
                })
                .width(Length::Fill),
            ]
            .spacing(2),
        );

        let body: Element<'a, SettingsMessage> = container(
            scrollable(container(gpu_col).padding(iced::Padding {
                top: 0.0,
                right: 24.0,
                bottom: 0.0,
                left: 0.0,
            }))
            .height(Length::Fill)
            .width(Length::Fill),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: (settings.overlay_cut as f32) + 16.0,
            bottom: (settings.overlay_cut as f32) + 16.0,
            left: (settings.overlay_cut as f32) + 16.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into();
        body
    }

    fn view_bar_layout<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let all_capsules = [
            "calendar", "music", "alarm", "weather", "unified", "notify", "audio", "clock", "mic",
            "custom", "net", "cpu", "gpu", "ram", "battery", "tray",
        ];

        let mk_toggle = move |label: &'static str, active: bool, msg: SettingsMessage| {
            let bg = if active {
                theme.success
            } else {
                theme.background
            };
            let fg = if active { Color::BLACK } else { theme.text };
            let border = if active { theme.success } else { theme.text };
            button(text(label).size((scale_sz(0.7)) as f32).bold().color(fg))
                .on_press(msg)
                .padding([4, 10])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(bg)),
                    border: iced::Border {
                        color: border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
        };

        // ── Monitor picker ────────────────────────────────
        let monitors = ["All", "DP-1", "DP-2", "HDMI-A-1", "HDMI-A-2"];
        let mut mon_row: iced::widget::Row<'a, SettingsMessage> = iced::widget::Row::new()
            .spacing(8)
            .align_y(Alignment::Center)
            .push(
                text("Bar Monitor:")
                    .size((scale_sz(0.85)) as f32)
                    .bold()
                    .color(theme.text),
            );
        for m in monitors {
            let is_sel = settings.bar_monitor == m;
            let m_str = m.to_string();
            let btn = button(
                text(m)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(if is_sel { Color::BLACK } else { theme.text }),
            )
            .on_press(SettingsMessage::SelectBarMonitor(m_str))
            .padding([3, 10])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(if is_sel {
                    theme.text
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    color: if is_sel { theme.text } else { theme.border },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            mon_row = mon_row.push(btn);
        }

        let bg_bar_row: Element<'a, SettingsMessage> = row![
            text("Background Bar Shape:")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.text),
            mk_toggle(
                "SLANT",
                settings.bar_shape == "slant",
                SettingsMessage::SelectBarShape("slant".into())
            ),
            mk_toggle(
                "HEXAGON",
                settings.bar_shape == "hexagon",
                SettingsMessage::SelectBarShape("hexagon".into())
            ),
            mk_toggle(
                "ROUNDED",
                settings.bar_shape == "rounded",
                SettingsMessage::SelectBarShape("rounded".into())
            ),
            mk_toggle(
                "FLAT",
                settings.bar_shape == "flat",
                SettingsMessage::SelectBarShape("flat".into())
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

        let module_slants_row: Element<'a, SettingsMessage> = row![
            text("Module Slants:")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.text),
            mk_toggle(
                "LEFT",
                settings.slant_mode == "left",
                SettingsMessage::SelectSlantMode("left".into())
            ),
            mk_toggle(
                "CENTER",
                settings.slant_mode == "center"
                    || settings.slant_mode == "symmetric"
                    || settings.slant_mode == "sym",
                SettingsMessage::SelectSlantMode("center".into())
            ),
            mk_toggle(
                "RIGHT",
                settings.slant_mode == "right",
                SettingsMessage::SelectSlantMode("right".into())
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

        let module_shape_row: Element<'a, SettingsMessage> = row![
            text("Module Shape:")
                .size((scale_sz(0.85)) as f32)
                .bold()
                .color(theme.text),
            mk_toggle(
                "SLANT",
                settings.module_shape == "slant",
                SettingsMessage::SelectModuleShape("slant".into())
            ),
            mk_toggle(
                "HEXAGON",
                settings.module_shape == "hexagon",
                SettingsMessage::SelectModuleShape("hexagon".into())
            ),
            mk_toggle(
                "ROUNDED",
                settings.module_shape == "rounded",
                SettingsMessage::SelectModuleShape("rounded".into())
            ),
            mk_toggle(
                "FLAT",
                settings.module_shape == "flat",
                SettingsMessage::SelectModuleShape("flat".into())
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

        let top_sliders: Element<'a, SettingsMessage> = column![
            row![
                column![
                    row![
                        text("Top Bar Height")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.bar_height))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(28..=90, settings.bar_height, SettingsMessage::BarHeight)
                        .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Global Font Size")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.global_font_size))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        8..=32,
                        settings.global_font_size,
                        SettingsMessage::GlobalFontSize
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
            ]
            .spacing(20)
            .width(Length::Fill),
            row![
                column![
                    row![
                        text("Capsule Spacing")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.capsule_spacing))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        -50..=40,
                        settings.capsule_spacing,
                        SettingsMessage::CapsuleSpacing
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Capsule Slant Width")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.slant_width as u32))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        0..=40,
                        settings.slant_width as u32,
                        SettingsMessage::SlantWidth
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Capsule Internal Padding")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.capsule_internal_padding))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        0..=60,
                        settings.capsule_internal_padding,
                        SettingsMessage::CapsuleInternalPadding
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
            ]
            .spacing(20)
            .width(Length::Fill),
        ]
        .spacing(12)
        .width(Length::Fill)
        .into();

        // Finding 77: per-slot offset columns. Slider shows the
        // effective value (auto or override); label marks "(auto)"
        // when the field is None; reset button clears the override.
        let offset_left_col: Element<'a, SettingsMessage> = {
            let is_auto = settings.tooltip_offset_left.is_none();
            let eff = settings.effective_tooltip_offset_left();
            column![
                row![
                    text("Tooltip Left Offset")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(if is_auto {
                        format!("{}px (auto)", eff)
                    } else {
                        format!("{}px", eff)
                    })
                    .size((scale_sz(0.8)) as f32)
                    .bold()
                    .color(theme.text),
                    button(text(if is_auto { " " } else { "↺" }).size(11))
                        .on_press_maybe(if is_auto {
                            None
                        } else {
                            Some(SettingsMessage::TooltipOffsetLeftReset)
                        })
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: if is_auto {
                                None
                            } else {
                                Some(iced::Background::Color(theme.surface_alt))
                            },
                            border: if is_auto {
                                iced::Border::default()
                            } else {
                                iced::Border {
                                    color: theme.warning,
                                    width: 1.0,
                                    radius: 3.0.into(),
                                }
                            },
                            text_color: if is_auto { theme.muted } else { theme.warning },
                            ..Default::default()
                        }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                slider(-50..=100, eff, SettingsMessage::TooltipOffsetLeft).width(Length::Fill),
            ]
            .spacing(3)
            .width(Length::FillPortion(1))
            .into()
        };
        let offset_right_col: Element<'a, SettingsMessage> = {
            let is_auto = settings.tooltip_offset_right.is_none();
            let eff = settings.effective_tooltip_offset_right();
            column![
                row![
                    text("Tooltip Right Offset")
                        .size((scale_sz(0.8)) as f32)
                        .bold()
                        .color(theme.text),
                    row![].width(Length::Fill),
                    text(if is_auto {
                        format!("{}px (auto)", eff)
                    } else {
                        format!("{}px", eff)
                    })
                    .size((scale_sz(0.8)) as f32)
                    .bold()
                    .color(theme.text),
                    button(text(if is_auto { " " } else { "↺" }).size(11))
                        .on_press_maybe(if is_auto {
                            None
                        } else {
                            Some(SettingsMessage::TooltipOffsetRightReset)
                        })
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: if is_auto {
                                None
                            } else {
                                Some(iced::Background::Color(theme.surface_alt))
                            },
                            border: if is_auto {
                                iced::Border::default()
                            } else {
                                iced::Border {
                                    color: theme.warning,
                                    width: 1.0,
                                    radius: 3.0.into(),
                                }
                            },
                            text_color: if is_auto { theme.muted } else { theme.warning },
                            ..Default::default()
                        }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                slider(-50..=100, eff, SettingsMessage::TooltipOffsetRight).width(Length::Fill),
            ]
            .spacing(3)
            .width(Length::FillPortion(1))
            .into()
        };
        let anim_sliders: Element<'a, SettingsMessage> = column![
            row![
                column![
                    row![
                        text("Tooltip Drop Speed")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}ms", settings.tooltip_drop_ms))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        5..=100,
                        (settings.tooltip_drop_ms / 10).clamp(5, 100),
                        |v: u32| SettingsMessage::TooltipDropMs(v * 10)
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Tooltip Expand Speed")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}ms", settings.tooltip_expand_ms))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        5..=100,
                        (settings.tooltip_expand_ms / 10).clamp(5, 100),
                        |v: u32| SettingsMessage::TooltipExpandMs(v * 10)
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
            ]
            .spacing(20)
            .width(Length::Fill),
            row![
                column![
                    row![
                        text("Tooltip Pause Time")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}ms", settings.tooltip_pause_ms))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        0..=100,
                        (settings.tooltip_pause_ms / 10).min(100),
                        |v: u32| SettingsMessage::TooltipPauseMs(v * 10)
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                column![
                    row![
                        text("Tooltip Text Size")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.tooltip_font_size))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(
                        8..=28,
                        settings.tooltip_font_size,
                        SettingsMessage::TooltipFontSize
                    )
                    .width(Length::Fill),
                ]
                .spacing(3)
                .width(Length::FillPortion(1)),
                offset_left_col,
                offset_right_col,
            ]
            .spacing(20)
            .width(Length::Fill),
        ]
        .spacing(12)
        .width(Length::Fill)
        .into();

        let render_row = |id_str: &str, is_visible: bool| -> Element<'a, SettingsMessage> {
            let id = id_str.to_string();
            let (w_val, h_val) = settings.tooltip_size(id_str);
            let _id_w = id_str.to_string();
            let _id_h = id_str.to_string();
            let id_reset = id_str.to_string();
            let has_override = settings.tooltip_sizes.contains_key(id_str);

            let up = button(text("▲").size((scale_sz(0.6)) as f32).color(theme.text))
                .on_press(SettingsMessage::MoveCapsuleUp(id.clone()))
                .padding([2, 6])
                .style(move |_t, _s| button::Style {
                    background: None,
                    border: iced::Border::default(),
                    ..Default::default()
                });
            let down = button(text("▼").size((scale_sz(0.6)) as f32).color(theme.text))
                .on_press(SettingsMessage::MoveCapsuleDown(id.clone()))
                .padding([2, 6])
                .style(move |_t, _s| button::Style {
                    background: None,
                    border: iced::Border::default(),
                    ..Default::default()
                });

            let (min_w, min_h): (u32, u32) = min_size_for(id_str);

            let size_controls = if id_str == "tray" {
                row![
                    text("W:").size((scale_sz(0.6)) as f32).color(theme.muted),
                    slider(30..=400, settings.tray_width, SettingsMessage::TrayWidth)
                        .width(Length::Fixed(80.0)),
                    text(format!("{}", settings.tray_width))
                        .size((scale_sz(0.6)) as f32)
                        .color(theme.text)
                        .width(Length::Fixed(25.0)),
                    text("I:").size((scale_sz(0.6)) as f32).color(theme.muted),
                    slider(
                        12..=48,
                        settings.tray_icon_size,
                        SettingsMessage::TrayIconSize
                    )
                    .width(Length::Fixed(80.0)),
                    text(format!("{}px", settings.tray_icon_size))
                        .size((scale_sz(0.6)) as f32)
                        .color(theme.text)
                        .width(Length::Fixed(25.0)),
                    text("T:").size((scale_sz(0.6)) as f32).color(theme.muted),
                    slider(
                        0..=30,
                        settings.tray_timeout_sec,
                        SettingsMessage::TrayTimeout
                    )
                    .width(Length::Fixed(80.0)),
                    text(format!("{}s", settings.tray_timeout_sec))
                        .size((scale_sz(0.6)) as f32)
                        .color(theme.text)
                        .width(Length::Fixed(25.0)),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
            } else {
                let resizer = super::size_resizer::SizeResizer {
                    id: id_str.to_string(),
                    w: w_val,
                    h: h_val,
                    min_w,
                    min_h,
                    max_w: 1200,
                    max_h: 1000,
                    bg: theme.background,
                    fg: theme.text,
                    border: theme.border,
                    muted: theme.muted,
                    display_w: 180.0,
                    display_h: 150.0,
                };
                row![
                    resizer.view(),
                    button(
                        text(if has_override { "×" } else { "·" })
                            .size((scale_sz(0.7)) as f32)
                            .color(if has_override {
                                theme.danger
                            } else {
                                theme.muted
                            })
                    )
                    .on_press(SettingsMessage::ResetTooltipSize(id_reset))
                    .padding([2, 6])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border {
                            color: if has_override {
                                theme.danger
                            } else {
                                theme.border
                            },
                            width: 1.0,
                            radius: 3.0.into()
                        },
                        ..Default::default()
                    }),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            };

            let btn_text = if is_visible { "HIDE" } else { "SHOW" };
            let btn_bg = if is_visible {
                theme.danger
            } else {
                theme.success
            };
            let toggle_btn = button(
                text(btn_text)
                    .size((scale_sz(0.65)) as f32)
                    .bold()
                    .color(Color::BLACK),
            )
            .on_press(SettingsMessage::ToggleCapsuleVisible(id.clone()))
            .padding([4, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(btn_bg)),
                border: iced::Border {
                    color: btn_bg,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            });
            let mk_move = |lbl: &'static str, target: &'static str| {
                button(text(lbl).size((scale_sz(0.55)) as f32).color(theme.text))
                    .on_press(SettingsMessage::MoveCapsuleTo(id.clone(), target.into()))
                    .padding([2, 4])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border {
                            color: theme.text,
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..Default::default()
                    })
            };
            let controls = row![
                mk_move("L", "left"),
                mk_move("C", "center"),
                mk_move("R", "right"),
                toggle_btn
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            let base_row = row![
                row![up, down].spacing(4),
                text(id_str.to_uppercase())
                    .size((scale_sz(0.7)) as f32)
                    .bold()
                    .color(theme.text)
                    .width(Length::Fixed(110.0)),
                size_controls,
                row![].width(Length::Fill),
                controls
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            let mut content_col = column![base_row].spacing(8).width(Length::Fill);

            if id_str == "alarm" {
                let audio_ui = column![
                    row![
                        text("Sound:")
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.muted)
                            .width(Length::Fixed(55.0)),
                        text_input(
                            "filename.mp3",
                            settings.alarm_sound.as_deref().unwrap_or("")
                        )
                        .on_input(SettingsMessage::AlarmSoundChanged)
                        .padding(4)
                        .size((scale_sz(0.7)) as f32)
                        .width(Length::Fill),
                        button(
                            text("Import")
                                .size((scale_sz(0.65)) as f32)
                                .bold()
                                .color(theme.text)
                        )
                        .on_press(SettingsMessage::ImportAlarmSound)
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.background)),
                            border: iced::Border {
                                color: theme.text,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                        button(
                            text("Test")
                                .size((scale_sz(0.65)) as f32)
                                .bold()
                                .color(theme.text)
                        )
                        .on_press(SettingsMessage::TestAlarmSound)
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.background)),
                            border: iced::Border {
                                color: theme.text,
                                width: 1.0,
                                radius: 4.0.into()
                            },
                            ..Default::default()
                        }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Duration:")
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.muted)
                            .width(Length::Fixed(55.0)),
                        slider(
                            1..=60,
                            settings.alarm_sound_duration_sec,
                            SettingsMessage::AlarmSoundDurationChanged
                        )
                        .width(Length::Fill),
                        text(format!("{}s", settings.alarm_sound_duration_sec))
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(25.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Volume:")
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.muted)
                            .width(Length::Fixed(55.0)),
                        slider(
                            0..=100,
                            settings.alarm_sound_volume,
                            SettingsMessage::AlarmSoundVolumeChanged
                        )
                        .width(Length::Fill),
                        text(format!("{}%", settings.alarm_sound_volume))
                            .size((scale_sz(0.7)) as f32)
                            .color(theme.text)
                            .width(Length::Fixed(25.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6)
                .width(Length::Fill);

                content_col = content_col.push(audio_ui);
            }

            container(content_col)
                .padding([8, 10])
                .width(Length::Fill)
                .style(move |_t| container::Style {
                    border: iced::Border {
                        color: theme.text,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        };

        let section_header = |title: &'static str, color: Color| {
            container(row![
                text(title)
                    .size((scale_sz(0.85)) as f32)
                    .bold()
                    .color(color),
                row![].width(Length::Fill),
            ])
            .padding([4, 12])
            .style(move |_t| container::Style {
                border: iced::Border {
                    color,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .width(Length::Fill)
        };

        let mut sections: iced::widget::Column<'a, SettingsMessage> =
            iced::widget::Column::new().spacing(16).width(Length::Fill);

        sections = sections.push(section_header("LEFT BAR SECTION", theme.success));
        for id in &settings.bar_left {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                sections = sections.push(render_row(id, true));
            }
        }
        sections = sections.push(section_header("CENTER BAR SECTION", theme.success));
        for id in &settings.bar_center {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                sections = sections.push(render_row(id, true));
            }
        }
        sections = sections.push(section_header("RIGHT BAR SECTION", theme.success));
        for id in &settings.bar_right {
            if settings.visible_capsules.get(id).copied().unwrap_or(true) {
                sections = sections.push(render_row(id, true));
            }
        }
        sections = sections.push(section_header("HIDDEN MODULES", theme.danger));
        for id_str in all_capsules {
            let id = id_str.to_string();
            let is_vis = settings.visible_capsules.get(&id).copied().unwrap_or(true);
            let in_left = settings.bar_left.contains(&id);
            let in_center = settings.bar_center.contains(&id);
            let in_right = settings.bar_right.contains(&id);
            if !is_vis || (!in_left && !in_center && !in_right) {
                sections = sections.push(render_row(id_str, false));
            }
        }

        container(
            scrollable(
                column![
                    text("BAR GEOMETRY, CAPSULE SHAPES & LAYOUT")
                        .size((scale_sz(1.0)) as f32)
                        .bold()
                        .color(theme.success),
                    mon_row,
                    bg_bar_row,
                    module_slants_row,
                    module_shape_row,
                    top_sliders,
                    anim_sliders,
                    sections,
                ]
                .spacing(16)
                .width(Length::Fill),
            )
            .height(Length::Fill)
            .width(Length::Fill),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: (settings.overlay_cut as f32) + 16.0,
            bottom: (settings.overlay_cut as f32) + 16.0,
            left: (settings.overlay_cut as f32) + 16.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into()
    }

    fn view_overlays_windows<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let mut col = column![
            text("🪟 OVERLAY WINDOW SIZING & GEOMETRY")
                .size(base_sz as f32)
                .bold()
                .color(theme.success),
            text("Global dimensions apply to all overlays (unless individually overridden).")
                .size(scale_sz(0.72) as f32)
                .color(theme.muted),
        ]
        .spacing(8);

        col = col.push(
            container(
                column![
                    row![
                        text("Overlay Window Font Size")
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text),
                        row![].width(Length::Fill),
                        text(format!("{}px", settings.overlay_font_size))
                            .size((scale_sz(0.8)) as f32)
                            .bold()
                            .color(theme.text)
                    ],
                    slider(11..=32, settings.overlay_font_size as u32, |v| {
                        SettingsMessage::OverlayFontSize(v as u16)
                    })
                    .width(Length::Fill),
                ]
                .spacing(6),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        col = col.push(
            container(
                column![
                    text("⬡ Global Overlay Hexagon Geometry")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    row![
                        text("Cut:").size(11).bold().color(theme.text),
                        slider(0..=200, settings.overlay_cut, SettingsMessage::OverlayCut)
                            .width(Length::Fixed(180.0)),
                        text(format!("{}px", settings.overlay_cut))
                            .size(11)
                            .color(theme.text)
                            .width(Length::Fixed(50.0)),
                        text("Angle:").size(11).bold().color(theme.text),
                        slider(
                            5..=85,
                            settings.overlay_angle,
                            SettingsMessage::OverlayAngle
                        )
                        .width(Length::Fixed(180.0)),
                        text(format!("{}°", settings.overlay_angle))
                            .size(11)
                            .color(theme.text)
                            .width(Length::Fixed(40.0)),
                        button(text("30°").size(11).bold().color(
                            if settings.overlay_angle == 30 {
                                theme.success
                            } else {
                                theme.text
                            }
                        ))
                        .on_press(SettingsMessage::OverlayAngle(30))
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: if settings.overlay_angle == 30 {
                                    theme.focus
                                } else {
                                    theme.border
                                },
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                        button(text("45°").size(11).bold().color(
                            if settings.overlay_angle == 45 {
                                theme.success
                            } else {
                                theme.text
                            }
                        ))
                        .on_press(SettingsMessage::OverlayAngle(45))
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: if settings.overlay_angle == 45 {
                                    theme.focus
                                } else {
                                    theme.border
                                },
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                        button(text("60°").size(11).bold().color(
                            if settings.overlay_angle == 60 {
                                theme.success
                            } else {
                                theme.text
                            }
                        ))
                        .on_press(SettingsMessage::OverlayAngle(60))
                        .padding([2, 6])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: if settings.overlay_angle == 60 {
                                    theme.focus
                                } else {
                                    theme.border
                                },
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        col = col.push(
            container(
                column![
                    text("📁 Attachment Download Directory")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    text("Leave blank for default (~/Downloads).")
                        .size(11)
                        .color(theme.muted),
                    text_input("Default (~/Downloads)", &settings.attachment_download_dir)
                        .on_input(SettingsMessage::AttachmentDownloadDirChanged)
                        .padding(8)
                        .size(11)
                        .width(Length::Fill),
                ]
                .spacing(8),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        col = col.push(
            container(
                column![
                    text("⬜ Global Overlay Dimensions")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    row![
                        text("Width:").size(11).bold().color(theme.text),
                        slider(
                            400..=2000,
                            settings.overlay_width,
                            SettingsMessage::OverlayWidth
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.overlay_width))
                            .size(11)
                            .color(theme.text)
                            .width(Length::Fixed(50.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Height:").size(11).bold().color(theme.text),
                        slider(
                            250..=1500,
                            settings.overlay_height,
                            SettingsMessage::OverlayHeight
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.overlay_height))
                            .size(11)
                            .color(theme.text)
                            .width(Length::Fixed(50.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Icon Size:").size(11).bold().color(theme.text),
                        slider(
                            48..=160,
                            settings.launcher_icon_size,
                            SettingsMessage::LauncherIconSize
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.launcher_icon_size))
                            .size(11)
                            .color(theme.text)
                            .width(Length::Fixed(50.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        text("Clipboard Thumb:").size(11).bold().color(theme.text),
                        slider(
                            32..=160,
                            settings.clipboard_thumbnail_size,
                            SettingsMessage::ClipboardThumbnailSize
                        )
                        .width(Length::Fixed(240.0)),
                        text(format!("{}px", settings.clipboard_thumbnail_size))
                            .size(11)
                            .color(theme.text)
                            .width(Length::Fixed(50.0)),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        // ── Screenshot Save Format ─────────────────────────────
        let fmt_btn =
            |fmt: ScreenshotSaveFormat, label: &'static str| -> Element<'a, SettingsMessage> {
                let is_sel = settings.screenshot_save_format == fmt;
                button(text(label).size(11).bold().color(if is_sel {
                    theme.background
                } else {
                    theme.text
                }))
                .on_press(SettingsMessage::ScreenshotSaveFormat(fmt))
                .padding([3, 8])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(if is_sel {
                        theme.text
                    } else {
                        Color::TRANSPARENT
                    })),
                    border: iced::Border {
                        color: if is_sel { theme.text } else { theme.border },
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
                .into()
            };

        col = col.push(
            container(
                column![
                    text("🖼 Screenshot Save Format")
                        .size(scale_sz(0.8) as f32)
                        .bold()
                        .color(theme.success),
                    text("PNG saves in ~0.5s for 4K. JXL is 4-20x slower but 4-5x smaller.")
                        .size(11)
                        .color(theme.muted),
                    row![
                        text("Format:").size(11).bold().color(theme.text),
                        fmt_btn(ScreenshotSaveFormat::Png, "PNG (fast, large)"),
                        fmt_btn(ScreenshotSaveFormat::JxlFast, "JXL (balanced)"),
                        fmt_btn(ScreenshotSaveFormat::JxlBest, "JXL (smallest)"),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            )
            .padding([10, 14])
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgb8(18, 20, 26))),
                border: iced::Border {
                    color: theme.border,
                    width: 1.5,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );

        container(scrollable(col).height(Length::Fill))
            .padding(iced::Padding {
                top: 0.0,
                right: (settings.overlay_cut as f32) + 16.0,
                bottom: (settings.overlay_cut as f32) + 16.0,
                left: (settings.overlay_cut as f32) + 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into()
    }

    fn view_ipc_commands<'a>(
        &'a self,
        settings: &'a ShellSettings,
        theme: &'a ShellTheme,
    ) -> Element<'a, SettingsMessage> {
        let base_sz = theme.font_size_overlay;
        let scale_sz = |factor: f32| -> u16 { ((base_sz as f32) * factor).round().max(9.0) as u16 };
        let fs_desc = scale_sz(0.72) as f32;
        let fs_cmd = scale_sz(0.75) as f32;
        let fs_head = scale_sz(0.85) as f32;

        // (title, [(description, command), ...])
        let sections: &[(&str, &[(&str, &str)])] = &[
            (
                "MUSIC",
                &[
                    (
                        "Cycle through active players (play, pause, next player, play, ...)",
                        "moonbeauty --music-cycle",
                    ),
                    (
                        "Play/pause the currently active player",
                        "moonbeauty --music-play-pause",
                    ),
                    (
                        "Skip to the next track on the active player",
                        "moonbeauty --music-next",
                    ),
                    (
                        "Go to the previous track on the active player",
                        "moonbeauty --music-previous",
                    ),
                ],
            ),
            (
                "OVERLAYS — each command toggles. Both --<target> and --toggle <target> work.",
                &[
                    ("Application launcher", "moonbeauty --launcher"),
                    ("Clipboard manager", "moonbeauty --clipboard"),
                    ("Notification history", "moonbeauty --history"),
                    ("Settings", "moonbeauty --settings"),
                    ("Calculator & units", "moonbeauty --calc"),
                    ("Password store", "moonbeauty --pass"),
                    ("Dictionary", "moonbeauty --dict"),
                    ("Unicode glyphs", "moonbeauty --unicode"),
                    ("Tasks & todos", "moonbeauty --todo"),
                    ("Reminders & alarms", "moonbeauty --reminders"),
                    ("Dice & RNG", "moonbeauty --rng"),
                    ("Web search", "moonbeauty --web"),
                    ("Email client", "moonbeauty --email"),
                    ("Among Us tracker", "moonbeauty --amogus"),
                    (
                        "Lock screen (must use --toggle; bare --lockscreen starts the coordinator)",
                        "moonbeauty --toggle lockscreen",
                    ),
                ],
            ),
            (
                "NOTIFICATIONS",
                &[
                    (
                        "Send a notification from a script or shell",
                        "moonbeauty --notify \"Title\" \"Body\"",
                    ),
                    (
                        "Focus the app that sent the newest toast",
                        "moonbeauty --activate-latest-notif",
                    ),
                    ("Dismiss the newest toast", "moonbeauty --dismiss-notif"),
                    (
                        "Toggle the notification history panel",
                        "moonbeauty --notif-history",
                    ),
                ],
            ),
            (
                "SCREENSHOTS",
                &[
                    (
                        "Capture a region, save to ~/Pictures/Screenshots",
                        "moonbeauty --screenshot",
                    ),
                    (
                        "Capture a region, open the annotator",
                        "moonbeauty --screenshot --annotate",
                    ),
                ],
            ),
            (
                "SYSTEM",
                &[
                    (
                        "Restart the bar (picks up new settings)",
                        "moonbeauty --restart-bar",
                    ),
                    ("Test IMAP/SMTP email connection", "moonbeauty --test-email"),
                    (
                        "Test lockscreen password authentication",
                        "moonbeauty --test-lock-auth",
                    ),
                ],
            ),
            (
                "SWAY KEYBIND EXAMPLES",
                &[
                    (
                        "Cycle music players with Super+M",
                        "bindsym $mod+m exec moonbeauty --music-cycle",
                    ),
                    (
                        "Play/pause with Super+Shift+M",
                        "bindsym $mod+Shift+m exec moonbeauty --music-play-pause",
                    ),
                    (
                        "Next track with Super+Shift+Right",
                        "bindsym $mod+Shift+Right exec moonbeauty --music-next",
                    ),
                    (
                        "Toggle notification history with Super+N",
                        "bindsym $mod+n exec moonbeauty --history",
                    ),
                    (
                        "Region screenshot with Print",
                        "bindsym Print exec moonbeauty --screenshot",
                    ),
                ],
            ),
        ];

        let mut col = column![].spacing(20).width(Length::Fill);

        col = col.push(
            container(
                text("Click 📋 to copy a command. Paste into a sway keybind, shell alias, or terminal.")
                    .size(fs_desc)
                    .color(theme.muted),
            )
            .padding([10, 14])
            .width(Length::Fill)
            .style(move |_t| container::Style {
                background: Some(iced::Background::Color(theme.surface_alt)),
                border: iced::Border {
                    color: theme.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        );

        for (title, entries) in sections {
            col = col.push(text(*title).size(fs_head).bold().color(theme.success));

            let mut section_col = column![].spacing(6).width(Length::Fill);
            for (desc, cmd) in entries.iter() {
                let cmd_clone = (*cmd).to_string();

                let desc_el = text(*desc).size(fs_desc).color(theme.muted);

                let cmd_el = text(*cmd)
                    .size(fs_cmd)
                    .font(iced::Font::MONOSPACE)
                    .color(theme.text)
                    .wrapping(iced::widget::text::Wrapping::None);

                let copy_btn = button(
                    container(text("📋").size(14))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .on_press(SettingsMessage::CopyIpc(cmd_clone))
                .padding(0)
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(32.0))
                .style(move |_t, status| button::Style {
                    background: Some(iced::Background::Color(match status {
                        button::Status::Hovered | button::Status::Pressed => theme.surface_alt,
                        _ => theme.background,
                    })),
                    border: iced::Border {
                        color: theme.text,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    text_color: theme.text,
                    ..Default::default()
                });

                let row = container(
                    row![
                        column![desc_el, cmd_el].spacing(2).width(Length::Fill),
                        copy_btn,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .width(Length::Fill),
                )
                .padding([6, 10])
                .width(Length::Fill)
                .style(move |_t| container::Style {
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });

                section_col = section_col.push(row);
            }
            col = col.push(section_col);
        }

        container(scrollable(col).height(Length::Fill).width(Length::Fill))
            .padding(iced::Padding {
                top: 0.0,
                right: (settings.overlay_cut as f32) + 16.0,
                bottom: (settings.overlay_cut as f32) + 16.0,
                left: (settings.overlay_cut as f32) + 16.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into()
    }
}
