use iced::widget::{button, container, text, tooltip, MouseArea, Row, Stack};
use std::sync::{Mutex, OnceLock};
use iced::{Alignment, Color, Element, Length};

use crate::backend::UnifiedTelemetry;
use crate::config::theme::{hex_to_color, ShellTheme};
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::style::shell_font;

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
    ToggleNotificationDnd,
    ToggleCustomMode,
    ToggleTrayExpanded,
    TrayActivate(crate::backend::TrayItem),
    TrayContextMenu(crate::backend::TrayItem),
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
                .size(size)
                .wrapping(iced::widget::text::Wrapping::None)
                .font(shell_font(iced::font::Weight::Bold))
                .color(*col),
        );
    }
    LABEL_W.with(|w| w.set(total_w));
    r.into()
}

static CAPSULE_WIDTHS: OnceLock<Mutex<std::collections::HashMap<String, f32>>> = OnceLock::new();

fn widths_map() -> &'static Mutex<std::collections::HashMap<String, f32>> {
    CAPSULE_WIDTHS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

pub fn record_width(id: &str, w: f32) {
    if let Ok(mut m) = widths_map().lock() {
        m.insert(id.to_string(), w);
    }
}

pub fn lookup_width(id: &str) -> Option<f32> {
    widths_map().lock().ok().and_then(|m| m.get(id).copied())
}

pub struct CapsuleRenderer;

