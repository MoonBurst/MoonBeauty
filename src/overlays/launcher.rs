use iced::widget::{button, column, container, row, scrollable, text, text_input, Stack, Row};
use iced::{Alignment, Color, Element, Length, Padding};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use super::calc::CalculatorOverlay;
use super::pass::PassOverlay;
use crate::config::settings::ShellSettings;
use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::{shell_font, TextExt};

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LauncherMessage {
    SearchChanged(String),
    ExecuteApp(String),
    Dismiss,
    Refocus,
    SelectNext,
    SelectPrev,
    SelectDown,
    SelectUp,
}

pub const LAUNCHER_SEARCH_ID: &str = "launcher_search";

pub struct AppLauncher {
    pub search_query: String,
    pub all_apps: Vec<AppEntry>,
    pub filtered_apps: Vec<AppEntry>,
    pub focus_requested: bool,
    pub selected_index: usize,
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLauncher {
    pub fn new() -> Self {
        let mut all_apps = vec![
            AppEntry { name: "Settings & Options".into(), exec: "internal:settings".into(), icon: "⚙".into(), icon_path: None },
            AppEntry { name: "Clipboard Manager".into(), exec: "internal:clip".into(), icon: "📋".into(), icon_path: None },
            AppEntry { name: "Quick Notes".into(), exec: "internal:notes".into(), icon: "📝".into(), icon_path: None },
            AppEntry { name: "Password Store".into(), exec: "internal:pass".into(), icon: "🔑".into(), icon_path: None },
            AppEntry { name: "Power & Session".into(), exec: "internal:power".into(), icon: "⚡".into(), icon_path: None },
            AppEntry { name: "Tasks & Todos".into(), exec: "internal:todo".into(), icon: "✅".into(), icon_path: None },
            AppEntry { name: "Reminders & Alarms".into(), exec: "internal:reminders".into(), icon: "⏰".into(), icon_path: None },
            AppEntry { name: "Dictionary Lookup".into(), exec: "internal:dict".into(), icon: "📖".into(), icon_path: None },
            AppEntry { name: "Unicode Glyphs".into(), exec: "internal:unicode".into(), icon: "🔣".into(), icon_path: None },
            AppEntry { name: "Web Search".into(), exec: "internal:web".into(), icon: "🌐".into(), icon_path: None },
            AppEntry { name: "Screenshot (Quickshot)".into(), exec: "moon-beauty-shell --screenshot --annotate".into(), icon: "📷".into(), icon_path: None },
        ];
        all_apps.extend(Self::scan_desktop_files());

        Self {
            search_query: String::new(),
            filtered_apps: all_apps.clone(),
            all_apps,
            focus_requested: true,
            selected_index: 0,
        }
    }

    /// If `query` looks like a math expression or unit conversion, evaluate
    /// it via the existing CalculatorOverlay. Returns None for non-math input
    /// so the launcher doesn't show a bogus "= firefox" row.
    fn compute_math_result(query: &str) -> Option<String> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        // Require at least one operator or a unit-conversion phrase.
        // Pure numbers ("123") are treated as search queries.
        let has_operator = q.contains('+')
            || q.contains('-')
            || q.contains('*')
            || q.contains('/')
            || q.contains('(');
        let has_unit = q.contains(" in ") || q.contains(" to ");
        if !has_operator && !has_unit {
            return None;
        }
        let result = CalculatorOverlay::evaluate(q);
        if result.is_empty() || result == "Syntax Error" {
            None
        } else {
            Some(result)
        }
    }

    /// Parse a bare number+unit query like "32f", "10usd", "100kg", "2.5mi".
    /// Returns None if the query has whitespace between number and unit
    /// (which means it's likely a phrase query like "100 kg in lbs" that the
    /// main math evaluator should handle instead).
    fn parse_number_unit(query: &str) -> Option<(f64, String)> {
        let q = query.trim();
        // Find longest prefix that parses as f64
        let mut best_end = 0;
        for i in 1..=q.len() {
            if q[..i].parse::<f64>().is_ok() {
                best_end = i;
            }
        }
        if best_end == 0 { return None; }
        let num: f64 = q[..best_end].parse().ok()?;
        let rest = q[best_end..].trim();
        if rest.is_empty() { return None; }
        if rest.contains(char::is_whitespace) { return None; }
        Some((num, rest.to_lowercase()))
    }

