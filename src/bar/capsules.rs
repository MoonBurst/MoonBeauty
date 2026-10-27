use iced::widget::{button, container, text, tooltip, MouseArea, Row, Stack};
use std::sync::{Mutex, OnceLock};
use iced::{Alignment, Color, Element, Length};

use crate::libraries::backend::UnifiedTelemetry;
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::{hex_to_color, ShellTheme};
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::libraries::style::shell_font;

#[derive(Debug, Clone)]
pub enum CapsuleAction {
    OpenOverlay(String),
    TogglePlayPause,
    NextTrack,
    PreviousTrack,
    CycleAudioSink,
    ToggleSinkMute,
    ToggleSourceMute,
    CycleGpu,
    CycleGpuTarget,
    ToggleNotificationDnd,
    ToggleCustomMode,
    ToggleTrayExpanded,
    TrayActivate(crate::bar::tray::TrayItem),
    TrayContextMenu(crate::bar::tray::TrayItem),
    CancelTooltip(String),
}

thread_local! {
    static LABEL_W: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

fn est_text_width(txt: &str, size: u16) -> f32 {
    let em = size as f32;
    txt.chars()
        .map(|c| if c.is_ascii() { em * 0.6 } else { em * 1.3 })
        .sum()
}

fn label<'a, Message: 'a>(
    segments: &[(String, Color)],
    size: u16,
) -> Element<'a, Message> {
    let mut r: Row<'a, Message, iced::Theme, iced::Renderer> = Row::new();
    r = r.spacing(0).align_y(Alignment::Center);
    let mut total_w = 0.0;
    for (txt, col) in segments {
        total_w += est_text_width(txt, size);
        if txt.is_empty() {
            continue;
        }
        r = r.push(
            text(txt.clone())
                .size((size) as f32)
                .wrapping(iced::widget::text::Wrapping::None)
                .font(shell_font(iced::font::Weight::Bold))
                .color(*col),
        );
    }
    LABEL_W.with(|w| w.set(total_w));
    r.into()
}

static SVG_HANDLE_CACHE: OnceLock<Mutex<std::collections::HashMap<String, iced::widget::svg::Handle>>> =
    OnceLock::new();

fn cached_svg_handle(path: &str) -> iced::widget::svg::Handle {
    let cache = SVG_HANDLE_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut g = cache.lock().unwrap();
    if let Some(h) = g.get(path) {
        return h.clone();
    }
    let h = iced::widget::svg::Handle::from_path(path);
    g.insert(path.to_string(), h.clone());
    h
}

static CAPSULE_WIDTHS: OnceLock<Mutex<std::collections::HashMap<String, f32>>> = OnceLock::new();

fn widths_map() -> &'static Mutex<std::collections::HashMap<String, f32>> {
    CAPSULE_WIDTHS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

pub fn record_width(id: &str, w: f32) {
    if let Ok(mut m) = widths_map().lock() {
        let old = m.get(id).copied();
        if old != Some(w) {
            if std::env::var_os("MBS_DEBUG").is_some() {
                eprintln!("[capsule-width] {} = {:.1}px", id, w);
            }
        }
        m.insert(id.to_string(), w);
    }
}

pub fn lookup_width(id: &str) -> Option<f32> {
    widths_map().lock().ok().and_then(|m| m.get(id).copied())
}

/// Compute the visible width of the fully-expanded tray (excluding slants).
/// Matches the internal math in `render_tray_expanded` so the animator can
/// target the exact final width.
pub fn compute_expanded_visible_width(settings: &crate::libraries::config::settings::ShellSettings, item_count: usize) -> f32 {
    let shown = item_count.min(8);
    let icon_size = settings.tray_icon_size as f32;
    let icon_slot = icon_size + 8.0; // icon_size + icon_pad * 2
    let close_slot = 24.0;
    let spacing = 4.0;
    let empty_w = if item_count == 0 { 150.0 } else { 0.0 };
    let content_w = close_slot
        + if shown > 0 { spacing } else { 0.0 }
        + (icon_slot + spacing) * shown as f32
        + empty_w;
    content_w + 32.0
}

pub struct CapsuleRenderer;