impl CapsuleRenderer {
    pub fn render<'a, Message: 'static + Clone>(
        id: &str,
        telemetry: &'a UnifiedTelemetry,
        theme: &ShellTheme,
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
                        format!("⏰ {}h{}m", diff / 3600, (diff % 3600) / 60)
                    } else if diff >= 60 {
                        format!("⏰ {}m", diff / 60)
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
                            crate::backend::WeatherAlert::Danger(_) => ("🚨 ", theme.danger),
                            crate::backend::WeatherAlert::Upcoming(_) => ("⚠️ ", theme.warning),
                            crate::backend::WeatherAlert::None => ("", theme.text),
                        };
                        (format!("{}{}°F/{}°C", prefix, w.temp_f, w.temp_c), col)
                    }
                    None => ("55°F/13°C".into(), theme.text),
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
                let held_count = crate::overlays::notifications::held_notifs_count();
                let base08_col = hex_to_color(&theme.palette.base08);
                let text_col = if paused { base08_col } else { theme.text };
                let border_col = if paused { base08_col } else { theme.border };

                let elem = label::<Message>(&[
                    ("Notifications:".to_string(), text_col),
                    (format!(" {}", held_count), text_col),
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
                let border_col = if telemetry.cpu.temp_c >= 80 || telemetry.cpu.usage_pct >= 90 {
                    theme.danger
                } else if telemetry.cpu.temp_c >= 70 || telemetry.cpu.usage_pct >= 75 {
                    theme.warning
                } else {
                    theme.border
                };
                let elem = label::<Message>(&[
                    ("CPU:".to_string(), theme.success),
                    (" ".to_string(), theme.success),
                    (format!("{}% {}°C", telemetry.cpu.usage_pct, telemetry.cpu.temp_c), theme.text),
                ], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("cpu".into()), border_col)
            }
            "gpu" => {
                let temp_col = if telemetry.gpu.temp_c >= theme.gpu0_temp_danger { theme.danger } else if telemetry.gpu.temp_c >= theme.gpu0_temp_warn { theme.warning } else { theme.text };
                let vram_col = if telemetry.gpu.vram_free_gib <= theme.gpu0_vram_danger { theme.danger } else if telemetry.gpu.vram_free_gib <= theme.gpu0_vram_warn { theme.warning } else { theme.text };
                let name = if telemetry.gpu.name.is_empty() { "GPU".to_string() } else { telemetry.gpu.name.clone() };
                let elem = label::<Message>(&[
                    (format!("{}:", name), theme.success),
                    (" ".to_string(), theme.text),
                    (format!("{}%", telemetry.gpu.usage_pct), theme.text),
                    (" ".to_string(), theme.text),
                    (format!("{}°C", telemetry.gpu.temp_c), temp_col),
                    (" ".to_string(), theme.text),
                    (format!("{}W", telemetry.gpu.power_w), theme.text),
                    (" ".to_string(), theme.text),
                    (format!("{}GiB", telemetry.gpu.vram_free_gib), vram_col),
                ], theme.font_size_normal);
                let border_col = if telemetry.gpu.temp_c >= theme.gpu0_temp_danger || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_danger { theme.danger } else if telemetry.gpu.temp_c >= theme.gpu0_temp_warn || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_warn { theme.warning } else { theme.border };
                (elem, CapsuleAction::OpenOverlay("gpu".into()), border_col)
            }
            "ram" => {
                let (display, total) = if telemetry.ram.effective_avail_gib > 0.0 {
                    (telemetry.ram.effective_avail_gib, telemetry.ram.total_gib + telemetry.ram.zram_saved_gib)
                } else {
                    (telemetry.ram.available_gib, telemetry.ram.total_gib)
                };
                let usage_ratio = if total > 0.0 { ((total - display).max(0.0)) / total } else { 0.0 };
                let val_col = if usage_ratio >= 0.85 { theme.danger } else if usage_ratio >= 0.50 { theme.warning } else { theme.text };
                let val_str = if display == 0.0 { " -- GiB".to_string() } else { format!(" {:.1} GiB", display) };
                let elem = label::<Message>(&[
                    ("RAM:".to_string(), theme.success),
                    (val_str, val_col),
                ], theme.font_size_normal);
                let border_col = if usage_ratio >= 0.85 { theme.danger } else if usage_ratio >= 0.50 { theme.warning } else { theme.border };
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
        let min_width = text_w + theme.slant_width * 2.0 + 24.0;
        record_width(id, min_width);

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
            .size(Length::Fixed(min_width), Length::Fill);

        let button_content = container(content)
            .width(Length::Fixed(min_width))
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        button(Stack::new().push(background_shape.view()).push(button_content))
            .on_press(on_action(action))
            .padding(0)
            .width(Length::Fixed(min_width))
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .into()
    }

    pub fn render_tray_expanded<'a, Message: 'static + Clone>(
        items: &[crate::backend::TrayItem],
        theme: &ShellTheme,
        slant_left: SlantSide,
        slant_right: SlantSide,
        region: &str,
        on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
    ) -> Element<'a, Message> {
        let icon_size = 20.0_f32;
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
        let min_width = content_w + theme.slant_width * 2.0 + 32.0;

        let hover_bg = theme.surface_alt;
        let fg = theme.text;

        let close_btn = button(text("\u{2715}").size(14).color(theme.muted))
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
                iced::widget::image(iced::widget::image::Handle::from_path(p.clone()))
                    .width(Length::Fixed(icon_size))
                    .height(Length::Fixed(icon_size))
                    .into()
            } else {
                text("\u{25CF}").size(icon_size * 0.7).color(theme.success).into()
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
        let mut content = Row::new()
            .push(close_btn)
            .push(icons_row)
            .spacing(spacing)
            .align_y(Alignment::Center)
            .padding([0, pad_x]);
        if items.is_empty() {
            content = content.push(
                text("No tray apps — restart Steam/CoreCtrl/etc.")
                    .size(12)
                    .color(theme.muted),
            );
        }

        let shape_type = ShapeType::Slant { left: slant_left, right: slant_right };
        let _ = region; // region inferred from sl/sr now

        let background_shape = ShapeBox::new()
            .shape(shape_type)
            .background(theme.background)
            .border(theme.text, theme.control_border_width)
            .slant_width(theme.slant_width)
            .size(Length::Fixed(min_width), Length::Fill);

        Stack::new()
            .push(background_shape.view())
            .push(
                container(content)
                    .width(Length::Fixed(min_width))
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .into()
    }

}