    fn fmt_num(v: f64) -> String {
        if v.fract().abs() < 0.001 {
            format!("{:.0}", v)
        } else {
            let s = format!("{:.4}", v);
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    }

    fn convert_temp(value: f64, from: char) -> Vec<(String, String)> {
        let from_label = match from { 'C' => "°C", 'F' => "°F", 'K' => "K", 'R' => "°R", _ => "?" };
        let celsius = match from {
            'C' => value,
            'F' => (value - 32.0) * 5.0 / 9.0,
            'K' => value - 273.15,
            'R' => (value - 491.67) * 5.0 / 9.0,
            _ => value,
        };
        let units = [('C', "°C"), ('F', "°F"), ('K', "K"), ('R', "°R")];
        let mut out = Vec::new();
        for (u, label) in units {
            if u == from { continue; }
            let converted = match u {
                'C' => celsius,
                'F' => celsius * 9.0 / 5.0 + 32.0,
                'K' => celsius + 273.15,
                'R' => (celsius + 273.15) * 9.0 / 5.0,
                _ => celsius,
            };
            let result = format!("{} {}", Self::fmt_num(converted), label);
            let display = format!("{} {} = {}", Self::fmt_num(value), from_label, result);
            out.push((display, format!("math:{}", result)));
        }
        out
    }

    fn convert_linear(value: f64, unit: &str, units: &[(&str, &str, f64)]) -> Option<Vec<(String, String)>> {
        let (_, from_label, from_factor) = units.iter().find(|(k, _, _)| k.eq_ignore_ascii_case(unit))?;
        let base = value * from_factor;
        let mut out = Vec::new();
        for (key, label, factor) in units {
            if key.eq_ignore_ascii_case(unit) { continue; }
            let converted = base / factor;
            let result = format!("{} {}", Self::fmt_num(converted), label);
            let display = format!("{} {} = {}", Self::fmt_num(value), from_label, result);
            out.push((display, format!("math:{}", result)));
        }
        Some(out)
    }

    fn convert_currency(value: f64, code: &str) -> Option<Vec<(String, String)>> {
        let rates = Self::get_rates();
        let upper = code.to_uppercase();
        let from_rate = *rates.get(&upper)?;
        let usd = value / from_rate;
        let mut keys: Vec<_> = rates.keys().cloned().collect();
        keys.sort();
        let mut out = Vec::new();
        for to_code in keys {
            if to_code == upper { continue; }
            let to_rate = rates.get(&to_code)?;
            let converted = usd * to_rate;
            let result = format!("{:.2} {}", converted, to_code);
            let display = format!("{:.2} {} = {}", value, upper, result);
            out.push((display, format!("math:{}", result)));
        }
        Some(out)
    }

    /// Hardcoded fallback rates (USD base). Approximate as of 2025; users
    /// who need accurate rates can wire up an API later via a settings hook.
    fn get_rates() -> &'static std::collections::HashMap<String, f64> {
        static RATES: std::sync::OnceLock<std::collections::HashMap<String, f64>> =
            std::sync::OnceLock::new();
        RATES.get_or_init(|| {
            let mut m = std::collections::HashMap::new();
            m.insert("USD".to_string(), 1.00);
            m.insert("EUR".to_string(), 0.92);
            m.insert("GBP".to_string(), 0.79);
            m.insert("JPY".to_string(), 150.2);
            m.insert("CAD".to_string(), 1.37);
            m.insert("AUD".to_string(), 1.52);
            m.insert("CHF".to_string(), 0.88);
            m.insert("CNY".to_string(), 7.24);
            m.insert("INR".to_string(), 83.3);
            m.insert("KRW".to_string(), 1330.0);
            m.insert("MXN".to_string(), 17.1);
            m.insert("BRL".to_string(), 4.97);
            m.insert("ZAR".to_string(), 18.9);
            m.insert("SEK".to_string(), 10.4);
            m.insert("NOK".to_string(), 10.6);
            m.insert("NZD".to_string(), 1.64);
            m.insert("SGD".to_string(), 1.34);
            m.insert("HKD".to_string(), 7.82);
            m.insert("TRY".to_string(), 30.1);
            m.insert("RUB".to_string(), 92.0);
            m
        })
    }