impl CapsuleRenderer {
    pub fn render<'a, Message: 'static + Clone>(
        id: &str,
        telemetry: &'a UnifiedTelemetry,
        theme: &ShellTheme,
        settings: &ShellSettings,
        slant_left: SlantSide,
        slant_right: SlantSide,
        module_shape: &str,
        region: &str,
        on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
    ) -> Element<'a, Message> {
        LABEL_W.with(|w| w.set(0.0));
        let (content, action, border_color): (
            Element<'a, Message>,
            CapsuleAction,
            iced::Color,
        ) = match id {
            "calendar" => {
                let date_str = chrono::Local::now().format("%a, %b %d").to_string();
                let elem = label::<Message>(&[(date_str, theme.text)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("calendar".into()), theme.border)
            }
            "music" => {
                let display = match &telemetry.mpris {
                    Some(m) if !m.title.is_empty() && !m.artist.is_empty() => {
                        format!("{} - {}", m.artist, m.title)
                    }
                    Some(m) if !m.title.is_empty() => m.title.clone(),
                    Some(m) if !m.artist.is_empty() => m.artist.clone(),
                    _ => "No Track".into(),
                };
                let truncated: String = if display.chars().count() > 20 {
                    let s: String = display.chars().take(20).collect();
                    format!("{}…", s.trim())
                } else {
                    display
                };
                let elem = label::<Message>(&[(truncated, theme.text)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("music".into()), theme.border)
            }
            "alarm" => {
                let entries = crate::overlays::reminders::RemindersOverlay::load();
                let now = chrono::Local::now().timestamp();
                let active: Vec<_> = entries.iter().filter(|e| e.target_epoch > now).collect();
                let (txt, col) = if let Some(first) = active.iter().min_by_key(|e| e.target_epoch) {
                    let diff = (first.target_epoch - now).max(0);
                    let label = if diff >= 3600 {
                        format!("⏰ {}h {}m", diff / 3600, (diff % 3600) / 60)
                    } else if diff >= 60 {
                        format!("⏰ {}m {}s", diff / 60, diff % 60)
                    } else {
                        format!("⏰ {}s", diff)
                    };
                    (label, theme.warning)
                } else {
                    ("No Alarm".to_string(), theme.text)
                };
                let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("alarm".into()), theme.border)
            }
            "weather" => {
                let (txt, col) = match &telemetry.weather {
                    Some(w) => {
                        let (prefix, col) = match &w.alert {
                            crate::bar::weather::WeatherAlert::Danger(_) => ("🚨 ", theme.danger),
                            crate::bar::weather::WeatherAlert::Upcoming(_) => ("⚠️ ", theme.warning),
                            crate::bar::weather::WeatherAlert::None => ("", theme.text),
                        };
                        (format!("{}{}°F/{}°C", prefix, w.temp_f, w.temp_c), col)
                    }
                    None => ("--°F/--°C".into(), theme.text),
                };
                let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("weather".into()), col)
            }
            "unified" => {
                let (txt, col) = if telemetry.podman.is_rate_limited {
                    ("Miner: Limited!".to_string(), theme.warning)
                } else if telemetry.borg.is_active && !telemetry.borg.state_label.is_empty() {
                    (telemetry.borg.state_label.clone(), theme.success)
                } else if let Some(w) = telemetry.podman.watchers.iter().find(|w| w.is_running && !w.streamer.is_empty()) {
                    (format!("📺 {}", w.streamer), theme.success)
                } else if !telemetry.podman.summary_label.is_empty() {
                    (telemetry.podman.summary_label.clone(), theme.success)
                } else {
                    ("Sys OK".to_string(), theme.success)
                };
                let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("unified".into()), theme.border)
            }
            "notify" | "notifications" => {
                let paused = crate::overlays::notifications::dnd_active();
                // Held covers DND-held notifications (in the held file).
                // Active covers currently-visible toasts + queued toasts
                // (published by the notif-surface process). The capsule
                // shows the sum, so unpausing doesn't drop to 0 while
                // notifications are still on screen.
                let held_count = crate::overlays::notifications::held_notifs_count();
                let active_count = crate::overlays::notifications::read_active_count();
                let total_count = held_count + active_count;
                let base08_col = hex_to_color(&theme.palette.base08);
                let text_col = if paused { base08_col } else { theme.text };
                let border_col = if paused { base08_col } else { theme.border };

                let elem = label::<Message>(&[
                    ("Notifications:".to_string(), text_col),
                    (format!(" {}", total_count), text_col),
                ], theme.font_size_normal);
                (elem, CapsuleAction::ToggleNotificationDnd, border_col)
            }
            "clock" => {
                let t = telemetry.bar_time.clone();
                let elem = label::<Message>(&[(t, theme.text)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("clock".into()), theme.border)
            }
            "audio" => {
                let val_col = if telemetry.audio.is_muted { theme.danger } else { theme.text };
                let val_str = if telemetry.audio.is_muted { "Mute".to_string() } else { format!("{}%", telemetry.audio.volume_pct) };
                let elem = label::<Message>(&[
                    ("Audio:".to_string(), theme.success),
                    (" ".to_string(), theme.success),
                    (val_str, val_col),
                ], theme.font_size_normal);
                let border = if telemetry.audio.is_muted { theme.danger } else { theme.border };
                (elem, CapsuleAction::ToggleSinkMute, border)
            }
            "mic" => {
                let muted = telemetry.mic.is_muted;
                let val_col = if muted { theme.danger } else { theme.text };
                let val_str = if muted { "MUTED".to_string() } else { format!("{}%", telemetry.mic.volume_pct) };
                let elem = label::<Message>(&[
                    ("Mic:".to_string(), theme.success),
                    (" ".to_string(), theme.success),
                    (val_str, val_col),
                ], theme.font_size_normal);
                let border = if muted { theme.danger } else { theme.border };
                (elem, CapsuleAction::ToggleSourceMute, border)
            }
            "net" => {
                let down = if telemetry.net.down_str.is_empty() { "0.0Mb" } else { &telemetry.net.down_str };
                let up = if telemetry.net.up_str.is_empty() { "0.0Mb" } else { &telemetry.net.up_str };
                let ping = if telemetry.net.ping_str.is_empty() { "--" } else { &telemetry.net.ping_str };
                let ping_col = if ping.contains("OFFLIN") { theme.danger } else { theme.text };
                let elem = label::<Message>(&[
                    ("NET:".to_string(), theme.success),
                    (" ▼".to_string(), theme.text),
                    (down.to_string(), theme.text),
                    (" ▲".to_string(), theme.text),
                    (up.to_string(), theme.text),
                    (" ".to_string(), theme.text),
                    (ping.to_string(), ping_col),
                ], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("net".into()), theme.border)
            }
            "cpu" => {
                let cpu_danger = telemetry.cpu.temp_c >= theme.cpu_temp_danger || u32::from(telemetry.cpu.usage_pct) >= theme.cpu_usage_danger;
                let cpu_warn = telemetry.cpu.temp_c >= theme.cpu_temp_warn || u32::from(telemetry.cpu.usage_pct) >= theme.cpu_usage_warn;
                let val_col = if cpu_danger { theme.danger } else if cpu_warn { theme.warning } else { theme.text };
                let border_col = if val_col == theme.text { theme.border } else { val_col };
                // Zero-pad usage to 3 digits so the capsule width is stable
                // across 9% → 10% → 100%. Padding zeros are drawn in the
                // capsule background color (invisible) but still count toward
                // `est_text_width`, which is what drives `bounding_w`.
                let trimmed = telemetry.cpu.usage_pct.to_string();
                let leading_zero_count = 3usize.saturating_sub(trimmed.len());
                let zero_pad: String = "0".repeat(leading_zero_count);
                let mut segments: Vec<(String, Color)> = vec![
                    ("CPU:".to_string(), theme.success),
                    (" ".to_string(), theme.success),
                ];
                if !zero_pad.is_empty() {
                    segments.push((zero_pad, theme.background));
                }
                segments.push((format!("{}% {}°C", trimmed, telemetry.cpu.temp_c), val_col));
                let elem = label::<Message>(&segments, theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("cpu".into()), border_col)
            }
            "gpu" => {
                let (tw, td, vw, vd) = if telemetry.gpu_selected_idx == 0 {
                    (theme.gpu0_temp_warn, theme.gpu0_temp_danger, theme.gpu0_vram_warn, theme.gpu0_vram_danger)
                } else {
                    (settings.gpu1.temp_warn, settings.gpu1.temp_danger, settings.gpu1.vram_warn, settings.gpu1.vram_danger)
                };
                let temp_col = if telemetry.gpu.temp_c >= td { theme.danger } else if telemetry.gpu.temp_c >= tw { theme.warning } else { theme.text };
                let vram_col = if telemetry.gpu.vram_free_gib <= vd { theme.danger } else if telemetry.gpu.vram_free_gib <= vw { theme.warning } else { theme.text };
                let name = if telemetry.gpu.name.is_empty() { "GPU".to_string() } else { telemetry.gpu.name.clone() };
                let usage_danger = telemetry.gpu.usage_pct >= 90;
                let usage_warn = telemetry.gpu.usage_pct >= 75;
                let usage_col = if usage_danger { theme.danger } else if usage_warn { theme.warning } else { theme.text };

                let val_col = if telemetry.gpu.temp_c >= td || telemetry.gpu.vram_free_gib <= vd || usage_danger {
                    theme.danger
                } else if telemetry.gpu.temp_c >= tw || telemetry.gpu.vram_free_gib <= vw || usage_warn {
                    theme.warning
                } else {
                    theme.text
                };

                // Zero-pad numeric fields so the capsule width is stable
                // across value transitions (e.g. 9% -> 10%, 99% -> 100%,
                // 8W -> 12W -> 100W, 4GiB -> 12GiB -> 24GiB). The padding
                // zeros are drawn in the capsule background color (visually
                // invisible) but still count toward `est_text_width`, which
                // is what drives the capsule's bounding box.
                let mut segments: Vec<(String, Color)> = vec![
                    (format!("{}:", name), theme.success),
                ];
                let push_num = |segments: &mut Vec<(String, Color)>,
                                val: u32,
                                width: usize,
                                suffix: &str,
                                col: Color| {
                    let s = val.to_string();
                    let pad_count = width.saturating_sub(s.len());
                    segments.push((" ".to_string(), val_col));
                    if pad_count > 0 {
                        segments.push(("0".repeat(pad_count), theme.background));
                    }
                    segments.push((format!("{}{}", s, suffix), col));
                };
                push_num(&mut segments, telemetry.gpu.usage_pct as u32, 3, "%", usage_col);
                push_num(&mut segments, telemetry.gpu.temp_c, 3, "°C", temp_col);
                push_num(&mut segments, telemetry.gpu.power_w, 3, "W", val_col);
                push_num(&mut segments, telemetry.gpu.vram_free_gib, 2, "GiB", vram_col);
                let elem = label::<Message>(&segments, theme.font_size_normal);
                let border_col = if val_col == theme.text { theme.border } else { val_col };
                (elem, CapsuleAction::OpenOverlay("gpu".into()), border_col)
            }
            "ram" => {
                let (display, _total) = if telemetry.ram.effective_avail_gib > 0.0 {
                    (telemetry.ram.effective_avail_gib, telemetry.ram.total_gib + telemetry.ram.zram_saved_gib)
                } else {
                    (telemetry.ram.available_gib, telemetry.ram.total_gib)
                };
                let warn_gib = theme.ram_warn_gib as f32;
                let danger_gib = theme.ram_danger_gib as f32;
                let val_col = if display <= danger_gib { theme.danger } else if display <= warn_gib { theme.warning } else { theme.text };
                // Pad integer part to 2 digits with background-colored zeros:
                //   " 9.9 GiB"  →  " 09.9 GiB"
                //   "16.0 GiB"  →  " 16.0 GiB"
                // Both are the same visual width. The leading padding zero is
                // invisible against the capsule background but stabilizes the
                // bounding box across the 9.9 → 10.0 transition.
                let mut segments: Vec<(String, Color)> = vec![
                    ("RAM:".to_string(), theme.success),
                ];
                if display == 0.0 {
                    segments.push((" -- GiB".to_string(), val_col));
                } else {
                    let num_str = format!("{:.1}", display);
                    let (int_part, frac_part) = num_str
                        .split_once('.')
                        .unwrap_or((num_str.as_str(), ""));
                    let pad_count = 2usize.saturating_sub(int_part.len());
                    segments.push((" ".to_string(), val_col));
                    if pad_count > 0 {
                        segments.push(("0".repeat(pad_count), theme.background));
                    }
                    segments.push((format!("{}.{} GiB", int_part, frac_part), val_col));
                }
                let elem = label::<Message>(&segments, theme.font_size_normal);
                let border_col = if display <= danger_gib { theme.danger } else if display <= warn_gib { theme.warning } else { theme.border };
                (elem, CapsuleAction::OpenOverlay("ram".into()), border_col)
            }
            "battery" => {
                if !telemetry.battery.has_battery {
                    let elem = label::<Message>(&[
                        ("⚡ AC:".to_string(), theme.text),
                        (" Online".to_string(), theme.success),
                    ], theme.font_size_normal);
                    (elem, CapsuleAction::OpenOverlay("power".into()), theme.border)
                } else {
                    let pct = telemetry.battery.percent;
                    let status = telemetry.battery.status.to_lowercase();
                    let icon = if status.contains("charging") { "⚡ " } else if pct <= 20 { "🪫 " } else { "🔋 " };
                    let val_col = if status.contains("charging") || status.contains("full") || status.contains("mains") { theme.success } else if pct <= 20 { theme.danger } else if pct <= 40 { theme.warning } else { theme.text };
                    let elem = label::<Message>(&[
                        (format!("{}BAT:", icon), theme.text),
                        (format!(" {}% ({:.1}W)", pct, telemetry.battery.power_w), val_col),
                    ], theme.font_size_normal);
                    (elem, CapsuleAction::OpenOverlay("power".into()), val_col)
                }
            }
            "custom" => {
                let col = theme.text;
                let elem = label::<Message>(&[("Custom".to_string(), col)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("custom".into()), col)
            }
            "tray" => {
                let arrow = match region {
                    "left"  => "\u{25B6}",
                    "right" => "\u{25C0}",
                    _       => "\u{25BC}",
                };
                let elem = label::<Message>(&[(arrow.to_string(), theme.text)], theme.font_size_normal);
                (elem, CapsuleAction::ToggleTrayExpanded, theme.text)
            }
            _ => {
                let elem = label::<Message>(&[(id.to_uppercase(), theme.text)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay(id.to_string()), theme.border)
            }
        };

        let text_w = LABEL_W.with(|w| w.get());
        let bounding_w = if id == "tray" {
            // tray_width means the VISIBLE parallelogram width.
            // The ShapeBox::Slant clips `slant_width` off each side, so
            // the bounding box must be wider by 2*slant_width.
            settings.tray_width as f32 + theme.slant_width * 2.0
        } else {
            text_w + theme.slant_width * 2.0 + 24.0
        };
        record_width(id, bounding_w);

        let border_color = if border_color == theme.border { theme.text } else { border_color };

        let shape_type = match module_shape {
            "hexagon" => ShapeType::Hexagon { cut: (theme.slant_width * 1.5) as u16 },
            "rounded" => ShapeType::Rounded(((theme.slant_width * 1.5) as u16).max(4)),
            "flat" => ShapeType::Flat,
            _ => ShapeType::Slant { left: slant_left, right: slant_right },
        };

        let background_shape = ShapeBox::new()
            .shape(shape_type)
            .background(theme.background)
            .border(border_color, theme.control_border_width)
            .slant_width(theme.slant_width)
            .size(Length::Fixed(bounding_w), Length::Fill);

        let button_content = container(content)
            .width(Length::Fixed(bounding_w))
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let action_btn = button(Stack::new().push(background_shape.view()).push(button_content))
            .on_press(on_action(action))
            .padding(0)
            .width(Length::Fixed(bounding_w))
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            });

