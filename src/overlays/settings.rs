use iced::widget::{button, column, container, row, scrollable, slider, text, text_input, Stack};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::config::settings::{ContentRule, FriendProfile, ShellSettings};
use crate::config::theme::{hex_to_color, ShellTheme};
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Notifications,
    StylixSliders,
    ColorsStyles,
    GpuHardware,
    BarLayout,
    OverlaysWindows,
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
    BorderWidth(u32),
    ControlBorderWidth(u32),
    CapsuleSpacing(i32),
    HardwarePollInterval(u64),

    // Bar Geometry
    SelectBarShape(String),
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
    NotifExcludedChanged(String),

    // Custom Pings
    SelectProfile(usize),
    ToggleProfileSound(usize),
    ToggleProfileTts(usize),
    ProfileSummaryChanged(usize, String),
    ProfileColorChanged(usize, String),
    ProfileSoundPathChanged(usize, String),
    ProfileTtsTemplateChanged(usize, String),
    DeleteProfile(usize),
    AddNewProfile,

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

    // Window Sizing & Geometry
    WindowWidth(String, u32),
    WindowHeight(String, u32),
    OverlayCut(u32),
    OverlayAngle(u32),

    SaveSettings,
    TestNotification,
    Dismiss,
}

pub struct SettingsOverlay {
    pub active_tab: SettingsTab,
    pub selected_screen: String,
    pub selected_profile: usize,
}

impl SettingsOverlay {
    pub fn new(_settings: &ShellSettings) -> Self {
        Self {
            active_tab: SettingsTab::Notifications,
            selected_screen: "Auto (Primary)".into(),
            selected_profile: 0,
        }
    }