    fn try_convert(num: f64, unit: &str) -> Option<Vec<(String, String)>> {
        // Temperature
        match unit {
            "f" | "fahrenheit" => return Some(Self::convert_temp(num, 'F')),
            "c" | "celsius" => return Some(Self::convert_temp(num, 'C')),
            "k" | "kelvin" => return Some(Self::convert_temp(num, 'K')),
            "r" | "rankine" => return Some(Self::convert_temp(num, 'R')),
            _ => {}
        }
        // Distance
        const DISTANCE: &[(&str, &str, f64)] = &[
            ("mm", "mm", 0.001), ("cm", "cm", 0.01), ("m", "m", 1.0),
            ("km", "km", 1000.0), ("in", "in", 0.0254), ("ft", "ft", 0.3048),
            ("yd", "yd", 0.9144), ("mi", "mi", 1609.344), ("nmi", "nmi", 1852.0),
        ];
        if let Some(r) = Self::convert_linear(num, unit, DISTANCE) { return Some(r); }
        // Weight
        const WEIGHT: &[(&str, &str, f64)] = &[
            ("mg", "mg", 0.000001), ("g", "g", 0.001), ("kg", "kg", 1.0),
            ("t", "t", 1000.0), ("oz", "oz", 0.0283495), ("lb", "lb", 0.453592),
            ("st", "st", 6.35029),
        ];
        if let Some(r) = Self::convert_linear(num, unit, WEIGHT) { return Some(r); }
        // Volume
        const VOLUME: &[(&str, &str, f64)] = &[
            ("ml", "mL", 0.001), ("l", "L", 1.0), ("cup", "cups", 0.236588),
            ("pt", "pt", 0.473176), ("qt", "qt", 0.946353), ("gal", "gal", 3.78541),
            ("floz", "fl oz", 0.0295735),
        ];
        if let Some(r) = Self::convert_linear(num, unit, VOLUME) { return Some(r); }
        // Data
        const DATA: &[(&str, &str, f64)] = &[
            ("b", "B", 1.0), ("kb", "KB", 1024.0), ("mb", "MB", 1048576.0),
            ("gb", "GB", 1073741824.0), ("tb", "TB", 1099511627776.0),
        ];
        if let Some(r) = Self::convert_linear(num, unit, DATA) { return Some(r); }
        // Time
        const TIME: &[(&str, &str, f64)] = &[
            ("s", "s", 1.0), ("min", "min", 60.0), ("h", "h", 3600.0),
            ("d", "d", 86400.0), ("w", "wk", 604800.0), ("y", "yr", 31536000.0),
        ];
        if let Some(r) = Self::convert_linear(num, unit, TIME) { return Some(r); }
        // Speed
        const SPEED: &[(&str, &str, f64)] = &[
            ("ms", "m/s", 1.0), ("kmh", "km/h", 0.277778),
            ("mph", "mph", 0.44704), ("kn", "kn", 0.514444),
        ];
        if let Some(r) = Self::convert_linear(num, unit, SPEED) { return Some(r); }
        // Currency
        if let Some(r) = Self::convert_currency(num, unit) { return Some(r); }
        None
    }

