use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::theme::Base16Palette;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentRule {
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub readout: String,
    #[serde(default)]
    pub sound_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendProfile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default = "default_profile_color")]
    pub color: String,
    #[serde(default)]
    pub sound: bool,
    #[serde(default)]
    pub sound_path: Option<String>,
    #[serde(default)]
    pub tts: bool,
    #[serde(default)]
    pub default_readout: Option<String>,
    #[serde(default)]
    pub content_rules: Vec<ContentRule>,
}

fn default_profile_color() -> String { "#f7f700".to_string() }

pub fn find_friend_profiles_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let candidates = [
        PathBuf::from("resources/friend_profiles.json"),
        home.join("projects/Moon-Beauty-Shell/resources/friend_profiles.json"),
        dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("moon-beauty-shell/resources/friend_profiles.json"),
    ];

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }
    PathBuf::from("resources/friend_profiles.json")
}

pub fn load_friend_profiles() -> Vec<FriendProfile> {
    let path = find_friend_profiles_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(list) = serde_json::from_str::<Vec<FriendProfile>>(&content) {
                return list;
            }
        }
    }
    Vec::new()
}

pub fn save_friend_profiles(profiles: &[FriendProfile]) -> Result<(), std::io::Error> {
    let path = find_friend_profiles_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(profiles)?;
    fs::write(path, json)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuThresholds {
    #[serde(default = "default_temp_warn", alias = "tempWarn", alias = "temp_warn")]
    pub temp_warn: u32,
    #[serde(default = "default_temp_danger", alias = "tempDanger", alias = "temp_danger")]
    pub temp_danger: u32,
    #[serde(default = "default_vram_warn", alias = "vramWarn", alias = "vram_warn")]
    pub vram_warn: u32,
    #[serde(default = "default_vram_danger", alias = "vramDanger", alias = "vram_danger")]
    pub vram_danger: u32,
}

fn default_temp_warn() -> u32 { 70 }
fn default_temp_danger() -> u32 { 80 }
fn default_vram_warn() -> u32 { 4 }
fn default_vram_danger() -> u32 { 2 }

impl Default for GpuThresholds {
    fn default() -> Self {
        Self {
            temp_warn: default_temp_warn(),
            temp_danger: default_temp_danger(),
            vram_warn: default_vram_warn(),
            vram_danger: default_vram_danger(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDim {
    pub width: u32,
    pub height: u32,
    #[serde(default, alias = "fieldHeight")]
    pub field_height: Option<u32>,
    #[serde(default, alias = "dismissOnBlur")]
    pub dismiss_on_blur: Option<bool>,
    #[serde(default, alias = "escapeCloses")]
    pub escape_closes: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSettings {
    #[serde(default = "default_bar_height", alias = "barHeight")]
    pub bar_height: u32,
    #[serde(default = "default_font_size", alias = "globalFontSize")]
    pub global_font_size: u16,
    #[serde(default = "default_overlay_font_size", alias = "overlayFontSize")]
    pub overlay_font_size: u16,
    #[serde(default = "default_tooltip_font_size", alias = "tooltipFontSize")]
    pub tooltip_font_size: u16,
    #[serde(default = "default_slant_width", alias = "slantWidth")]
    pub slant_width: f32,
    #[serde(default = "default_tooltip_drop_ms", alias = "tooltipDropMs")]
    pub tooltip_drop_ms: u32,
    #[serde(default = "default_tooltip_expand_ms", alias = "tooltipExpandMs")]
    pub tooltip_expand_ms: u32,
    #[serde(default = "default_tooltip_width_px", alias = "tooltipWidthPx")]
    pub tooltip_width_px: u32,
    #[serde(default = "default_tooltip_height_px", alias = "tooltipHeightPx")]
    pub tooltip_height_px: u32,
    #[serde(default = "default_tooltip_pause_ms", alias = "tooltipPauseMs")]
    pub tooltip_pause_ms: u32,
    #[serde(default, alias = "tooltipSizes")]
    pub tooltip_sizes: HashMap<String, (u32, u32)>,
    #[serde(default = "default_slant_mode", alias = "slantStyleMode")]
    pub slant_mode: String,
    #[serde(default = "default_bar_shape", alias = "barShapeStyle")]
    pub bar_shape: String,
    #[serde(default = "default_bar_shape", alias = "moduleShapeStyle")]
    pub module_shape: String,
    #[serde(default = "default_global_border_width", alias = "globalBorderWidth")]
    pub global_border_width: f32,
    #[serde(default = "default_control_border_width", alias = "controlBorderWidth")]
    pub control_border_width: f32,
    #[serde(default = "default_global_padding", alias = "globalPadding")]
    pub global_padding: u16,
    #[serde(default = "default_capsule_spacing", alias = "capsuleSpacing")]
    pub capsule_spacing: i32,
    #[serde(default = "default_tray_width", alias = "trayWidth")]
    pub tray_width: u32,
    #[serde(default = "default_tray_timeout", alias = "trayTimeout")]
    pub tray_timeout_sec: u32,
    #[serde(default = "default_tray_icon_size", alias = "trayIconSize")]
    pub tray_icon_size: u32,
    #[serde(default = "default_poll_interval", alias = "hardwarePollInterval", alias = "hardware_poll_interval_ms")]
    pub hardware_poll_interval_ms: u64,

    #[serde(default = "default_true", alias = "notificationsEnabled")]
    pub notifications_enabled: bool,
    #[serde(default = "default_notif_hold", alias = "notifHoldDurationSec")]
    pub notif_hold_duration_sec: u64,
    #[serde(default = "default_notif_volume", alias = "notifVolume")]
    pub notif_volume: u32,
    #[serde(default = "default_notif_corner", alias = "notifCorner")]
    pub notif_corner: String,
    #[serde(default = "default_notif_shape", alias = "notificationShapeStyle")]
    pub notif_shape: String,
    #[serde(default = "default_notif_screen", alias = "notifScreenName")]
    pub notif_screen_name: String,
    #[serde(default = "default_bar_monitor", alias = "barMonitor")]
    pub bar_monitor: String,
    #[serde(default = "default_notif_baseline", alias = "notifBaselineY")]
    pub notif_baseline_y: i32,
    #[serde(default = "default_notif_margin_x", alias = "notifMarginX")]
    pub notif_margin_x: i32,
    #[serde(default = "default_notif_overlap", alias = "notifStackOverlap")]
    pub notif_stack_overlap: i32,
    #[serde(default = "default_notif_card_width", alias = "notifCardWidth")]
    pub notif_card_width: u32,
    #[serde(default = "default_notif_card_height", alias = "notifCardHeight")]
    pub notif_card_height: u32,
    #[serde(default = "default_slide_in", alias = "notifSlideInDurationMs")]
    pub notif_slide_in_ms: u32,
    #[serde(default = "default_drop_pause", alias = "notifDropPauseMs")]
    pub notif_drop_pause_ms: u32,
    #[serde(default = "default_drop_distance", alias = "notifDropDistancePx")]
    pub notif_drop_distance_px: u32,
    #[serde(default = "default_summary_size", alias = "textSummarySize")]
    pub notif_summary_size: u16,
    #[serde(default = "default_body_size", alias = "textBodySize")]
    pub notif_body_size: u16,
    #[serde(default = "default_excluded", alias = "notifExcludedStr")]
    pub notif_excluded: String,
    #[serde(default, alias = "alarmSound")]
    pub alarm_sound: Option<String>,
    #[serde(default = "default_alarm_sound_duration", alias = "alarmSoundDurationSec")]
    pub alarm_sound_duration_sec: u32,
    #[serde(default = "default_alarm_sound_volume", alias = "alarmSoundVolume")]
    pub alarm_sound_volume: u32,

    #[serde(default = "load_friend_profiles", alias = "friendProfiles")]
    pub friend_profiles: Vec<FriendProfile>,

    #[serde(default = "default_bar_left", alias = "barLeft", alias = "barLeftModules")]
    pub bar_left: Vec<String>,
    #[serde(default = "default_bar_center", alias = "barCenter", alias = "barCenterModules")]
    pub bar_center: Vec<String>,
    #[serde(default = "default_bar_right", alias = "barRight", alias = "barRightModules")]
    pub bar_right: Vec<String>,
    #[serde(default, alias = "visibleCapsules")]
    pub visible_capsules: HashMap<String, bool>,
    #[serde(default, alias = "capsuleSlants")]
    pub capsule_slants: HashMap<String, String>,
    #[serde(default, alias = "capsuleShapes")]
    pub capsule_shapes: HashMap<String, String>,
    #[serde(default, alias = "capsuleBarWidths")]
    pub capsule_bar_widths: HashMap<String, u32>,
    #[serde(default, alias = "standaloneWindows")]
    pub standalone_windows: HashMap<String, WindowDim>,
    #[serde(default, alias = "gpu0")]
    pub gpu0: GpuThresholds,
    #[serde(default, alias = "gpu1")]
    pub gpu1: GpuThresholds,
    #[serde(default = "default_gpu_selected_id", alias = "gpuSelectedId", alias = "gpu_selected")]
    pub gpu_selected_id: String,
    #[serde(default, alias = "customColors", alias = "palette")]
    pub palette: Base16Palette,
    #[serde(default, alias = "ttsCommand")]
    pub tts_command: Option<String>,
    #[serde(default = "default_overlay_cut", alias = "overlayCut", alias = "overlay_cut", alias = "launcherCut", alias = "launcher_cut")]
    pub overlay_cut: u32,
    #[serde(default = "default_overlay_angle", alias = "overlayAngle", alias = "overlay_angle", alias = "launcherAngle", alias = "launcher_angle")]
    pub overlay_angle: u32,
}

fn default_bar_height() -> u32 { 42 }
fn default_font_size() -> u16 { 14 }
fn default_overlay_font_size() -> u16 { 16 }
fn default_tooltip_font_size() -> u16 { 14 }
fn default_slant_width() -> f32 { 12.0 }
fn default_tooltip_drop_ms() -> u32 { 180 }
fn default_tooltip_expand_ms() -> u32 { 162 }
fn default_tooltip_width_px() -> u32 { 380 }
fn default_tooltip_height_px() -> u32 { 300 }
fn default_tooltip_pause_ms() -> u32 { 0 }
fn default_slant_mode() -> String { "symmetric".to_string() }
fn default_bar_shape() -> String { "slant".to_string() }
fn default_global_border_width() -> f32 { 3.0 }
fn default_control_border_width() -> f32 { 2.0 }
fn default_global_padding() -> u16 { 12 }
fn default_capsule_spacing() -> i32 { 2 }
fn default_tray_width() -> u32 { 60 }
fn default_tray_timeout() -> u32 { 3 }
fn default_tray_icon_size() -> u32 { 20 }
fn default_poll_interval() -> u64 { 2000 }
fn default_true() -> bool { true }

fn default_notif_hold() -> u64 { 3 }
fn default_notif_volume() -> u32 { 50 }
fn default_notif_corner() -> String { "top-right".to_string() }
fn default_notif_shape() -> String { "hex".to_string() }
fn default_notif_screen() -> String { "Auto (Primary)".to_string() }
fn default_bar_monitor() -> String { "All".to_string() }
fn default_notif_baseline() -> i32 { 340 }
fn default_notif_margin_x() -> i32 { 0 }
fn default_notif_overlap() -> i32 { -20 }
fn default_notif_card_width() -> u32 { 400 }
fn default_notif_card_height() -> u32 { 150 }
fn default_slide_in() -> u32 { 250 }
fn default_drop_pause() -> u32 { 100 }
fn default_drop_distance() -> u32 { 50 }
fn default_summary_size() -> u16 { 24 }
fn default_body_size() -> u16 { 22 }
fn default_excluded() -> String { "greenclip, copyq".to_string() }
fn default_alarm_sound_duration() -> u32 { 5 }
fn default_alarm_sound_volume() -> u32 { 100 }
fn default_gpu_selected_id() -> String { "card0".to_string() }
fn default_overlay_cut() -> u32 { 24 }
fn default_overlay_angle() -> u32 { 45 }

fn default_bar_left() -> Vec<String> {
    vec!["calendar".into(), "music".into(), "alarm".into(), "weather".into(), "unified".into(), "notify".into()]
}
fn default_bar_center() -> Vec<String> {
    vec!["audio".into(), "clock".into(), "mic".into(), "custom".into()]
}
fn default_bar_right() -> Vec<String> {
    vec!["tray".into(), "ram".into(), "gpu".into(), "cpu".into(), "net".into(), "battery".into()]
}

impl Default for ShellSettings {
    fn default() -> Self {
        let mut visible = HashMap::new();
        for key in &[
            "calendar", "music", "alarm", "weather", "unified", "notify", "clock",
            "audio", "mic", "net", "cpu", "gpu", "ram", "tray", "battery", "custom",
        ] {
            visible.insert(key.to_string(), true);
        }

        Self {
            bar_height: default_bar_height(),
            global_font_size: default_font_size(),
            overlay_font_size: default_overlay_font_size(),
            tooltip_font_size: default_tooltip_font_size(),
            slant_width: default_slant_width(),
            tooltip_drop_ms: default_tooltip_drop_ms(),
            tooltip_expand_ms: default_tooltip_expand_ms(),
            tooltip_width_px: default_tooltip_width_px(),
            tooltip_height_px: default_tooltip_height_px(),
            tooltip_pause_ms: default_tooltip_pause_ms(),
            tooltip_sizes: HashMap::new(),
            slant_mode: default_slant_mode(),
            bar_shape: default_bar_shape(),
            module_shape: "slant".into(),
            global_border_width: default_global_border_width(),
            control_border_width: default_control_border_width(),
            global_padding: default_global_padding(),
            capsule_spacing: default_capsule_spacing(),
            tray_width: default_tray_width(),
            tray_timeout_sec: default_tray_timeout(),
            tray_icon_size: default_tray_icon_size(),
            hardware_poll_interval_ms: default_poll_interval(),
            notifications_enabled: true,
            notif_hold_duration_sec: default_notif_hold(),
            notif_volume: default_notif_volume(),
            notif_corner: default_notif_corner(),
            notif_shape: default_notif_shape(),
            notif_screen_name: default_notif_screen(),
            bar_monitor: default_bar_monitor(),
            notif_baseline_y: default_notif_baseline(),
            notif_margin_x: default_notif_margin_x(),
            notif_stack_overlap: default_notif_overlap(),
            notif_card_width: default_notif_card_width(),
            notif_card_height: default_notif_card_height(),
            notif_slide_in_ms: default_slide_in(),
            notif_drop_pause_ms: default_drop_pause(),
            notif_drop_distance_px: default_drop_distance(),
            notif_summary_size: default_summary_size(),
            notif_body_size: default_body_size(),
            notif_excluded: default_excluded(),
            alarm_sound: None,
            alarm_sound_duration_sec: default_alarm_sound_duration(),
            alarm_sound_volume: default_alarm_sound_volume(),
            friend_profiles: load_friend_profiles(),
            bar_left: default_bar_left(),
            bar_center: default_bar_center(),
            bar_right: default_bar_right(),
            visible_capsules: visible,
            capsule_slants: HashMap::new(),
            capsule_shapes: HashMap::new(),
            capsule_bar_widths: HashMap::new(),
            standalone_windows: HashMap::new(),
            gpu0: GpuThresholds::default(),
            gpu1: GpuThresholds {
                temp_warn: 65,
                temp_danger: 75,
                vram_warn: 1,
                vram_danger: 0,
            },
            gpu_selected_id: default_gpu_selected_id(),
            palette: Base16Palette::default(),
            tts_command: None,
            overlay_cut: default_overlay_cut(),
            overlay_angle: default_overlay_angle(),
        }
    }
}

impl ShellSettings {
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        config_dir.join("moon-beauty-shell").join("settings.json")
    }

    pub fn tooltip_size(&self, id: &str) -> (u32, u32) {
        self.tooltip_sizes.get(id).copied()
            .unwrap_or((self.tooltip_width_px, self.tooltip_height_px))
    }

    pub fn get_window_size(&self, id: &str, def_w: u32, def_h: u32) -> (f32, f32) {
        if let Some(win) = self.standalone_windows.get(id) {
            (win.width as f32, win.height as f32)
        } else {
            (def_w as f32, def_h as f32)
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut settings) = serde_json::from_str::<ShellSettings>(&content) {
                    let disk_profiles = load_friend_profiles();
                    if !disk_profiles.is_empty() {
                        settings.friend_profiles = disk_profiles;
                    }
                    return settings;
                }
            }
        }
        let mut def = Self::default();
        def.friend_profiles = load_friend_profiles();
        def
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        eprintln!("[save] writing tooltip_sizes={:?} bar_monitor={:?}",
            self.tooltip_sizes, self.bar_monitor);
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, &json)?;
        fs::rename(tmp_path, &path)?;

        let _ = save_friend_profiles(&self.friend_profiles);
        eprintln!("[save] OK -> {:?}", path);
        Ok(())
    }
}
