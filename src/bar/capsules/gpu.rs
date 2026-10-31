use super::{label, CapsuleAction};
use crate::bar::unified::UnifiedTelemetry;
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use iced::{Color, Element};

pub(super) fn render<'a, Message: 'a>(
    telemetry: &'a UnifiedTelemetry,
    theme: &ShellTheme,
    settings: &ShellSettings,
) -> (Element<'a, Message>, CapsuleAction, Color) {
    let (tw, td, vw, vd) = if telemetry.gpu_selected_idx == 0 {
        (
            theme.gpu0_temp_warn,
            theme.gpu0_temp_danger,
            theme.gpu0_vram_warn,
            theme.gpu0_vram_danger,
        )
    } else {
        (
            settings.gpu1.temp_warn,
            settings.gpu1.temp_danger,
            settings.gpu1.vram_warn,
            settings.gpu1.vram_danger,
        )
    };
    let temp_col = if telemetry.gpu.temp_c >= td {
        theme.danger
    } else if telemetry.gpu.temp_c >= tw {
        theme.warning
    } else {
        theme.text
    };
    let vram_col = if telemetry.gpu.vram_free_gib <= vd {
        theme.danger
    } else if telemetry.gpu.vram_free_gib <= vw {
        theme.warning
    } else {
        theme.text
    };
    let name = if telemetry.gpu.name.is_empty() {
        "GPU".to_string()
    } else {
        telemetry.gpu.name.clone()
    };
    let usage_danger = telemetry.gpu.usage_pct >= 90;
    let usage_warn = telemetry.gpu.usage_pct >= 75;
    let usage_col = if usage_danger {
        theme.danger
    } else if usage_warn {
        theme.warning
    } else {
        theme.text
    };

    let val_col = if telemetry.gpu.temp_c >= td || telemetry.gpu.vram_free_gib <= vd || usage_danger
    {
        theme.danger
    } else if telemetry.gpu.temp_c >= tw || telemetry.gpu.vram_free_gib <= vw || usage_warn {
        theme.warning
    } else {
        theme.text
    };

    let mut segments: Vec<(String, Color)> = vec![(format!("{}:", name), theme.success)];
    let push_num =
        |segments: &mut Vec<(String, Color)>, val: u32, width: usize, suffix: &str, col: Color| {
            let s = val.to_string();
            let pad_count = width.saturating_sub(s.len());
            segments.push((" ".to_string(), val_col));
            if pad_count > 0 {
                segments.push(("0".repeat(pad_count), theme.background));
            }
            segments.push((format!("{}{}", s, suffix), col));
        };
    push_num(
        &mut segments,
        telemetry.gpu.usage_pct as u32,
        3,
        "%",
        usage_col,
    );
    push_num(&mut segments, telemetry.gpu.temp_c, 3, "°C", temp_col);
    push_num(&mut segments, telemetry.gpu.power_w, 3, "W", val_col);
    push_num(
        &mut segments,
        telemetry.gpu.vram_free_gib,
        2,
        "GiB",
        vram_col,
    );
    let elem = label::<Message>(&segments, theme.font_size_normal);
    let border_col = if val_col == theme.text {
        theme.border
    } else {
        val_col
    };
    (elem, CapsuleAction::OpenOverlay("gpu".into()), border_col)
}