    pub fn update(&mut self, message: SettingsMessage, settings: &mut ShellSettings) {
        match message {
            SettingsMessage::SelectTab(t) => self.active_tab = t,

            SettingsMessage::BarHeight(v) => settings.bar_height = v.clamp(28, 90),
            SettingsMessage::GlobalFontSize(v) => settings.global_font_size = v.clamp(8, 32),
            SettingsMessage::OverlayFontSize(v) => settings.overlay_font_size = v.clamp(10, 36),
            SettingsMessage::TooltipFontSize(v) => settings.tooltip_font_size = v.clamp(8, 28),
            SettingsMessage::SlantWidth(v) => settings.slant_width = (v as f32).clamp(0.0, 40.0),
            SettingsMessage::BorderWidth(v) => settings.global_border_width = (v as f32).clamp(1.0, 12.0),
            SettingsMessage::ControlBorderWidth(v) => settings.control_border_width = (v as f32).clamp(1.0, 8.0),
            SettingsMessage::CapsuleSpacing(v) => settings.capsule_spacing = v.clamp(-50, 40),
            SettingsMessage::HardwarePollInterval(v) => settings.hardware_poll_interval_ms = v.clamp(500, 10000),

            SettingsMessage::SelectBarShape(s) => settings.bar_shape = s,
            SettingsMessage::SelectSlantMode(m) => settings.slant_mode = m,

            SettingsMessage::ToggleNotifications => settings.notifications_enabled = !settings.notifications_enabled,
            SettingsMessage::SelectNotifCorner(c) => settings.notif_corner = c,
            SettingsMessage::SelectNotifShape(s) => settings.notif_shape = s,
            SettingsMessage::SelectNotifScreen(s) => {
                self.selected_screen = s.clone();
                settings.notif_screen_name = s;
            }
            SettingsMessage::NotifHoldDuration(v) => settings.notif_hold_duration_sec = v.clamp(1, 30),
            SettingsMessage::NotifVolume(v) => settings.notif_volume = v.clamp(0, 100),
            SettingsMessage::NotifBaselineY(v) => settings.notif_baseline_y = v.clamp(20, 1200),
            SettingsMessage::NotifMarginX(v) => settings.notif_margin_x = v.clamp(0, 200),
            SettingsMessage::NotifStackOverlap(v) => settings.notif_stack_overlap = v.clamp(-100, 100),
            SettingsMessage::NotifCardWidth(v) => settings.notif_card_width = v.clamp(250, 800),
            SettingsMessage::NotifCardHeight(v) => settings.notif_card_height = v.clamp(80, 300),
            SettingsMessage::NotifSlideIn(v) => settings.notif_slide_in_ms = v.clamp(20, 1000),
            SettingsMessage::NotifDropPause(v) => settings.notif_drop_pause_ms = v.clamp(0, 500),
            SettingsMessage::NotifDropDistance(v) => settings.notif_drop_distance_px = v.clamp(10, 500),
            SettingsMessage::NotifSummarySize(v) => settings.notif_summary_size = v.clamp(10, 36),
            SettingsMessage::NotifBodySize(v) => settings.notif_body_size = v.clamp(8, 32),
            SettingsMessage::NotifExcludedChanged(v) => settings.notif_excluded = v,

            SettingsMessage::SelectProfile(idx) => self.selected_profile = idx,
            SettingsMessage::ToggleProfileSound(idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.sound = !p.sound;
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::ToggleProfileTts(idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.tts = !p.tts;
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::ProfileSummaryChanged(idx, val) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.summary = val.clone();
                    p.name = val.to_lowercase().replace(' ', "_");
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::ProfileColorChanged(idx, col) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.color = col;
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::ProfileSoundPathChanged(idx, path) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.sound_path = if path.trim().is_empty() { None } else { Some(path.trim().to_string()) };
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::ProfileTtsTemplateChanged(idx, tpl) => {
                if let Some(p) = settings.friend_profiles.get_mut(idx) {
                    p.default_readout = if tpl.trim().is_empty() { None } else { Some(tpl.trim().to_string()) };
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::DeleteProfile(idx) => {
                if idx < settings.friend_profiles.len() {
                    settings.friend_profiles.remove(idx);
                    if self.selected_profile >= settings.friend_profiles.len() {
                        self.selected_profile = settings.friend_profiles.len().saturating_sub(1);
                    }
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::AddNewProfile => {
                settings.friend_profiles.push(FriendProfile {
                    name: "new_ping".into(),
                    summary: "New Ping".into(),
                    color: "#38FEDC".into(),
                    sound: true,
                    sound_path: None,
                    tts: true,
                    default_readout: Some("Message from {name}".into()),
                    content_rules: Vec::new(),
                });
                self.selected_profile = settings.friend_profiles.len().saturating_sub(1);
                let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
            }

            SettingsMessage::AddContentRule(p_idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    p.content_rules.push(ContentRule {
                        pattern: "logged in".into(),
                        readout: "{name} has logged in".into(),
                        sound_path: None,
                    });
                    let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                }
            }
            SettingsMessage::DeleteContentRule(p_idx, r_idx) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    if r_idx < p.content_rules.len() {
                        p.content_rules.remove(r_idx);
                        let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                    }
                }
            }
            SettingsMessage::RulePatternChanged(p_idx, r_idx, val) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    if let Some(r) = p.content_rules.get_mut(r_idx) {
                        r.pattern = val;
                        let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                    }
                }
            }
            SettingsMessage::RuleReadoutChanged(p_idx, r_idx, val) => {
                if let Some(p) = settings.friend_profiles.get_mut(p_idx) {
                    if let Some(r) = p.content_rules.get_mut(r_idx) {
                        r.readout = val;
                        let _ = crate::config::settings::save_friend_profiles(&settings.friend_profiles);
                    }
                }
            }

            SettingsMessage::ColorChanged(slot, hex) => {
                match slot.as_str() {
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
                }
            }

            SettingsMessage::GpuTempWarn(v) => settings.gpu0.temp_warn = v,
            SettingsMessage::GpuTempDanger(v) => settings.gpu0.temp_danger = v,
            SettingsMessage::GpuVramWarn(v) => settings.gpu0.vram_warn = v,
            SettingsMessage::GpuVramDanger(v) => settings.gpu0.vram_danger = v,

            SettingsMessage::ToggleCapsuleVisible(id) => {
                let cur = settings.visible_capsules.get(&id).copied().unwrap_or(true);
                settings.visible_capsules.insert(id, !cur);
            }

            SettingsMessage::WindowWidth(id, w) => {
                let entry = settings.standalone_windows.entry(id).or_insert(crate::config::settings::WindowDim {
                    width: w,
                    height: 650,
                    field_height: None,
                    dismiss_on_blur: Some(true),
                    escape_closes: Some(true),
                });
                entry.width = w;
            }
            SettingsMessage::WindowHeight(id, h) => {
                let entry = settings.standalone_windows.entry(id).or_insert(crate::config::settings::WindowDim {
                    width: 840,
                    height: h,
                    field_height: None,
                    dismiss_on_blur: Some(true),
                    escape_closes: Some(true),
                });
                entry.height = h;
            }
            SettingsMessage::OverlayCut(v) => {
                settings.overlay_cut = v.clamp(0, 200);
            }
            SettingsMessage::OverlayAngle(v) => {
                settings.overlay_angle = v.clamp(5, 85);
            }

            SettingsMessage::TestNotification => {
                settings.friend_profiles = crate::config::settings::load_friend_profiles();
            }
            SettingsMessage::SaveSettings => {}
            SettingsMessage::Dismiss => {}
        }

        let _ = settings.save();
    }

    pub fn view<'a>(&'a self, settings: &'a ShellSettings, theme: &'a ShellTheme) -> Element<'a, SettingsMessage> {
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
            (SettingsTab::StylixSliders, "Stylix & Sliders"),
            (SettingsTab::ColorsStyles, "Colors & Styles"),
            (SettingsTab::GpuHardware, "GPU & Hardware"),
            (SettingsTab::BarLayout, "Bar Layout & Slants"),
            (SettingsTab::OverlaysWindows, "Overlays & Windows"),
        ];

        let mut tab_row = row![].spacing(6).align_y(Alignment::Center);
        for (tab, label) in tabs {
            let is_sel = self.active_tab == tab;
            let col = if is_sel { theme.background } else { theme.text };
            let bg_col = if is_sel { theme.text } else { Color::TRANSPARENT };

            let btn_item = button(
                text(label)
                    .size(scale_sz(0.82))
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
            SettingsTab::Notifications => {
                let mut pings_col = column![
                    text("CUSTOM PINGS")
                        .size(scale_sz(0.95))
                        .bold()
                        .color(theme.success),
                    row![
                        text("ALL:").size(scale_sz(0.68)).bold().color(theme.text),
                        text("Sound").size(scale_sz(0.68)).bold().color(theme.danger),
                        text("TTS").size(scale_sz(0.68)).bold().color(theme.danger),
                    ].spacing(6).align_y(Alignment::Center),
                ].spacing(8).width(Length::Fixed(270.0));

                let mut pings_list = column![].spacing(6);
                for (idx, p) in settings.friend_profiles.iter().enumerate() {
                    let p_color = hex_to_color(&p.color);
                    let p_name = p.summary.clone();
                    let is_sound = p.sound;
                    let is_tts = p.tts;
                    let is_selected = idx == self.selected_profile;

                    let sound_btn = button(
                        text("Sound").size(scale_sz(0.6)).bold().color(if is_sound { Color::BLACK } else { Color::from_rgb8(255, 60, 60) })
                    )
                    .on_press(SettingsMessage::ToggleProfileSound(idx))
                    .padding([2, 6])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(if is_sound { Color::from_rgb8(0, 224, 0) } else { Color::TRANSPARENT })),
                        border: iced::Border { color: if is_sound { Color::from_rgb8(0, 224, 0) } else { Color::from_rgb8(60, 60, 80) }, width: 1.0, radius: 3.0.into() },
                        ..Default::default()
                    });

                    let tts_btn = button(
                        text("TTS").size(scale_sz(0.6)).bold().color(if is_tts { Color::BLACK } else { Color::from_rgb8(255, 60, 60) })
                    )
                    .on_press(SettingsMessage::ToggleProfileTts(idx))
                    .padding([2, 6])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(if is_tts { Color::from_rgb8(0, 224, 0) } else { Color::TRANSPARENT })),
                        border: iced::Border { color: if is_tts { Color::from_rgb8(0, 224, 0) } else { Color::from_rgb8(60, 60, 80) }, width: 1.0, radius: 3.0.into() },
                        ..Default::default()
                    });

                    let swatch = container(text(""))
                        .width(Length::Fixed(14.0))
                        .height(Length::Fixed(14.0))
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(p_color)),
                            border: iced::Border { color: Color::WHITE, width: 1.0, radius: 7.0.into() },
                            ..Default::default()
                        });

                    let capsule_content = row![
                        swatch,
                        text(p_name).size(scale_sz(0.75)).bold().color(if is_selected { theme.text } else { Color::from_rgb8(200, 200, 200) }).width(Length::Fill),
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
                            background: Some(iced::Background::Color(if is_selected { Color::from_rgb8(34, 38, 50) } else { Color::from_rgb8(18, 20, 26) })),
                            border: iced::Border { color: p_color, width: if is_selected { 2.0 } else { 1.5 }, radius: 6.0.into() },
                            ..Default::default()
                        });

                    pings_list = pings_list.push(card_btn);
                }

                let add_btn = button(
                    text("+ Add User / Word").size(scale_sz(0.72)).bold().color(theme.success).align_x(Alignment::Center)
                )
                .on_press(SettingsMessage::AddNewProfile)
                .padding([6, 12])
                .width(Length::Fill)
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgb8(15, 20, 15))),
                    border: iced::Border { color: theme.success, width: 1.5, radius: 6.0.into() },
                    ..Default::default()
                });

                pings_col = pings_col.push(scrollable(pings_list).height(Length::Fixed(560.0)));
                pings_col = pings_col.push(add_btn);

                let active_pill = button(text(if settings.notifications_enabled { "ON" } else { "OFF" }).size(scale_sz(0.7)).bold().color(Color::BLACK))
                    .on_press(SettingsMessage::ToggleNotifications)
                    .padding([4, 14])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(if settings.notifications_enabled { theme.success } else { theme.danger })),
                        border: iced::Border { color: if settings.notifications_enabled { theme.success } else { theme.danger }, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    });

                let test_btn = button(text("Test Notification").size(scale_sz(0.72)).bold().color(theme.text))
                    .on_press(SettingsMessage::TestNotification)
                    .padding([6, 16])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(Color::from_rgb8(20, 22, 28))),
                        border: iced::Border { color: theme.text, width: 1.5, radius: 6.0.into() },
                        ..Default::default()
                    });

                let screens = ["Auto (Primary)", "HDMI-A-2", "DP-2", "DP-1"];
                let mut scr_row = row![text("Screen:").size(scale_sz(0.78)).bold().color(theme.text)].spacing(6).align_y(Alignment::Center);
                for s in screens {
                    let is_s = self.selected_screen == s;
                    let s_str = s.to_string();
                    let scr_btn = button(text(s).size(scale_sz(0.65)).bold().color(if is_s { Color::BLACK } else { theme.text }))
                        .on_press(SettingsMessage::SelectNotifScreen(s_str))
                        .padding([3, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(if is_s { theme.text } else { Color::TRANSPARENT })),
                            border: iced::Border { color: if is_s { theme.text } else { theme.border }, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        });
                    scr_row = scr_row.push(scr_btn);
                }

                let corners = [("top-right", "Top-R"), ("top-left", "Top-L"), ("bottom-right", "Bot-R"), ("bottom-left", "Bot-L")];
                let mut corner_row = row![text("Corner:").size(scale_sz(0.78)).bold().color(theme.text)].spacing(6).align_y(Alignment::Center);
                for (c_id, c_lbl) in corners {
                    let is_c = settings.notif_corner == c_id;
                    let c_str = c_id.to_string();
                    let c_btn = button(text(c_lbl).size(scale_sz(0.65)).bold().color(if is_c { Color::BLACK } else { theme.text }))
                        .on_press(SettingsMessage::SelectNotifCorner(c_str))
                        .padding([3, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(if is_c { theme.text } else { Color::TRANSPARENT })),
                            border: iced::Border { color: if is_c { theme.text } else { theme.border }, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        });
                    corner_row = corner_row.push(c_btn);
                }

                let shapes = [("round", "Round"), ("hex", "Hex"), ("slant", "Slant"), ("flat", "Flat")];
                let mut shape_row = row![text("Shape:").size(scale_sz(0.78)).bold().color(theme.text)].spacing(6).align_y(Alignment::Center);
                for (s_id, s_lbl) in shapes {
                    let is_sh = settings.notif_shape == s_id;
                    let sh_str = s_id.to_string();
                    let sh_btn = button(text(s_lbl).size(scale_sz(0.65)).bold().color(if is_sh { Color::BLACK } else { theme.text }))
                        .on_press(SettingsMessage::SelectNotifShape(sh_str))
                        .padding([3, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(if is_sh { theme.text } else { Color::TRANSPARENT })),
                            border: iced::Border { color: if is_sh { theme.text } else { theme.border }, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        });
                    shape_row = shape_row.push(sh_btn);
                }

                let col_left_sliders = column![
                    column![
                        row![text("Hold Duration").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}s", settings.notif_hold_duration_sec)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(1..=20, settings.notif_hold_duration_sec as u32, |v| SettingsMessage::NotifHoldDuration(v as u64)).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Baseline Y Offset").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_baseline_y)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(20..=800, settings.notif_baseline_y, SettingsMessage::NotifBaselineY).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Card Stack Overlap").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_stack_overlap)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(-100..=100, settings.notif_stack_overlap, SettingsMessage::NotifStackOverlap).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Notification Card Height").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_card_height)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(90..=240, settings.notif_card_height, SettingsMessage::NotifCardHeight).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Drop Pause").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}ms", settings.notif_drop_pause_ms)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(0..=400, settings.notif_drop_pause_ms, SettingsMessage::NotifDropPause).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Summary Font Size").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_summary_size)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(10..=36, settings.notif_summary_size as u32, |v| SettingsMessage::NotifSummarySize(v as u16)).width(Length::Fill),
                    ].spacing(2),
                ].spacing(8).width(Length::FillPortion(1));

                let col_right_sliders = column![
                    column![
                        row![text("Sound Volume").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}%", settings.notif_volume)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(0..=100, settings.notif_volume, SettingsMessage::NotifVolume).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Margin X Offset").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_margin_x)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(0..=150, settings.notif_margin_x, SettingsMessage::NotifMarginX).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Notification Card Width").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_card_width)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(250..=650, settings.notif_card_width, SettingsMessage::NotifCardWidth).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Slide-In Time").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}ms", settings.notif_slide_in_ms)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(50..=600, settings.notif_slide_in_ms, SettingsMessage::NotifSlideIn).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Drop Travel Distance").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_drop_distance_px)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(10..=400, settings.notif_drop_distance_px, SettingsMessage::NotifDropDistance).width(Length::Fill),
                    ].spacing(2),
                    column![
                        row![text("Body Font Size").size(scale_sz(0.7)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.notif_body_size)).size(scale_sz(0.7)).bold().color(theme.text)],
                        slider(8..=32, settings.notif_body_size as u32, |v| SettingsMessage::NotifBodySize(v as u16)).width(Length::Fill),
                    ].spacing(2),
                ].spacing(8).width(Length::FillPortion(1));

                let profile_inspector: Element<'a, SettingsMessage> = if let Some(p) = settings.friend_profiles.get(self.selected_profile) {
                    let sel_idx = self.selected_profile;
                    let p_summary = p.summary.clone();
                    let p_sound = p.sound_path.clone().unwrap_or_default();
                    let p_tts = p.default_readout.clone().unwrap_or_else(|| "Message from {name}".into());

                    let colors = [
                        "#38FEDC", "#758593", "#8be9fd", "#ED54BA",
                        "#F5F557", "#132ED1", "#C51111", "#EF7D0D",
                        "#117F2D", "#D6E0F0", "#FFFFFF"
                    ];
                    let mut swatch_row = row![text("Tag Color:").size(scale_sz(0.72)).bold().color(theme.text)].spacing(6).align_y(Alignment::Center);
                    for c_hex in colors {
                        let c_val = hex_to_color(c_hex);
                        let hex_str = c_hex.to_string();
                        let is_active = p.color.eq_ignore_ascii_case(c_hex);
                        let s_btn = button(container(text("")).width(Length::Fixed(16.0)).height(Length::Fixed(16.0)).style(move |_t| container::Style {
                            background: Some(iced::Background::Color(c_val)),
                            border: iced::Border { color: if is_active { Color::WHITE } else { Color::TRANSPARENT }, width: if is_active { 2.0 } else { 0.0 }, radius: 8.0.into() },
                            ..Default::default()
                        }))
                        .on_press(SettingsMessage::ProfileColorChanged(sel_idx, hex_str))
                        .padding(2)
                        .style(|_t, _s| button::Style { background: None, ..Default::default() });

                        swatch_row = swatch_row.push(s_btn);
                    }

                    let mut rules_col = column![
                        row![
                            text(format!("BODY RULES ({})", p.content_rules.len())).size(scale_sz(0.72)).bold().color(theme.success),
                            row![].width(Length::Fill),
                            button(text("+ Add Body Rule").size(scale_sz(0.68)).bold().color(theme.success))
                                .on_press(SettingsMessage::AddContentRule(sel_idx))
                                .padding([3, 8])
                                .style(move |_t, _s| button::Style {
                                    background: Some(iced::Background::Color(Color::from_rgb8(20, 25, 20))),
                                    border: iced::Border { color: theme.success, width: 1.0, radius: 4.0.into() },
                                    ..Default::default()
                                }),
                        ].align_y(Alignment::Center),
                    ].spacing(6);

                    for (r_idx, r) in p.content_rules.iter().enumerate() {
                        let pat = r.pattern.clone();
                        let rd = r.readout.clone();
                        rules_col = rules_col.push(
                            row![
                                text("When contains:").size(scale_sz(0.65)).color(theme.muted),
                                text_input("pattern", &pat).on_input(move |v| SettingsMessage::RulePatternChanged(sel_idx, r_idx, v)).padding(4).size(scale_sz(0.68)).width(Length::Fixed(140.0)),
                                text("TTS:").size(scale_sz(0.65)).color(theme.muted),
                                text_input("readout", &rd).on_input(move |v| SettingsMessage::RuleReadoutChanged(sel_idx, r_idx, v)).padding(4).size(scale_sz(0.68)).width(Length::Fill),
                                button(text("✕").size(10).color(theme.danger))
                                    .on_press(SettingsMessage::DeleteContentRule(sel_idx, r_idx))
                                    .padding([3, 6])
                                    .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center)
                        );
                    }

                    column![
                        row![
                            text(format!("CONFIGURING PING: {}", p.summary.to_uppercase())).size(scale_sz(0.8)).bold().color(theme.success),
                            row![].width(Length::Fill),
                            button(text("Delete Rule").size(scale_sz(0.68)).bold().color(theme.danger))
                                .on_press(SettingsMessage::DeleteProfile(sel_idx))
                                .padding([3, 10])
                                .style(move |_t, _s| button::Style {
                                    background: Some(iced::Background::Color(Color::from_rgb8(30, 15, 15))),
                                    border: iced::Border { color: theme.danger, width: 1.0, radius: 4.0.into() },
                                    ..Default::default()
                                }),
                        ].align_y(Alignment::Center),

                        row![
                            text("Match Tag:").size(scale_sz(0.72)).bold().color(theme.text).width(Length::Fixed(100.0)),
                            text_input("Name/Tag", &p_summary)
                                .on_input(move |v| SettingsMessage::ProfileSummaryChanged(sel_idx, v))
                                .padding(5)
                                .size(scale_sz(0.72)),
                        ].align_y(Alignment::Center).spacing(8),

                        row![
                            text("Base Sound:").size(scale_sz(0.72)).bold().color(theme.text).width(Length::Fixed(100.0)),
                            text_input("Sound Path (opt)", &p_sound)
                                .on_input(move |v| SettingsMessage::ProfileSoundPathChanged(sel_idx, v))
                                .padding(5)
                                .size(scale_sz(0.72)),
                            button(text("✕").size(10).color(theme.danger))
                                .on_press(SettingsMessage::ProfileSoundPathChanged(sel_idx, "".into()))
                                .padding([4, 8])
                                .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                        ].align_y(Alignment::Center).spacing(8),

                        row![
                            text("Default TTS:").size(scale_sz(0.72)).bold().color(theme.text).width(Length::Fixed(100.0)),
                            text_input("Message from {name}", &p_tts)
                                .on_input(move |v| SettingsMessage::ProfileTtsTemplateChanged(sel_idx, v))
                                .padding(5)
                                .size(scale_sz(0.72)),
                        ].align_y(Alignment::Center).spacing(8),

                        swatch_row,
                        rules_col,
                    ].spacing(8).into()
                } else {
                    column![].into()
                };

                let controls_col = column![
                    row![
                        text("NOTIFICATION CONTROLS").size(scale_sz(0.95)).bold().color(theme.success),
                        row![].width(Length::Fill),
                        test_btn,
                    ].align_y(Alignment::Center),

                    row![
                        text("Notifications Active").size(scale_sz(0.8)).bold().color(theme.text),
                        row![].width(Length::Fill),
                        active_pill,
                    ].align_y(Alignment::Center),

                    scr_row,
                    corner_row,
                    shape_row,

                    row![col_left_sliders, col_right_sliders].spacing(16),

                    column![
                        text("Excluded Apps (comma separated):").size(scale_sz(0.72)).bold().color(theme.text),
                        text_input("greenclip, copyq", &settings.notif_excluded)
                            .on_input(SettingsMessage::NotifExcludedChanged)
                            .padding(6)
                            .size(scale_sz(0.75)),
                    ].spacing(4),

                    container(text("")).width(Length::Fill).height(Length::Fixed(1.0)).style(move |_t| container::Style {
                        background: Some(iced::Background::Color(theme.border)),
                        ..Default::default()
                    }),

                    profile_inspector,
                ].spacing(10).width(Length::Fill);

                let right_scrollable = scrollable(
                    container(controls_col).padding(Padding {
                        top: 0.0,
                        right: 28.0,
                        bottom: 12.0,
                        left: 0.0,
                    })
                )
                .height(Length::Fixed(card_h - 180.0))
                .width(Length::Fill);

                row![pings_col, right_scrollable].spacing(24).into()
            }

            SettingsTab::StylixSliders => column![
                column![
                    row![text("Top Bar Height").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.bar_height)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(30..=80, settings.bar_height, SettingsMessage::BarHeight).width(Length::Fill),
                ].spacing(3),
                column![
                    row![text("Bar Global Font Size").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.global_font_size)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(10..=24, settings.global_font_size as u32, |v| SettingsMessage::GlobalFontSize(v as u16)).width(Length::Fill),
                ].spacing(3),
                column![
                    row![text("Overlay Window Font Size").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.overlay_font_size)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(11..=32, settings.overlay_font_size as u32, |v| SettingsMessage::OverlayFontSize(v as u16)).width(Length::Fill),
                ].spacing(3),
                column![
                    row![text("Capsule Slant Width").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.slant_width as u32)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(4..=32, settings.slant_width as u32, SettingsMessage::SlantWidth).width(Length::Fill),
                ].spacing(3),
                column![
                    row![text("Capsule Spacing").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.capsule_spacing)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(-50..=40, settings.capsule_spacing, SettingsMessage::CapsuleSpacing).width(Length::Fill),
                ].spacing(3),
                column![
                    row![text("Window & Card Border Width").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.global_border_width as u32)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(1..=10, settings.global_border_width as u32, SettingsMessage::BorderWidth).width(Length::Fill),
                ].spacing(3),
                column![
                    row![text("Control / Button Border Width").size(scale_sz(0.8)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}px", settings.control_border_width as u32)).size(scale_sz(0.8)).bold().color(theme.text)],
                    slider(1..=8, settings.control_border_width as u32, SettingsMessage::ControlBorderWidth).width(Length::Fill),
                ].spacing(3),
            ].spacing(12).into(),

            SettingsTab::ColorsStyles => {
                let slots = [
                    ("base00", "Primary Background (base00)", &settings.palette.base00),
                    ("base01", "Surface Alternate (base01)", &settings.palette.base01),
                    ("base02", "Selection / Row Highlight (base02)", &settings.palette.base02),
                    ("base03", "Outer Bar & Card Border (base03)", &settings.palette.base03),
                    ("base05", "Primary Accent / Text (base05)", &settings.palette.base05),
                    ("base08", "Danger / Alert Red (base08)", &settings.palette.base08),
                    ("base09", "Warning / Orange (base09)", &settings.palette.base09),
                    ("base0c", "Success / Green (base0C)", &settings.palette.base0c),
                    ("base0d", "Focus / Blue Accent (base0D)", &settings.palette.base0d),
                ];

                let mut col = column![
                    text("🎨 PALETTE & SYSTEM ACCENT COLORS")
                        .size(scale_sz(0.95))
                        .bold()
                        .color(theme.success),
                    text("Click on color swatches to apply presets, or type any valid hex color:")
                        .size(scale_sz(0.72))
                        .color(theme.muted),
                ].spacing(10);

                let preset_colors = [
                    "#0f0f0f", "#003399", "#f7f700", "#ff5555",
                    "#04f100", "#675ddb", "#fabd2f", "#38fedc", "#ffffff"
                ];

                for (slot, name, current_hex) in slots {
                    let slot_clone = slot.to_string();
                    let swatch_col = hex_to_color(current_hex);

                    let swatch_box = container(text(""))
                        .width(Length::Fixed(24.0))
                        .height(Length::Fixed(24.0))
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(swatch_col)),
                            border: iced::Border { color: Color::WHITE, width: 1.5, radius: 4.0.into() },
                            ..Default::default()
                        });

                    let input = text_input("#ffffff", current_hex)
                        .on_input(move |h| SettingsMessage::ColorChanged(slot_clone.clone(), h))
                        .padding(5)
                        .size(scale_sz(0.72))
                        .width(Length::Fixed(110.0));

                    let mut swatches_row = row![swatch_box, input].spacing(8).align_y(Alignment::Center);

                    for p_hex in preset_colors {
                        let p_val = hex_to_color(p_hex);
                        let p_str = p_hex.to_string();
                        let s_clone = slot.to_string();
                        let p_btn = button(container(text("")).width(Length::Fixed(14.0)).height(Length::Fixed(14.0)).style(move |_t| container::Style {
                            background: Some(iced::Background::Color(p_val)),
                            border: iced::Border { radius: 3.0.into(), ..Default::default() },
                            ..Default::default()
                        }))
                        .on_press(SettingsMessage::ColorChanged(s_clone, p_str))
                        .padding(2)
                        .style(|_t, _s| button::Style { background: None, ..Default::default() });
                        swatches_row = swatches_row.push(p_btn);
                    }

                    col = col.push(
                        row![
                            swatches_row,
                            text(name).size(scale_sz(0.75)).bold().color(theme.text),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    );
                }

                col.into()
            }

            SettingsTab::GpuHardware => column![
                text("🎮 GPU ALERT THRESHOLDS & MONITORING").size(scale_sz(0.95)).bold().color(theme.success),

                column![
                    row![text("GPU 0 Temp Warning (°C)").size(scale_sz(0.72)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}°C", settings.gpu0.temp_warn)).size(scale_sz(0.72)).bold().color(theme.text)],
                    slider(40..=95, settings.gpu0.temp_warn, SettingsMessage::GpuTempWarn).width(Length::Fill),
                ].spacing(2),
                column![
                    row![text("GPU 0 Temp Danger (°C)").size(scale_sz(0.72)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}°C", settings.gpu0.temp_danger)).size(scale_sz(0.72)).bold().color(theme.text)],
                    slider(50..=105, settings.gpu0.temp_danger, SettingsMessage::GpuTempDanger).width(Length::Fill),
                ].spacing(2),
                column![
                    row![text("GPU 0 Free VRAM Warning (GiB)").size(scale_sz(0.72)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{} GiB", settings.gpu0.vram_warn)).size(scale_sz(0.72)).bold().color(theme.text)],
                    slider(1..=16, settings.gpu0.vram_warn, SettingsMessage::GpuVramWarn).width(Length::Fill),
                ].spacing(2),
                column![
                    row![text("GPU 0 Free VRAM Danger (GiB)").size(scale_sz(0.72)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{} GiB", settings.gpu0.vram_danger)).size(scale_sz(0.72)).bold().color(theme.text)],
                    slider(0..=8, settings.gpu0.vram_danger, SettingsMessage::GpuVramDanger).width(Length::Fill),
                ].spacing(2),
                column![
                    row![text("Hardware Polling Interval (ms)").size(scale_sz(0.72)).bold().color(theme.text), row![].width(Length::Fill), text(format!("{}ms", settings.hardware_poll_interval_ms)).size(scale_sz(0.72)).bold().color(theme.text)],
                    slider(500..=5000, settings.hardware_poll_interval_ms as u32, |v| SettingsMessage::HardwarePollInterval(v as u64)).width(Length::Fill),
                ].spacing(2),
            ]
            .spacing(12)
            .into(),

            SettingsTab::BarLayout => {
                let all_modules = [
                    "calendar", "music", "alarm", "weather", "unified", "notify",
                    "audio", "clock", "mic", "custom", "net", "cpu", "gpu", "ram", "battery", "tray"
                ];

                let slant_modes = [
                    ("symmetric", "Symmetric (\\ /)"),
                    ("all-left", "All Left (\\ \\)"),
                    ("all-right", "All Right (/ /)"),
                ];
                let mut slants_row = row![text("Global Slants:").size(scale_sz(0.78)).bold().color(theme.text)].spacing(8).align_y(Alignment::Center);
                for (sm_id, sm_label) in slant_modes {
                    let is_sm = settings.slant_mode == sm_id;
                    let sm_str = sm_id.to_string();
                    let sm_btn = button(text(sm_label).size(scale_sz(0.7)).bold().color(if is_sm { Color::BLACK } else { theme.text }))
                        .on_press(SettingsMessage::SelectSlantMode(sm_str))
                        .padding([4, 10])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(if is_sm { theme.text } else { Color::TRANSPARENT })),
                            border: iced::Border { color: theme.text, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        });
                    slants_row = slants_row.push(sm_btn);
                }

                let bar_shapes = ["slant", "hexagon", "rounded", "flat"];
                let mut shape_row = row![text("Bar Shape:").size(scale_sz(0.78)).bold().color(theme.text)].spacing(8).align_y(Alignment::Center);
                for bs in bar_shapes {
                    let is_bs = settings.bar_shape == bs;
                    let bs_str = bs.to_string();
                    let bs_btn = button(text(bs.to_uppercase()).size(scale_sz(0.7)).bold().color(if is_bs { Color::BLACK } else { theme.text }))
                        .on_press(SettingsMessage::SelectBarShape(bs_str))
                        .padding([4, 10])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(if is_bs { theme.text } else { Color::TRANSPARENT })),
                            border: iced::Border { color: theme.text, width: 1.0, radius: 4.0.into() },
                            ..Default::default()
                        });
                    shape_row = shape_row.push(bs_btn);
                }

                let mut col = column![
                    text("BAR GEOMETRY, CAPSULE SHAPES & LAYOUT")
                        .size(scale_sz(0.95))
                        .bold()
                        .color(theme.success),
                    slants_row,
                    shape_row,
                    container(text("")).width(Length::Fill).height(Length::Fixed(1.0)).style(move |_t| container::Style {
                        background: Some(iced::Background::Color(theme.border)),
                        ..Default::default()
                    }),
                    text("Toggle individual capsules on or off in the top bar:")
                        .size(scale_sz(0.7))
                        .color(theme.muted),
                ].spacing(10);

                for id in all_modules {
                    let is_vis = settings.visible_capsules.get(id).copied().unwrap_or(true);
                    let id_str = id.to_string();

                    let vis_btn = button(
                        text(if is_vis { "SHOW" } else { "HIDE" })
                            .size(10)
                            .bold()
                            .color(if is_vis { Color::BLACK } else { theme.text }),
                    )
                    .on_press(SettingsMessage::ToggleCapsuleVisible(id_str))
                    .padding([4, 14])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(if is_vis { theme.success } else { theme.danger })),
                        border: iced::Border { color: if is_vis { theme.success } else { theme.danger }, width: 1.0, radius: 4.0.into() },
                        ..Default::default()
                    });

                    col = col.push(
                        row![
                            text(id.to_uppercase()).size(scale_sz(0.75)).bold().color(theme.text).width(Length::Fixed(160.0)),
                            vis_btn,
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    );
                }

                scrollable(col).height(Length::Fixed(card_h - 220.0)).into()
            }

            SettingsTab::OverlaysWindows => {
                let windows = [
                    ("launcher", "🚀 App Launcher", 840, 650),
                    ("calc", "🧮 Calculator & Units", 820, 580),
                    ("clipboard", "📋 Clipboard Manager", 1080, 700),
                    ("dictionary", "📖 Dictionary", 820, 600),
                    ("unicode", "🔣 Unicode Search", 780, 600),
                    ("notes", "📝 Quick Notes", 840, 650),
                    ("pass", "🔑 Password Store", 820, 600),
                    ("power", "⚡ Power & Session", 720, 560),
                    ("todo", "✅ Todo Task Board", 860, 740),
                    ("settings", "⚙ Settings Menu", 1260, 840),
                    ("email", "✉ Email Client", 1500, 900),
                    ("reminders", "⏰ Reminders", 780, 580),
                    ("rng", "🎲 Dice / RNG", 620, 840),
                    ("amogus", "ඞ Among Us Tracker", 780, 560),
                ];

                let mut col = column![
                    text("🪟 OVERLAY WINDOW SIZING & GEOMETRY")
                        .size(base_sz)
                        .bold()
                        .color(theme.success),
                    text("Adjust global hexagon corner cuts, angles, and window dimensions:")
                        .size(scale_sz(0.72))
                        .color(theme.muted),
                ].spacing(8);

                col = col.push(
                    container(
                        column![
                            text("⬡ Global Overlay Hexagon Geometry").size(scale_sz(0.8)).bold().color(theme.success),
                            row![
                                text("Cut:").size(11).bold().color(theme.text),
                                slider(0..=200, settings.overlay_cut, SettingsMessage::OverlayCut).width(Length::Fixed(180.0)),
                                text(format!("{}px", settings.overlay_cut)).size(11).color(theme.text).width(Length::Fixed(50.0)),
                                text("Angle:").size(11).bold().color(theme.text),
                                slider(5..=85, settings.overlay_angle, SettingsMessage::OverlayAngle).width(Length::Fixed(180.0)),
                                text(format!("{}°", settings.overlay_angle)).size(11).color(theme.text).width(Length::Fixed(40.0)),
                                button(text("30°").size(11).bold().color(if settings.overlay_angle == 30 { theme.success } else { theme.text }))
                                    .on_press(SettingsMessage::OverlayAngle(30))
                                    .padding([2, 6])
                                    .style(move |_t, _s| button::Style {
                                        background: Some(iced::Background::Color(theme.surface_alt)),
                                        border: iced::Border {
                                            color: if settings.overlay_angle == 30 { theme.focus } else { theme.border },
                                            width: 1.0,
                                            radius: 4.0.into(),
                                        },
                                        ..Default::default()
                                    }),
                                button(text("45°").size(11).bold().color(if settings.overlay_angle == 45 { theme.success } else { theme.text }))
                                    .on_press(SettingsMessage::OverlayAngle(45))
                                    .padding([2, 6])
                                    .style(move |_t, _s| button::Style {
                                        background: Some(iced::Background::Color(theme.surface_alt)),
                                        border: iced::Border {
                                            color: if settings.overlay_angle == 45 { theme.focus } else { theme.border },
                                            width: 1.0,
                                            radius: 4.0.into(),
                                        },
                                        ..Default::default()
                                    }),
                                button(text("60°").size(11).bold().color(if settings.overlay_angle == 60 { theme.success } else { theme.text }))
                                    .on_press(SettingsMessage::OverlayAngle(60))
                                    .padding([2, 6])
                                    .style(move |_t, _s| button::Style {
                                        background: Some(iced::Background::Color(theme.surface_alt)),
                                        border: iced::Border {
                                            color: if settings.overlay_angle == 60 { theme.focus } else { theme.border },
                                            width: 1.0,
                                            radius: 4.0.into(),
                                        },
                                        ..Default::default()
                                    }),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        ].spacing(6)
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
                    })
                );

                for (id, name, def_w, def_h) in windows {
                    let (w_val, h_val) = settings.get_window_size(id, def_w, def_h);
                    let w_u32 = w_val as u32;
                    let h_u32 = h_val as u32;
                    let id_w = id.to_string();
                    let id_h = id.to_string();

                    col = col.push(
                        row![
                            text(name).size(scale_sz(0.7)).bold().color(theme.text).width(Length::Fixed(200.0)),
                            text("W:").size(10).color(theme.muted),
                            slider(400..=1600, w_u32, move |v| SettingsMessage::WindowWidth(id_w.clone(), v)).width(Length::Fixed(180.0)),
                            text(format!("{}px", w_u32)).size(10).color(theme.text).width(Length::Fixed(50.0)),
                            text("H:").size(10).color(theme.muted),
                            slider(250..=1000, h_u32, move |v| SettingsMessage::WindowHeight(id_h.clone(), v)).width(Length::Fixed(180.0)),
                            text(format!("{}px", h_u32)).size(10).color(theme.text).width(Length::Fixed(50.0)),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    );
                }

                scrollable(col).height(Length::Fixed(card_h - 180.0)).into()
            }
        };

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(card_w), Length::Fixed(card_h));

        let header_row = container(
            row![
                text("⚙ SYSTEM SETTINGS & PREFERENCES")
                    .size(scale_sz(1.15))
                    .bold()
                    .color(theme.text),
                row![].width(Length::Fill),
                button(text("✕").size(scale_sz(0.85)).bold().color(theme.danger))
                    .on_press(SettingsMessage::Dismiss)
                    .padding([4, 10])
                    .style(|_t, _s| button::Style { background: None, ..Default::default() }),
            ]
            .align_y(Alignment::Center)
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: header_inset,
            bottom: 0.0,
            left: header_inset,
        });

        let tab_container = container(tab_row)
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: tab_inset,
                bottom: 0.0,
                left: tab_inset,
            });

        let footer_container = container(
            text("ESC or [✕] to dismiss  •  Changes update and save live in real time")
                .size(scale_sz(0.65))
                .color(theme.muted)
                .align_x(Alignment::Center)
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

        let backdrop = container(column![]).width(Length::Fill).height(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.65))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
