use iced::widget::{button, container, text, Row, Stack};
use iced::{Alignment, Color, Element, Length};

use crate::backend::UnifiedTelemetry;
use crate::config::theme::ShellTheme;
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
}

/// Multi-coloured inline label: a `Row` of individually-styled `Text` widgets.
/// Iced 0.13 has no `rich_text`, and `Row::spacing(0)` + explicit space spans
/// reproduces the QML `<font color>` layout closely enough at bar size.
fn label<'a, Message: 'a>(
    segments: &[(String, Color)],
    size: u16,
) -> Element<'a, Message> {
    let mut r: Row<'a, Message, iced::Theme, iced::Renderer> = Row::new();
    r = r.spacing(0).align_y(Alignment::Center);
    for (txt, col) in segments {
        LABEL_W.with(|w| w.set(w.get() + est_text_width(txt, size)));
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
    r.into()
}

thread_local! {
    /// Estimated pixel width of the label most recently built by `label()`.
    static LABEL_W: std::cell::Cell<f32> = std::cell::Cell::new(0.0);
}

fn est_text_width(txt: &str, size: u16) -> f32 {
    let em = size as f32;
    txt.chars()
        .map(|c| if c.is_ascii() { em * 0.5 } else { em * 1.3 })
        .sum()
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
        on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
    ) -> Element<'a, Message> {
        LABEL_W.with(|w| w.set(0.0));
        let (content, action, border_color, min_width): (
            Element<'a, Message>,
            CapsuleAction,
            iced::Color,
            f32,
        ) = match id {
            // ── calendar ────────────────────────────────────────────────
            "calendar" => {
                let date_str = chrono::Local::now().format("%a, %b %d").to_string();
                let elem = label::<Message>(&[(date_str, theme.text)], theme.font_size_normal);
                (
                    elem,
                    CapsuleAction::OpenOverlay("calendar".into()),
                    theme.border,
                    110.0,
                )
            }

            // ── music ───────────────────────────────────────────────────
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
                (elem, CapsuleAction::TogglePlayPause, theme.border, 140.0)
            }

            // ── alarm ───────────────────────────────────────────────────
            "alarm" => {
                let elem = label::<Message>(
                    &[("No Alarm".to_string(), theme.text)],
                    theme.font_size_normal,
                );
                (
                    elem,
                    CapsuleAction::OpenOverlay("reminders".into()),
                    theme.border,
                    90.0,
                )
            }

            // ── weather ─────────────────────────────────────────────────
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
                (elem, CapsuleAction::OpenOverlay("weather".into()), col, 95.0)
            }

            // ── unified ─────────────────────────────────────────────────
            "unified" => {
                let (txt, col) = if telemetry.podman.is_rate_limited {
                    ("Miner: Limited!".to_string(), theme.warning)
                } else if telemetry.borg.is_active && !telemetry.borg.state_label.is_empty() {
                    (telemetry.borg.state_label.clone(), theme.success)
                } else if let Some(w) = telemetry
                    .podman
                    .watchers
                    .iter()
                    .find(|w| w.is_running && !w.streamer.is_empty())
                {
                    (format!("📺 {}", w.streamer), theme.success)
                } else if !telemetry.podman.summary_label.is_empty() {
                    (telemetry.podman.summary_label.clone(), theme.success)
                } else {
                    ("Sys OK".to_string(), theme.success)
                };
                let elem = label::<Message>(&[(txt, col)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("unified".into()), theme.border, 80.0)
            }

            // ── notify ──────────────────────────────────────────────────
            "notify" => {
                // Click pauses/resumes toast popups (history still records them).
                let paused = crate::overlays::notifications::dnd_active();
                let col = if paused { theme.warning } else { theme.text };
                let elem = label::<Message>(
                    &[
                        ("Notifications:".to_string(), col),
                        ((if paused { " Paused" } else { " 0" }).to_string(), col),
                    ],
                    theme.font_size_normal,
                );
                (elem, CapsuleAction::ToggleNotificationDnd, col, 120.0)
            }

            // ── clock ───────────────────────────────────────────────────
            "clock" => {
                let t = telemetry.bar_time.clone();
                let elem = label::<Message>(&[(t, theme.text)], theme.font_size_normal);
                (elem, CapsuleAction::OpenOverlay("clock".into()), theme.border, 85.0)
            }

            // ── audio ───────────────────────────────────────────────────
            "audio" => {
                let val_col = if telemetry.audio.is_muted {
                    theme.danger
                } else {
                    theme.text
                };
                let val_str = if telemetry.audio.is_muted {
                    "Mute".to_string()
                } else {
                    format!("{}%", telemetry.audio.volume_pct)
                };
                let elem = label::<Message>(
                    &[
                        ("Audio:".to_string(), theme.success),
                        (" ".to_string(), theme.success),
                        (val_str, val_col),
                    ],
                    theme.font_size_normal,
                );
                let border = if telemetry.audio.is_muted {
                    theme.danger
                } else {
                    theme.border
                };
                (elem, CapsuleAction::ToggleSinkMute, border, 95.0)
            }

            // ── mic ─────────────────────────────────────────────────────
            "mic" => {
                let muted = telemetry.mic.is_muted;
                let val_col = if muted { theme.danger } else { theme.text };
                let val_str = if muted {
                    "MUTED".to_string()
                } else {
                    format!("{}%", telemetry.mic.volume_pct)
                };
                let elem = label::<Message>(
                    &[
                        ("Mic:".to_string(), theme.success),
                        (" ".to_string(), theme.success),
                        (val_str, val_col),
                    ],
                    theme.font_size_normal,
                );
                let border = if muted { theme.danger } else { theme.border };
                (elem, CapsuleAction::ToggleSourceMute, border, 80.0)
            }

            // ── net ─────────────────────────────────────────────────────
            "net" => {
                let down = if telemetry.net.down_str.is_empty() {
                    "0.0Mb"
                } else {
                    &telemetry.net.down_str
                };
                let up = if telemetry.net.up_str.is_empty() {
                    "0.0Mb"
                } else {
                    &telemetry.net.up_str
                };
                let ping = if telemetry.net.ping_str.is_empty() {
                    "--"
                } else {
                    &telemetry.net.ping_str
                };
                let ping_col = if ping.contains("OFFLIN") {
                    theme.danger
                } else {
                    theme.text
                };
                let elem = label::<Message>(
                    &[
                        ("NET:".to_string(), theme.success),
                        (" ▼".to_string(), theme.text),
                        (down.to_string(), theme.text),
                        (" ▲".to_string(), theme.text),
                        (up.to_string(), theme.text),
                        (" ".to_string(), theme.text),
                        (ping.to_string(), ping_col),
                    ],
                    theme.font_size_normal,
                );
                (
                    elem,
                    CapsuleAction::OpenOverlay("net".into()),
                    theme.border,
                    145.0,
                )
            }

            // ── cpu ─────────────────────────────────────────────────────
            "cpu" => {
                let elem = label::<Message>(
                    &[
                        ("CPU:".to_string(), theme.success),
                        (" ".to_string(), theme.success),
                        (
                            format!("{}% {}°C", telemetry.cpu.usage_pct, telemetry.cpu.temp_c),
                            theme.text,
                        ),
                    ],
                    theme.font_size_normal,
                );
                (
                    elem,
                    CapsuleAction::OpenOverlay("cpu".into()),
                    theme.border,
                    115.0,
                )
            }

            // ── gpu ─────────────────────────────────────────────────────
            "gpu" => {
                let temp_col = if telemetry.gpu.temp_c >= theme.gpu0_temp_danger {
                    theme.danger
                } else if telemetry.gpu.temp_c >= theme.gpu0_temp_warn {
                    theme.warning
                } else {
                    theme.text
                };
                let vram_col = if telemetry.gpu.vram_free_gib <= theme.gpu0_vram_danger {
                    theme.danger
                } else if telemetry.gpu.vram_free_gib <= theme.gpu0_vram_warn {
                    theme.warning
                } else {
                    theme.text
                };
                let name = if telemetry.gpu.name.is_empty() {
                    "GPU".to_string()
                } else {
                    telemetry.gpu.name.clone()
                };
                let elem = label::<Message>(
                    &[
                        (format!("{}:", name), theme.success),
                        (" ".to_string(), theme.text),
                        (format!("{}%", telemetry.gpu.usage_pct), theme.text),
                        (" ".to_string(), theme.text),
                        (format!("{}°C", telemetry.gpu.temp_c), temp_col),
                        (" ".to_string(), theme.text),
                        (format!("{}W", telemetry.gpu.power_w), theme.text),
                        (" ".to_string(), theme.text),
                        (format!("{}GiB", telemetry.gpu.vram_free_gib), vram_col),
                    ],
                    theme.font_size_normal,
                );
                let border_col = if telemetry.gpu.temp_c >= theme.gpu0_temp_danger
                    || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_danger
                {
                    theme.danger
                } else if telemetry.gpu.temp_c >= theme.gpu0_temp_warn
                    || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_warn
                {
                    theme.warning
                } else {
                    theme.success
                };
                (elem, CapsuleAction::CycleGpu, border_col, 150.0)
            }

            // ── ram ─────────────────────────────────────────────────────
            "ram" => {
                let (display, total) = if telemetry.ram.effective_avail_gib > 0.0 {
                    (
                        telemetry.ram.effective_avail_gib,
                        telemetry.ram.total_gib + telemetry.ram.zram_saved_gib,
                    )
                } else {
                    (telemetry.ram.available_gib, telemetry.ram.total_gib)
                };
                let usage_ratio = if total > 0.0 {
                    ((total - display).max(0.0)) / total
                } else {
                    0.0
                };
                let val_col = if usage_ratio >= 0.85 {
                    theme.danger
                } else if usage_ratio >= 0.50 {
                    theme.warning
                } else {
                    theme.text
                };
                let val_str = if display == 0.0 {
                    " -- GiB".to_string()
                } else {
                    format!(" {:.1} GiB", display)
                };
                let elem = label::<Message>(
                    &[
                        ("RAM:".to_string(), theme.success),
                        (val_str, val_col),
                    ],
                    theme.font_size_normal,
                );
                (
                    elem,
                    CapsuleAction::OpenOverlay("ram".into()),
                    theme.border,
                    105.0,
                )
            }

            // ── battery ─────────────────────────────────────────────────
            "battery" => {
                if !telemetry.battery.has_battery {
                    let elem = label::<Message>(
                        &[
                            ("⚡ AC:".to_string(), theme.text),
                            (" Online".to_string(), theme.success),
                        ],
                        theme.font_size_normal,
                    );
                    (
                        elem,
                        CapsuleAction::OpenOverlay("power".into()),
                        theme.border,
                        85.0,
                    )
                } else {
                    let pct = telemetry.battery.percent;
                    let status = telemetry.battery.status.to_lowercase();
                    let icon = if status.contains("charging") {
                        "⚡ "
                    } else if pct <= 20 {
                        "🪫 "
                    } else {
                        "🔋 "
                    };
                    let val_col = if status.contains("charging")
                        || status.contains("full")
                        || status.contains("mains")
                    {
                        theme.success
                    } else if pct <= 20 {
                        theme.danger
                    } else if pct <= 40 {
                        theme.warning
                    } else {
                        theme.text
                    };
                    let elem = label::<Message>(
                        &[
                            (format!("{}BAT:", icon), theme.text),
                            (
                                format!(" {}% ({:.1}W)", pct, telemetry.battery.power_w),
                                val_col,
                            ),
                        ],
                        theme.font_size_normal,
                    );
                    (
                        elem,
                        CapsuleAction::OpenOverlay("power".into()),
                        val_col,
                        85.0,
                    )
                }
            }

            // ── custom ──────────────────────────────────────────────────
            // Styled like the weather capsule: theme.text label, border
            // matching the text colour.
            "custom" => {
                let col = theme.text;
                let elem = label::<Message>(
                    &[("Custom".to_string(), col)],
                    theme.font_size_normal,
                );
                (elem, CapsuleAction::ToggleCustomMode, col, 95.0)
            }

            // ── tray ────────────────────────────────────────────────────
            // Styled like the weather capsule: theme.text label, border
            // matching the text colour.
            "tray" => {
                let col = theme.text;
                let elem = label::<Message>(
                    &[("Tray".to_string(), col)],
                    theme.font_size_normal,
                );
                (elem, CapsuleAction::OpenOverlay("tray".into()), col, 80.0)
            }

            // ── fallback ────────────────────────────────────────────────
            _ => {
                let elem = label::<Message>(
                    &[(id.to_uppercase(), theme.text)],
                    theme.font_size_normal,
                );
                (
                    elem,
                    CapsuleAction::OpenOverlay(id.to_string()),
                    theme.border,
                    80.0,
                )
            }
        };

        // weather-style border: neutral capsules use theme.text
        let border_color = if border_color == theme.border { theme.text } else { border_color };

        // grow the capsule so its text always fits on one row
        let min_width = min_width.max(LABEL_W.with(|w| w.get()) + theme.slant_width + 4.0);

        let shape_type = match module_shape {
            "hexagon" => ShapeType::Hexagon {
                cut: (theme.slant_width * 1.5) as u16,
            },
            "rounded" => ShapeType::Rounded(((theme.slant_width * 1.5) as u16).max(4)),
            "flat" => ShapeType::Flat,
            _ => ShapeType::Slant {
                left: slant_left,
                right: slant_right,
            },
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
}
