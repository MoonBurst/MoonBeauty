use iced::widget::{button, container, text, Stack};
use iced::{Alignment, Element, Length};

use crate::backend::UnifiedTelemetry;
use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::style::TextExt;

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

pub struct CapsuleRenderer;

impl CapsuleRenderer {
    pub fn render<'a, Message: 'static + Clone>(
        id: &str,
        telemetry: &'a UnifiedTelemetry,
        theme: &ShellTheme,
        slant_left: SlantSide,
        slant_right: SlantSide,
        on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
    ) -> Element<'a, Message> {
        let (content, action, border_color, min_width): (Element<'a, Message>, CapsuleAction, iced::Color, f32) = match id {
            "calendar" => {
                let date_str = chrono::Local::now().format("%a, %b %d").to_string();
                let elem: Element<'a, Message> = text(date_str)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();
                (
                    elem,
                    CapsuleAction::OpenOverlay("calendar".into()),
                    theme.border,
                    110.0,
                )
            }
            "music" => {
                let txt = if let Some(m) = &telemetry.mpris {
                    let title = if m.title.len() > 18 {
                        format!("{}…", &m.title[..18])
                    } else {
                        m.title.clone()
                    };
                    if m.artist.is_empty() {
                        title
                    } else {
                        format!("{} - {}", m.artist, title)
                    }
                } else {
                    "No Music".into()
                };

                let elem: Element<'a, Message> = text(txt)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();

                (
                    elem,
                    CapsuleAction::TogglePlayPause,
                    theme.border,
                    140.0,
                )
            }
            "alarm" => {
                let elem: Element<'a, Message> = text("No Alarm")
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("reminders".into()),
                    theme.border,
                    90.0,
                )
            }
            "weather" => {
                let (txt, col) = if let Some(w) = &telemetry.weather {
                    (
                        format!("{}°F/{}°C", w.temp_f, w.temp_c),
                        match &w.alert {
                            crate::backend::WeatherAlert::Danger(_) => theme.danger,
                            crate::backend::WeatherAlert::Upcoming(_) => theme.warning,
                            crate::backend::WeatherAlert::None => theme.text,
                        },
                    )
                } else {
                    ("55°F/13°C".into(), theme.text)
                };

                let elem: Element<'a, Message> = text(txt)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(col)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("weather".into()),
                    theme.border,
                    95.0,
                )
            }
            "unified" => {
                let col = if telemetry.borg.is_active || telemetry.podman.is_rate_limited {
                    theme.warning
                } else {
                    theme.success
                };

                let label = if telemetry.borg.is_active {
                    &telemetry.borg.state_label
                } else if !telemetry.podman.summary_label.is_empty() {
                    &telemetry.podman.summary_label
                } else {
                    "Sys OK"
                };

                let elem: Element<'a, Message> = text(label)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(col)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("unified".into()),
                    theme.border,
                    80.0,
                )
            }
            "notify" => {
                let elem: Element<'a, Message> = text("Notifications: 0")
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("history".into()),
                    theme.border,
                    120.0,
                )
            }
            "clock" => {
                let t = &telemetry.bar_time;
                let elem: Element<'a, Message> = text(t)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("clock".into()),
                    theme.border,
                    85.0,
                )
            }
            "audio" => {
                let col = if telemetry.audio.is_muted {
                    theme.danger
                } else {
                    theme.success
                };

                let vol_txt = if telemetry.audio.is_muted {
                    "Audio: Mute".into()
                } else {
                    format!("Audio: {}%", telemetry.audio.volume_pct)
                };

                let elem: Element<'a, Message> = text(vol_txt)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(col)
                    .into();

                (elem, CapsuleAction::ToggleSinkMute, theme.border, 95.0)
            }
            "mic" => {
                let elem: Element<'a, Message> = text("Mic: ON")
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.success)
                    .into();

                (elem, CapsuleAction::ToggleSourceMute, theme.border, 80.0)
            }
            "net" => {
                let down = if telemetry.net.down_str.is_empty() { "0.0Mb" } else { &telemetry.net.down_str };
                let up = if telemetry.net.up_str.is_empty() { "0.0Mb" } else { &telemetry.net.up_str };
                let net_col = if telemetry.net.iface.is_empty() || telemetry.net.iface == "none" {
                    theme.danger
                } else {
                    theme.success
                };
                let elem: Element<'a, Message> = text(format!("NET: ▼{} ▲{}", down, up))
                    .size(theme.font_size_normal)
                    .bold()
                    .color(net_col)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("net".into()),
                    theme.border,
                    145.0,
                )
            }
            "cpu" => {
                // CPU shares the GPU temp thresholds (70/80°C by default) as a
                // reasonable proxy for "hot". The Quickshell reference does the
                // same thing: base0C normally, base09 at warn, base08 at danger.
                let temp_col = if telemetry.cpu.temp_c >= theme.gpu0_temp_danger {
                    theme.danger
                } else if telemetry.cpu.temp_c >= theme.gpu0_temp_warn {
                    theme.warning
                } else {
                    theme.success
                };
                let elem: Element<'a, Message> = text(format!("CPU: {}% {}°C", telemetry.cpu.usage_pct, telemetry.cpu.temp_c))
                    .size(theme.font_size_normal)
                    .bold()
                    .color(temp_col)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("cpu".into()),
                    theme.border,
                    115.0,
                )
            }
            "gpu" => {
                // Match GpuCapsule.qml: temp col is warn/danger by °C, VRAM col
                // is warn/danger by *free* GiB, and the label itself is
                // base0C (green) unless a value is out of range.
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
                // Use the worse of the two for the capsule outline color
                let border_col = if telemetry.gpu.temp_c >= theme.gpu0_temp_danger
                                     || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_danger {
                    theme.danger
                } else if telemetry.gpu.temp_c >= theme.gpu0_temp_warn
                                     || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_warn {
                    theme.warning
                } else {
                    theme.success
                };

                let name = if telemetry.gpu.name.is_empty() { "GPU".into() } else { telemetry.gpu.name.clone() };
                let txt = if telemetry.gpu.usage_pct == 0 && telemetry.gpu.temp_c == 0 {
                    format!("{}: Active", name)
                } else {
                    // Colour the temp portion by threshold. Iced's Text widget
                    // can't do inline spans, so we surface the *worst* value
                    // as the whole-label colour and keep the string compact.
                    format!("{}: {}% {}°C {}W", name, telemetry.gpu.usage_pct, telemetry.gpu.temp_c, telemetry.gpu.power_w)
                };
                let label_col = if telemetry.gpu.temp_c >= theme.gpu0_temp_danger
                                    || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_danger {
                    theme.danger
                } else if telemetry.gpu.temp_c >= theme.gpu0_temp_warn
                                    || telemetry.gpu.vram_free_gib <= theme.gpu0_vram_warn {
                    theme.warning
                } else {
                    theme.success
                };
                let _ = (temp_col, vram_col); // reserved for a future inline-styled widget

                let elem: Element<'a, Message> = text(txt)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(label_col)
                    .into();

                (elem, CapsuleAction::CycleGpu, border_col, 150.0)
            }
            "ram" => {
                let ram_val = if telemetry.ram.effective_avail_gib > 0.0 {
                    telemetry.ram.effective_avail_gib
                } else {
                    telemetry.ram.available_gib
                };

                // Match RamCapsule.qml: usage ratio vs effective total drives
                // base09 at >= 50% used and base08 at >= 85% used.
                let total = if telemetry.ram.effective_avail_gib > 0.0 {
                    telemetry.ram.total_gib + telemetry.ram.zram_saved_gib
                } else {
                    telemetry.ram.total_gib
                };
                let usage_ratio = if total > 0.0 {
                    ((total - ram_val).max(0.0)) / total
                } else {
                    0.0
                };
                let ram_col = if usage_ratio >= 0.85 {
                    theme.danger
                } else if usage_ratio >= 0.50 {
                    theme.warning
                } else {
                    theme.success
                };

                let elem: Element<'a, Message> = text(format!("RAM: {:.1} GiB", ram_val))
                    .size(theme.font_size_normal)
                    .bold()
                    .color(ram_col)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("ram".into()),
                    theme.border,
                    105.0,
                )
            }
            "battery" => {
                let txt = if !telemetry.battery.has_battery {
                    "AC: Online".into()
                } else {
                    format!("BAT: {}%", telemetry.battery.percent)
                };

                let elem: Element<'a, Message> = text(txt)
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();

                (
                    elem,
                    CapsuleAction::OpenOverlay("power".into()),
                    theme.border,
                    85.0,
                )
            }
            _ => {
                let elem: Element<'a, Message> = text(id.to_uppercase())
                    .size(theme.font_size_normal)
                    .bold()
                    .color(theme.text)
                    .into();
                (
                    elem,
                    CapsuleAction::OpenOverlay(id.to_string()),
                    theme.border,
                    80.0,
                )
            }
        };

        let background_shape = ShapeBox::new()
            .shape(ShapeType::Slant {
                left: slant_left,
                right: slant_right,
            })
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