    /// Cached pass-store entries — reads the filesystem once per launcher session.
    fn get_pass_keys() -> &'static Vec<String> {
        static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        CACHE.get_or_init(|| PassOverlay::scan_passwords())
    }

    /// If the query starts with a pass-prefix keyword (`pass`, `passw`, `password`,
    /// `pw`, `passwd`, `cred`, `creds`), return all matching entries.
    /// Query `pass git` → entries containing "git".
    /// Query `pass` alone → all entries (capped at 30).
    fn compute_pass_entries(query: &str) -> Option<Vec<(String, String)>> {
        let q = query.trim().to_lowercase();
        let filter_term: &str = if matches!(q.as_str(),
            "pass" | "passw" | "password" | "pw" | "passwd"
            | "cred" | "creds" | "credential" | "credentials" | "secret" | "secrets"
        ) {
            ""
        } else if let Some(rest) = q.strip_prefix("pass ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("passw ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("password ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("pw ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("passwd ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("cred ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("creds ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("credential ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("credentials ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("secret ") {
            rest.trim()
        } else if let Some(rest) = q.strip_prefix("secrets ") {
            rest.trim()
        } else {
            return None;
        };

        let keys = Self::get_pass_keys();
        let results: Vec<(String, String)> = keys.iter()
            .filter(|k| filter_term.is_empty() || k.to_lowercase().contains(filter_term))
            .take(30)
            .map(|k| (k.clone(), format!("pass:{}", k)))
            .collect();

        Some(results)
    }

    /// The new multi-result entry point.
    /// - "32f"  → 3 temp conversions
    /// - "10usd"→ 19 currency conversions
    /// - "10+10"→ 1 math result
    /// - "10"   → nothing (no operator, no unit)
    fn compute_multi_conversions(query: &str) -> Vec<(String, String)> {
        // 1. Try number+unit syntax first
        if let Some((num, unit)) = Self::parse_number_unit(query) {
            if let Some(results) = Self::try_convert(num, &unit) {
                if !results.is_empty() {
                    return results;
                }
            }
        }
        // 2. Fall back to single math expression
        let q = query.trim();
        if q.is_empty() { return vec![]; }
        let has_operator = q.contains('+') || q.contains('-') || q.contains('*') || q.contains('/') || q.contains('(');
        let has_unit_phrase = q.contains(" in ") || q.contains(" to ");
        if !has_operator && !has_unit_phrase {
            return vec![];
        }
        let result = CalculatorOverlay::evaluate(q);
        if result.is_empty() || result == "Syntax Error" {
            return vec![];
        }
        vec![(format!("= {}", result), format!("math:{}", result))]
    }

    /// Parse a bare number+unit query like "32f", "10usd", "100kg", "2.5mi".
    /// Returns None if the query has whitespace between number and unit
    /// (which means it's likely a phrase query like "100 kg in lbs" that the
    /// main math evaluator should handle instead).
    pub fn scan_desktop_files() -> Vec<AppEntry> {
        let mut apps = Vec::new();
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let dirs = [
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            home.join(".local/share/applications"),
            PathBuf::from("/run/current-system/sw/share/applications"),
            home.join(".nix-profile/share/applications"),
        ];

        let mut seen = std::collections::HashSet::new();
        let mut total_files = 0usize;
        let mut hidden = 0usize;

        for dir in &dirs {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                        total_files += 1;
                        if let Ok(content) = fs::read_to_string(&path) {
                            let mut name = None;
                            let mut exec = None;
                            let mut icon_name: Option<String> = None;
                            let mut nodisplay = false;

                            for line in content.lines() {
                                if line.starts_with("Name=") && name.is_none() {
                                    name = Some(line["Name=".len()..].trim().to_string());
                                } else if line.starts_with("Exec=") && exec.is_none() {
                                    let raw_exec = line["Exec=".len()..].trim();
                                    let cleaned = raw_exec
                                        .split('%')
                                        .next()
                                        .unwrap_or(raw_exec)
                                        .trim()
                                        .to_string();
                                    exec = Some(cleaned);
                                } else if line.starts_with("Icon=") && icon_name.is_none() {
                                    icon_name = Some(line["Icon=".len()..].trim().to_string());
                                } else if line.eq_ignore_ascii_case("NoDisplay=true") {
                                    nodisplay = true;
                                }
                            }

                            if nodisplay { hidden += 1; continue; }
                            if let (Some(n), Some(e)) = (name, exec) {
                                let key = format!("{}|{}", n.to_lowercase(), e.to_lowercase());
                                if !seen.contains(&key) {
                                    seen.insert(key);
                                    let icon_path = icon_name
                                        .as_deref()
                                        .and_then(Self::resolve_icon);
                                    apps.push(AppEntry {
                                        name: n,
                                        exec: e,
                                        icon: "🚀".to_string(),
                                        icon_path,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        apps.sort_by_key(|a| a.name.to_lowercase());
        let with_icons = apps.iter().filter(|a| a.icon_path.is_some()).count();
        eprintln!(
            "[launcher] {} .desktop files scanned ({} hidden), {} apps loaded, {} with icons",
            total_files, hidden, apps.len(), with_icons,
        );
        apps
    }

    fn resolve_icon(icon_name: &str) -> Option<String> {
        use std::path::Path;
        if icon_name.is_empty() { return None; }
        if icon_name.starts_with('/') {
            return if Path::new(icon_name).exists() {
                Some(icon_name.to_string())
            } else { None };
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let roots = [
            PathBuf::from("/run/current-system/sw/share/icons/hicolor"),
            PathBuf::from("/usr/share/icons/hicolor"),
            PathBuf::from("/usr/local/share/icons/hicolor"),
            home.join(".local/share/icons/hicolor"),
            home.join(".nix-profile/share/icons/hicolor"),
        ];
        let sizes = ["48x48", "64x64", "128x128", "256x256", "512x512", "scalable", "symbolic"];
        let exts = ["png", "svg", "xpm"];
        for root in &roots {
            for size in &sizes {
                for ext in &exts {
                    let p = root.join(size).join("apps")
                        .join(format!("{}.{}", icon_name, ext));
                    if p.exists() {
                        return Some(p.to_string_lossy().into_owned());
                    }
                }
            }
        }
        for pix in ["/run/current-system/sw/share/pixmaps", "/usr/share/pixmaps"] {
            for ext in &exts {
                let p = PathBuf::from(pix).join(format!("{}.{}", icon_name, ext));
                if p.exists() {
                    return Some(p.to_string_lossy().into_owned());
                }
            }
        }
        None
    }

    pub fn update(&mut self, message: LauncherMessage) -> iced::Task<LauncherMessage> {
        match message {
            LauncherMessage::Refocus => {
                return iced::widget::text_input::focus(LAUNCHER_SEARCH_ID);
            }
            LauncherMessage::SelectNext => {
                if !self.filtered_apps.is_empty() {
                    self.selected_index = (self.selected_index + 1) % self.filtered_apps.len();
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectPrev => {
                if !self.filtered_apps.is_empty() {
                    let n = self.filtered_apps.len();
                    self.selected_index = (self.selected_index + n - 1) % n;
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectDown => {
                if !self.filtered_apps.is_empty() {
                    let math_count = self.filtered_apps.iter()
                        .filter(|a| a.exec.starts_with("math:")).count();
                    // Grid row = +3, app list = +1
                    let step = if math_count > 0 && self.selected_index < math_count { 3 } else { 1 };
                    self.selected_index = (self.selected_index + step).min(self.filtered_apps.len() - 1);
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectUp => {
                if !self.filtered_apps.is_empty() {
                    let math_count = self.filtered_apps.iter()
                        .filter(|a| a.exec.starts_with("math:")).count();
                    let step = if math_count > 0 && self.selected_index < math_count { 3 } else { 1 };
                    self.selected_index = self.selected_index.saturating_sub(step);
                }
                return iced::Task::none();
            }
            LauncherMessage::SearchChanged(q) => {
                eprintln!("[launcher-input] {:?}", q);
                self.search_query = q.clone();
                self.selected_index = 0;

                let lower = q.trim().to_lowercase();
                let mut filtered: Vec<AppEntry> = if lower.is_empty() {
                    self.all_apps.clone()
                } else {
                    let mut f: Vec<AppEntry> = self
                        .all_apps
                        .iter()
                        .filter(|a| {
                            a.name.to_lowercase().contains(&lower)
                                || a.exec.to_lowercase().contains(&lower)
                        })
                        .cloned()
                        .collect();
                    if let Some(alias) = Self::routing_alias(&lower) {
                        if !f.iter().any(|a| a.exec == alias.exec) {
                            f.insert(0, alias);
                        }
                    }
                    f
                };

                // Password store search — takes priority if the query starts with a pass-prefix
                if let Some(pass_entries) = Self::compute_pass_entries(&q) {
                    eprintln!("[launcher-pass] {:?} → {} entries", q, pass_entries.len());
                    let mut pass_items: Vec<AppEntry> = pass_entries.into_iter().map(|(name, exec)| {
                        AppEntry {
                            name,
                            exec,
                            icon: ">".to_string(),
                            icon_path: None,
                        }
                    }).collect();
                    pass_items.append(&mut filtered);
                    filtered = pass_items;
                }

                // Multi-conversion + math result at top of list
                let conversions = Self::compute_multi_conversions(&q);
                if !conversions.is_empty() {
                    eprintln!("[launcher-multi] {:?} → {} results", q, conversions.len());
                    let mut math_entries: Vec<AppEntry> = conversions.into_iter().map(|(label, exec)| {
                        AppEntry {
                            name: label,
                            exec,
                            icon: "=".to_string(),
                            icon_path: None,
                        }
                    }).collect();
                    math_entries.append(&mut filtered);
                    filtered = math_entries;
                } else {
                    eprintln!("[launcher-multi] {:?} → 0 results", q);
                }

                eprintln!("[launcher-final] {} entries:", filtered.len());
                for (i, e) in filtered.iter().enumerate().take(5) {
                    eprintln!("  [{}] {} -> {}", i, e.name, e.exec);
                }
                self.filtered_apps = filtered;
                return iced::Task::none();
            }
            _ => {}
        }
        iced::Task::none()
    }

    fn routing_alias(query: &str) -> Option<AppEntry> {
        let (name, exec) = match query {
            // Settings
            "cfg" | "set" | "settings" | "config" | "prefs" | "preferences"
            | "options" | "settings & options" | "preference" | "configuration" => {
                ("Settings & Options", "internal:settings")
            }
            // Clipboard
            "clip" | "clipboard" | "clipboard manager" | "clipboard history" | "paste" | "pastebin" => {
                ("Clipboard Manager", "internal:clip")
            }
            // Todo
            "todo" | "td" | "task" | "tasks" | "tasks & todos" | "task list" => {
                ("Tasks & Todos", "internal:todo")
            }
            // Notes
            "note" | "notes" | "quick notes" | "quicknote" => {
                ("Quick Notes", "internal:notes")
            }
            // Power
            "power" | "pwr" | "session" | "power & session" | "logout"
            | "reboot" | "shutdown" | "lock" | "lockscreen" => {
                ("Power & Session", "internal:power")
            }
            // Reminders
            "remind" | "reminder" | "reminders" | "alarm" | "alarms"
            | "reminders & alarms" | "timer" | "timers" => {
                ("Reminders & Alarms", "internal:reminders")
            }
            // Dictionary
            "dict" | "def" | "dictionary" | "dictionary lookup" | "word"
            | "lookup" | "define" | "defn" => {
                ("Dictionary", "internal:dict")
            }
            // Unicode
            "unicode" | "glyph" | "glyphs" | "symbol" | "symbols"
            | "emoji" | "emojis" | "unicode glyphs" => {
                ("Unicode", "internal:unicode")
            }
            // Email
            "email" | "mail" | "em" | "mail client" | "inbox" => {
                ("Email", "internal:email")
            }
            // RNG / Dice
            "rng" | "dice" | "roll" | "random" | "rand" | "d20" => {
                ("RNG / Dice", "internal:rng")
            }
            // Web search
            "web" | "search" | "web search" | "google" | "ddg" | "duckduckgo" => {
                ("Web Search", "internal:web")
            }
            // Amogus
            "amogus" | "amongus" | "among us" | "sus" => {
                ("Among Us", "internal:amogus")
            }
            // Screenshot
            "shot" | "screenshot" | "capture" | "screenshot (quickshot)" => {
                ("Screenshot", "moon-beauty-shell --screenshot --annotate")
            }
            _ => return None,
        };
        Some(AppEntry { name: name.into(), exec: exec.into(), icon: ">".into(), icon_path: None })
    }

    pub fn focus_search(&mut self) -> iced::Task<LauncherMessage> {
        iced::widget::text_input::focus(LAUNCHER_SEARCH_ID)
    }

    pub async fn launch(exec: &str) {
        if exec.is_empty() {
            return;
        }
        let _ = Command::new("systemd-run")
            .args(["--user", "--collect", "--quiet", "--slice=app.slice", "sh", "-c", exec])
            .spawn();
    }

    pub fn view<'a>(&'a self, theme: &'a ShellTheme, settings: &ShellSettings) -> Element<'a, LauncherMessage> {
        let (win_w, win_h) = settings.get_window_size("launcher", 840, 650);

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let pad_top = 18.0_f32;
        let pad_bottom = 16.0_f32;

        // Inward horizontal clearance for the search bar (y ≈ 18px)
        let search_inward = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        // Inward horizontal clearance for the scrollable list items (y_top ≈ 72px, y_bottom ≈ win_h - 45px)
        let list_top_inset = if dy > 72.0 {
            (dx * (1.0 - (72.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_bottom_inset = if dy > 45.0 {
            (dx * (1.0 - (45.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_side_inset = list_top_inset.max(list_bottom_inset).max(20.0);

        // Inward clearance for bottom footer
        let footer_inward = if dy > pad_bottom + 8.0 {
            (dx * (1.0 - ((pad_bottom + 8.0) / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        let input = text_input("Search applications or settings...", &self.search_query)
            .id(LAUNCHER_SEARCH_ID)
            .font(shell_font(iced::font::Weight::Normal))
            .on_input(LauncherMessage::SearchChanged)
            .on_submit(
                if let Some(selected) = self.filtered_apps.get(self.selected_index) {
                    LauncherMessage::ExecuteApp(selected.exec.clone())
                } else if let Some(first) = self.filtered_apps.first() {
                    LauncherMessage::ExecuteApp(first.exec.clone())
                } else {
                    LauncherMessage::ExecuteApp(self.search_query.clone())
                },
            )
            .padding(14)
            .size(theme.font_size_overlay)
            .width(Length::Fill)
            .style(move |_t, status| {
        let border_color = match status {
            text_input::Status::Focused => theme.focus,
            text_input::Status::Hovered => theme.border,
            _ => theme.border,
        };
        let border_width = match status {
            text_input::Status::Focused => theme.control_border_width + 0.5,
            _ => theme.control_border_width,
        };
        text_input::Style {
            background: iced::Background::Color(theme.surface_alt),
            border: iced::Border {
                color: border_color,
                width: border_width,
                radius: 8.0.into(),
            },
            icon: theme.muted,
            placeholder: theme.muted,
            value: theme.text,
            selection: theme.focus,
        }
    });

        let input_row = container(input)
            .width(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: search_inward,
                bottom: 0.0,
                left: search_inward,
            });

        // Split math results from apps — math gets hex grid, apps get list
        let math_count = self.filtered_apps.iter()
            .filter(|a| a.exec.starts_with("math:")).count();
        let math_items: Vec<&AppEntry> = self.filtered_apps.iter()
            .filter(|a| a.exec.starts_with("math:")).take(40).collect();
        let app_items: Vec<&AppEntry> = self.filtered_apps.iter()
            .filter(|a| !a.exec.starts_with("math:")).take(40).collect();

        let mut content_col = column![].spacing(10).width(Length::Fill);

        // Math results as 3-column hexagon grid
        if !math_items.is_empty() {
            let tile_h = 76.0_f32;
            // Compute tile width from container width — required for ShapeBox to render
            let content_w = win_w - list_side_inset * 2.0;
            let tile_w = ((content_w - 12.0) / 3.0).max(60.0);
            let mut grid_col = column![].spacing(6).width(Length::Fill);
            for (chunk_idx, chunk) in math_items.chunks(3).enumerate() {
                let mut grid_row = Row::new().spacing(6).width(Length::Fill);
                for (rel_i, item) in chunk.iter().enumerate() {
                    let global_idx = chunk_idx * 3 + rel_i;
                    let is_selected = global_idx == self.selected_index;
                    let exec_cmd = item.exec.clone();
                    let copy_val = item.exec.strip_prefix("math:").unwrap_or("").to_string();
                    let input_part = if item.name.starts_with("= ") {
                        let q = self.search_query.clone();
                        if q.chars().count() > 20 {
                            q.chars().take(17).collect::<String>() + "..."
                        } else {
                            q
                        }
                    } else {
                        item.name.split(" = ").next().unwrap_or("").to_string()
                    };

                    let bg = if is_selected { theme.surface_alt } else { theme.background };
                    let border_c = if is_selected { theme.text } else { theme.border };

                    let tile_bg = ShapeBox::new()
                        .shape(ShapeType::Hexagon { cut: 8 })
                        .background(bg)
                        .border(border_c, theme.control_border_width)
                        .size(Length::Fixed(tile_w), Length::Fixed(tile_h));

                    let tile_content = container(
                        column![
                            text(copy_val)
                                .size(theme.font_size_overlay + 1)
                                .bold()
                                .color(theme.text_strong)
                                .align_x(Alignment::Center)
                                .wrapping(iced::widget::text::Wrapping::None),
                            text(input_part)
                                .size(theme.font_size_normal.saturating_sub(2))
                                .color(theme.muted)
                                .align_x(Alignment::Center)
                                .wrapping(iced::widget::text::Wrapping::None),
                        ]
                        .spacing(2)
                        .align_x(Alignment::Center)
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .padding([0, 20]);

                    let tile_inner = container(
                        Stack::new().push(tile_bg.view()).push(tile_content)
                    )
                    .width(Length::Fixed(tile_w))
                    .height(Length::Fixed(tile_h))
                    .clip(true);

                    let tile = button(tile_inner)
                        .on_press(LauncherMessage::ExecuteApp(exec_cmd))
                        .padding(0)
                        .width(Length::Fixed(tile_w))
                        .height(Length::Fixed(tile_h))
                        .style(|_t, _s| button::Style { background: None, ..Default::default() });

                    grid_row = grid_row.push(tile);
                }
                for _ in chunk.len()..3 {
                    grid_row = grid_row.push(container("").width(Length::Fixed(tile_w)).height(Length::Fixed(tile_h)));
                }
                grid_col = grid_col.push(grid_row);
            }
            content_col = content_col.push(grid_col);
        }

        // App rows (unchanged logic, but global index offset by math_count)
        if !app_items.is_empty() {
            let mut list_col = column![].spacing(6).width(Length::Fill);
            for (rel_idx, app) in app_items.iter().enumerate() {
                let idx = math_count + rel_idx;
                let exec_cmd = app.exec.clone();
                let icon_str = if app.exec.starts_with("internal:") { ">" } else { ">" };
                let icon_elem: Element<'_, LauncherMessage> = if let Some(ref path) = app.icon_path {
                    iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                        .width(Length::Fixed(24.0))
                        .height(Length::Fixed(24.0))
                        .into()
                } else {
                    text(icon_str).size(16).into()
                };

                let is_selected = idx == self.selected_index;
                let row_item = button(
                    row![
                        icon_elem,
                        column![
                            text(&app.name)
                                .size(theme.font_size_overlay)
                                .bold(),
                            text(&app.exec)
                                .size(theme.font_size_normal.saturating_sub(2))
                                .shell()
                                .color(theme.muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .on_press(LauncherMessage::ExecuteApp(exec_cmd))
                .padding([8, 14])
                .width(Length::Fill)
                .style(move |_t, status| {
                    let is_hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                    let bg = if is_hovered || is_selected {
                        Some(iced::Background::Color(theme.surface_alt))
                    } else {
                        None
                    };
                    let border_color = if is_hovered {
                        theme.focus
                    } else if is_selected {
                        theme.text
                    } else {
                        Color::TRANSPARENT
                    };
                    button::Style {
                        background: bg,
                        border: iced::Border {
                            color: border_color,
                            width: if is_hovered || is_selected {
                                theme.control_border_width
                            } else {
                                0.0
                            },
                            radius: 6.0.into(),
                        },
                        text_color: if is_selected || is_hovered {
                            theme.text_strong
                        } else {
                            theme.text
                        },
                        ..Default::default()
                    }
                });

                list_col = list_col.push(row_item);
            }
            content_col = content_col.push(list_col);
        }

        if math_items.is_empty() && app_items.is_empty() {
            content_col = content_col.push(
                container(text("No results").size(theme.font_size_overlay).color(theme.muted))
                    .width(Length::Fill)
                    .center_x(Length::Fill)
                    .padding(20),
            );
        }

        let list_container = container(scrollable(content_col).height(Length::Fill).width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: list_side_inset,
                bottom: 0.0,
                left: list_side_inset,
            });

                let bottom_footer = container(
            text("↑↓ Row  •  Tab Cycle  •  Alt+←→ Tile  •  Enter Select  •  ESC Dismiss")
                .size(11)
                .shell()
                .color(theme.muted)
                .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: footer_inward,
            bottom: 0.0,
            left: footer_inward,
        });

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon { cut: cut_f, angle_deg })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![
                input_row,
                list_container,
                bottom_footer,
            ]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill),
        )
        .padding(Padding {
            top: pad_top,
            right: 0.0,
            bottom: pad_bottom,
            left: 0.0,
        })
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(LauncherMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.65))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
