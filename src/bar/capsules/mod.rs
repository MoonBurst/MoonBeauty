use iced::widget::{button, container, text, MouseArea, Row, Stack};
use iced::{Alignment, Color, Element, Length};
use std::sync::{Mutex, OnceLock};

use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType, SlantSide};
use crate::libraries::style::shell_font;

mod alarm;
mod audio;
mod battery;
mod calendar;
mod clock;
mod cpu;
mod custom;
mod gpu;
mod music;
mod net;
mod notify;
mod ram;
mod tray;
mod unified;
mod weather;

// ══════════════ Shared helpers ══════════════

thread_local! {
    static LABEL_W: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

fn est_text_width(txt: &str, size: u16) -> f32 {
    let em = size as f32;
    // Fira Code is monospaced: every character (including spaces) 
    // takes exactly the same width. We use 0.65em to provide a safe margin.
    txt.chars().count() as f32 * (em * 0.65)
}

fn label<'a, Message: 'a>(segments: &[(String, Color)], size: u16) -> Element<'a, Message> {
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
                .size(size as f32)
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
        let old = m.get(id).copied();
        if old != Some(w) && std::env::var_os("MBS_DEBUG").is_some() {
            eprintln!("[capsule-width] {} = {:.1}px", id, w);
        }
        m.insert(id.to_string(), w);
    }
}

pub fn tooltip_bias(id: &str, theme: &ShellTheme) -> (i32, i32) {
    match id {
        "tray" => tray::tooltip_bias(theme),
        _ => (0, 0),
    }
}
pub fn lookup_width(id: &str) -> Option<f32> {
    widths_map().lock().ok().and_then(|m| m.get(id).copied())
}

pub fn compute_expanded_visible_width(settings: &ShellSettings, item_count: usize) -> f32 {
    let shown = item_count.min(8);
    let icon_size = settings.tray_icon_size as f32;
    let icon_slot = icon_size + 8.0;
    let close_slot = 24.0;
    let spacing = 4.0;
    let empty_w = if item_count == 0 { 150.0 } else { 0.0 };
    let content_w = close_slot
        + if shown > 0 { spacing } else { 0.0 }
        + (icon_slot + spacing) * shown as f32
        + empty_w;
    content_w + 32.0
}

// ══════════════ Public API ══════════════

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
        let (content, action, border_color): (Element<'a, Message>, CapsuleAction, Color) = match id
        {
            "calendar" => calendar::render::<Message>(theme),
            "music" => music::render::<Message>(telemetry, theme),
            "alarm" => alarm::render::<Message>(theme),
            "weather" => weather::render::<Message>(telemetry, theme),
            "unified" => unified::render::<Message>(telemetry, theme),
            "notify" | "notifications" => notify::render::<Message>(theme),
            "clock" => clock::render::<Message>(telemetry, theme),
            "audio" => audio::render::<Message>(telemetry, theme),
            "mic" => audio::render_mic::<Message>(telemetry, theme),
            "net" => net::render::<Message>(telemetry, theme),
            "cpu" => cpu::render::<Message>(telemetry, theme),
            "gpu" => gpu::render::<Message>(telemetry, theme, settings),
            "ram" => ram::render::<Message>(telemetry, theme),
            "battery" => battery::render::<Message>(telemetry, theme),
            "custom" => custom::render::<Message>(theme),
            "tray" => tray::render::<Message>(theme, region),
            _ => custom::fallback::<Message>(id, theme),
        };

        let text_w = LABEL_W.with(|w| w.get());
        
        // Let all capsules dynamically size to their text content.
        // Fira Code (monospace) ensures numbers don't cause jitter.
        let bounding_w = if id == "tray" {
            settings.tray_width as f32 + theme.slant_width * 2.0
        } else {
            // Add a 16px buffer so the text never touches the slanted edges
            text_w + theme.slant_width * 2.0 + settings.capsule_internal_padding as f32 + 16.0
        };
        record_width(id, bounding_w);

        let border_color = if border_color == theme.border {
            theme.text
        } else {
            border_color
        };

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
            .size(Length::Fixed(bounding_w), Length::Fill);

        let button_content = container(content)
            .width(Length::Fixed(bounding_w))
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        let action_btn = button(
            Stack::new()
                .push(background_shape.view())
                .push(button_content),
        )
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
        settings: &ShellSettings,
        slant_left: SlantSide,
        slant_right: SlantSide,
        region: &str,
        display_width: f32,
        on_action: impl Fn(CapsuleAction) -> Message + 'static + Copy,
    ) -> Element<'a, Message> {
        tray::render_expanded(
            items,
            theme,
            settings,
            slant_left,
            slant_right,
            region,
            display_width,
            on_action,
        )
    }
}
