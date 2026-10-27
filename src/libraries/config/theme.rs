use iced::Color;
use serde::{Deserialize, Serialize};

use super::settings::ShellSettings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Base16Palette {
    #[serde(default = "default_base00", alias = "base00")]
    pub base00: String,
    #[serde(default = "default_base01", alias = "base01")]
    pub base01: String,
    #[serde(default = "default_base02", alias = "base02")]
    pub base02: String,
    #[serde(default = "default_base03", alias = "base03")]
    pub base03: String,
    #[serde(default = "default_base04", alias = "base04")]
    pub base04: String,
    #[serde(default = "default_base05", alias = "base05")]
    pub base05: String,
    #[serde(default = "default_base06", alias = "base06")]
    pub base06: String,
    #[serde(default = "default_base07", alias = "base07")]
    pub base07: String,
    #[serde(default = "default_base08", alias = "base08")]
    pub base08: String,
    #[serde(default = "default_base09", alias = "base09")]
    pub base09: String,
    #[serde(default = "default_base0a", alias = "base0A", alias = "base0a")]
    pub base0a: String,
    #[serde(default = "default_base0b", alias = "base0B", alias = "base0b")]
    pub base0b: String,
    #[serde(default = "default_base0c", alias = "base0C", alias = "base0c")]
    pub base0c: String,
    #[serde(default = "default_base0d", alias = "base0D", alias = "base0d")]
    pub base0d: String,
    #[serde(default = "default_base0e", alias = "base0E", alias = "base0e")]
    pub base0e: String,
    #[serde(default = "default_base0f", alias = "base0F", alias = "base0f")]
    pub base0f: String,
}

fn default_base00() -> String { "#0f0f0f".to_string() }
fn default_base01() -> String { "#0f0f0f".to_string() }
fn default_base02() -> String { "#1a1a1a".to_string() }
fn default_base03() -> String { "#003399".to_string() }
fn default_base04() -> String { "#4d4e93".to_string() }
fn default_base05() -> String { "#f7f700".to_string() }
fn default_base06() -> String { "#ebdbb2".to_string() }
fn default_base07() -> String { "#cccccc".to_string() }
fn default_base08() -> String { "#ff0000".to_string() }
fn default_base09() -> String { "#fe8019".to_string() }
fn default_base0a() -> String { "#fabd2f".to_string() }
fn default_base0b() -> String { "#545454".to_string() }
fn default_base0c() -> String { "#04f100".to_string() }
fn default_base0d() -> String { "#003399".to_string() }
fn default_base0e() -> String { "#675ddb".to_string() }
fn default_base0f() -> String { "#ff8019".to_string() }

impl Default for Base16Palette {
    fn default() -> Self {
        Self {
            base00: default_base00(),
            base01: default_base01(),
            base02: default_base02(),
            base03: default_base03(),
            base04: default_base04(),
            base05: default_base05(),
            base06: default_base06(),
            base07: default_base07(),
            base08: default_base08(),
            base09: default_base09(),
            base0a: default_base0a(),
            base0b: default_base0b(),
            base0c: default_base0c(),
            base0d: default_base0d(),
            base0e: default_base0e(),
            base0f: default_base0f(),
        }
    }
}

pub fn hex_to_color(hex: &str) -> Color {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
        Color::from_rgb8(r, g, b)
    } else if clean.len() == 8 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
        let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
        Color::from_rgba8(r, g, b, (a as f32) / 255.0)
    } else {
        Color::WHITE
    }
}

#[derive(Debug, Clone)]
pub struct ShellTheme {
    pub palette: Base16Palette,
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub muted: Color,
    pub text: Color,
    pub text_strong: Color,
    pub danger: Color,
    pub warning: Color,
    pub caution: Color,
    pub success: Color,
    pub focus: Color,
    pub font_family: String,
    pub font_size_normal: u16,
    pub font_size_overlay: u16,
    pub border_width: f32,
    pub control_border_width: f32,
    pub slant_width: f32,
    pub cpu_temp_warn: u32,
    pub cpu_temp_danger: u32,
    pub cpu_usage_warn: u32,
    pub cpu_usage_danger: u32,
    pub ram_warn_gib: u32,
    pub ram_danger_gib: u32,
    pub gpu0_temp_warn: u32,
    pub gpu0_temp_danger: u32,
    pub gpu0_vram_warn: u32,
    pub gpu0_vram_danger: u32,
    pub overlay_cut: f32,
    pub overlay_angle: f32,
}

impl ShellTheme {
    pub fn from_settings(settings: &ShellSettings) -> Self {
        let palette = &settings.palette;
        let background = hex_to_color(&palette.base00);
        let surface = hex_to_color(&palette.base01);
        let surface_alt = hex_to_color(&palette.base02);
        let border = hex_to_color(&palette.base03);
        let muted = hex_to_color(&palette.base04);
        let text = hex_to_color(&palette.base05);
        let text_strong = hex_to_color(&palette.base06);
        let danger = hex_to_color(&palette.base08);
        let warning = hex_to_color(&palette.base09);
        let caution = hex_to_color(&palette.base0a);
        let success = hex_to_color(&palette.base0c);
        let focus = hex_to_color(&palette.base0d);

        Self {
            palette: settings.palette.clone(),
            background,
            surface,
            surface_alt,
            border,
            muted,
            text,
            text_strong,
            danger,
            warning,
            caution,
            success,
            focus,
            font_family: "Fira Sans Condensed".to_string(),
            font_size_normal: settings.global_font_size,
            font_size_overlay: settings.overlay_font_size,
            border_width: settings.global_border_width,
            control_border_width: settings.control_border_width,
            slant_width: settings.slant_width,
            cpu_temp_warn: settings.cpu_temp_warn,
            cpu_temp_danger: settings.cpu_temp_danger,
            cpu_usage_warn: settings.cpu_usage_warn,
            cpu_usage_danger: settings.cpu_usage_danger,
            ram_warn_gib: settings.ram_warn_gib,
            ram_danger_gib: settings.ram_danger_gib,
            gpu0_temp_warn: settings.gpu0.temp_warn,
            gpu0_temp_danger: settings.gpu0.temp_danger,
            gpu0_vram_warn: settings.gpu0.vram_warn,
            gpu0_vram_danger: settings.gpu0.vram_danger,
            overlay_cut: settings.overlay_cut as f32,
            overlay_angle: settings.overlay_angle as f32,
        }
    }
}