        if id == "alarm" {
            let id_str = id.to_string();
            MouseArea::new(action_btn)
                .on_right_press(on_action(CapsuleAction::CancelTooltip(id_str)))
                .into()
        } else if id == "gpu" {
            MouseArea::new(action_btn)
                .on_right_press(on_action(CapsuleAction::CycleGpuTarget))
                .into()
        } else {
            action_btn.into()
        }
    }

    pub fn render_tray_expanded<'a, Message: 'static + Clone>(
        items: &[crate::bar::tray::TrayItem],
        theme: &ShellTheme,
        settings: &crate::libraries::config::settings::ShellSettings,
        slant_left: SlantSide,
        slant_right: SlantSide,
        region: &str,
        display_width: f32,
        on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
    ) -> Element<'a, Message> {
        let icon_size = settings.tray_icon_size as f32;
        let icon_pad = 4.0_f32;
        let icon_slot = icon_size + icon_pad * 2.0;
        let close_slot = 24.0_f32;
        let spacing = 4.0_f32;

        let shown = items.len().min(8);
        let empty_w = if items.is_empty() { 150.0 } else { 0.0 };
        let content_w = close_slot
            + if shown > 0 { spacing } else { 0.0 }
            + (icon_slot + spacing) * shown as f32
            + empty_w;
        let _full_min_width = content_w + theme.slant_width * 2.0 + 32.0;
        // Use the animated display_width passed by caller, add slants for bounding box
        let bounding_w = display_width + theme.slant_width * 2.0;
        let show_icons = display_width > settings.tray_width as f32 + 3.0;

        let hover_bg = theme.surface_alt;
        let fg = theme.text;

        let close_arrow = match region {
            "left"  => "◀",
            "right" => "▶",
            _       => "▲",
        };
        let close_btn = button(text(close_arrow).size(16).color(theme.text))
            .on_press(on_action(CapsuleAction::ToggleTrayExpanded))
            .padding(0)
            .width(Length::Fixed(close_slot))
            .height(Length::Fixed(close_slot))
            .style(move |_t, status| {
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed =>
                        Some(iced::Background::Color(hover_bg)),
                    _ => None,
                };
                button::Style {
                    background: bg,
                    border: iced::Border { radius: 4.0.into(), ..Default::default() },
                    text_color: fg,
                    ..Default::default()
                }
            });

        let mut icons_row = Row::new().spacing(spacing).align_y(Alignment::Center);
        for item in items.iter().take(shown) {
            let icon_elem: Element<'a, Message> = if let Some(p) = &item.icon_path {
                let exists = std::path::Path::new(p).exists();
                if exists && p.ends_with(".svg") {
                    iced::widget::svg(cached_svg_handle(p))
                        .width(Length::Fixed(icon_size))
                        .height(Length::Fixed(icon_size))
                        .into()
                } else if exists {
                    iced::widget::image(iced::widget::image::Handle::from_path(p.clone()))
                        .width(Length::Fixed(icon_size))
                        .height(Length::Fixed(icon_size))
                        .into()
                } else {
                    let initial = item.title.chars().next().unwrap_or('?').to_uppercase().to_string();
                    text(initial).size((icon_size * 0.7) as f32).color(theme.text).into()
                }
            } else {
                let initial = item.title.chars().next().unwrap_or('?').to_uppercase().to_string();
                text(initial).size((icon_size * 0.7) as f32).color(theme.text).into()
            };

            let activate_item = item.clone();
            let ctx_item = item.clone();

            let icon_box = container(icon_elem)
                .padding(icon_pad)
                .center_x(Length::Fixed(icon_slot))
                .center_y(Length::Fixed(icon_slot));

            let clickable = MouseArea::new(icon_box)
                .on_press(on_action(CapsuleAction::TrayActivate(activate_item)))
                .on_right_press(on_action(CapsuleAction::TrayContextMenu(ctx_item)));

            let interactive: Element<'a, Message> = if !item.tooltip.is_empty() {
                let tt_text = item.tooltip.clone();
                let tt_bg = theme.surface_alt;
                let tt_fg = theme.text;
                let tt_border = theme.border;
                tooltip(
                    clickable,
                    container(text(tt_text).size(11).color(tt_fg))
                        .padding(6)
                        .style(move |_t| container::Style {
                            background: Some(iced::Background::Color(tt_bg)),
                            border: iced::Border {
                                color: tt_border,
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                    tooltip::Position::Bottom,
                )
                .into()
            } else {
                clickable.into()
            };

            let _ = hover_bg; // kept for signature compat; tooltip handles hover styling
            icons_row = icons_row.push(interactive);
        }

        let pad_x = theme.slant_width.round() as u16;
        let mut content = if region == "right" {
            // Icons left, close right
            let mut r = Row::new().spacing(spacing).align_y(Alignment::Center);
            if show_icons {
                r = r.push(icons_row);
            }
            r = r.push(close_btn);
            r
        } else {
            // Close left, icons right
            let mut r = Row::new().spacing(spacing).align_y(Alignment::Center);
            r = r.push(close_btn);
            if show_icons {
                r = r.push(icons_row);
            }
            r
        }
        .padding([0, pad_x]);
        if items.is_empty() {
            content = content.push(
                container(text("Empty").size(11).color(theme.muted))
                    .width(Length::Fixed(bounding_w - pad_x as f32 * 2.0))
                    .center_x(Length::Fill)
                    .clip(true)
            );
        }

        let shape_type = ShapeType::Slant { left: slant_left, right: slant_right };
        let _ = region; // region inferred from sl/sr now

        let background_shape = ShapeBox::new()
            .shape(shape_type)
            .background(theme.background)
            .border(theme.text, theme.control_border_width)
            .slant_width(theme.slant_width)
            .size(Length::Fixed(bounding_w), Length::Fill);

        container(Stack::new()
            .push(background_shape.view())
            .push(
                container(content)
                    .width(Length::Fixed(bounding_w))
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            ))
            .width(Length::Fixed(bounding_w))
            .clip(true)
            .into()
    }

}
