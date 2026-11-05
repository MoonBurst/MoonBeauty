use iced::widget::{button, column, container, row, scrollable, text, text_input, Row, Stack};
use iced::{Alignment, Element, Length, Padding};
use std::fs;
use std::path::PathBuf;
use tokio::process::Command;

use super::calc::CalculatorOverlay;
use super::dictionary::{DictionaryOverlay, MeaningEntry};
use super::pass::PassOverlay;
use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::{shell_font, TextExt};

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
    DictLookupReady(String, Vec<MeaningEntry>),
}

pub const LAUNCHER_SEARCH_ID: &str = "launcher_search";

pub struct AppLauncher {
    pub search_query: String,
    pub all_apps: Vec<AppEntry>,
    pub filtered_apps: Vec<AppEntry>,
    pub focus_requested: bool,
    pub selected_index: usize,
    dict_cache: std::collections::HashMap<String, Vec<MeaningEntry>>,
    dict_pending: Option<String>,
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLauncher {
    /// Cheap constructor — no scan. Used by processes that never open the
    /// launcher (notif-surface). Saves ~200 file reads per startup.
    pub fn new_empty() -> Self {
        Self {
            search_query: String::new(),
            all_apps: Vec::new(),
            filtered_apps: Vec::new(),
            focus_requested: false,
            selected_index: 0,
            dict_cache: std::collections::HashMap::new(),
            dict_pending: None,
        }
    }

    pub fn new() -> Self {
        let mut all_apps = vec![
            AppEntry {
                name: "Settings & Options".into(),
                exec: "internal:settings".into(),
                icon: "⚙".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Clipboard Manager".into(),
                exec: "internal:clip".into(),
                icon: "📋".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Quick Notes".into(),
                exec: "internal:notes".into(),
                icon: "📝".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Password Store".into(),
                exec: "internal:pass".into(),
                icon: "🔑".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Power & Session".into(),
                exec: "internal:power".into(),
                icon: "⚡".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Tasks & Todos".into(),
                exec: "internal:todo".into(),
                icon: "✅".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Reminders & Alarms".into(),
                exec: "internal:reminders".into(),
                icon: "⏰".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Unicode Glyphs".into(),
                exec: "internal:unicode".into(),
                icon: "🔣".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Web Search".into(),
                exec: "internal:web".into(),
                icon: "🌐".into(),
                icon_path: None,
            },
            AppEntry {
                name: "Screenshot (Quickshot)".into(),
                exec: "moonbeauty --screenshot --annotate".into(),
                icon: "📷".into(),
                icon_path: None,
            },
        ];
        all_apps.extend(Self::scan_desktop_files());

        Self {
            search_query: String::new(),
            filtered_apps: all_apps.clone(),
            all_apps,
            focus_requested: true,
            selected_index: 0,
            dict_cache: DictionaryOverlay::load_cache(),
            dict_pending: None,
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
        if best_end == 0 {
            return None;
        }
        let num: f64 = q[..best_end].parse().ok()?;
        let rest = q[best_end..].trim();
        if rest.is_empty() {
            return None;
        }
        if rest.contains(char::is_whitespace) {
            return None;
        }
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
        let from_label = match from {
            'C' => "°C",
            'F' => "°F",
            'K' => "K",
            'R' => "°R",
            _ => "?",
        };
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
            if u == from {
                continue;
            }
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

    fn convert_linear(
        value: f64,
        unit: &str,
        units: &[(&str, &str, f64)],
    ) -> Option<Vec<(String, String)>> {
        let (_, from_label, from_factor) = units
            .iter()
            .find(|(k, _, _)| k.eq_ignore_ascii_case(unit))?;
        let base = value * from_factor;
        let mut out = Vec::new();
        for (key, label, factor) in units {
            if key.eq_ignore_ascii_case(unit) {
                continue;
            }
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
            if to_code == upper {
                continue;
            }
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
            ("mm", "mm", 0.001),
            ("cm", "cm", 0.01),
            ("m", "m", 1.0),
            ("km", "km", 1000.0),
            ("in", "in", 0.0254),
            ("ft", "ft", 0.3048),
            ("yd", "yd", 0.9144),
            ("mi", "mi", 1609.344),
            ("nmi", "nmi", 1852.0),
        ];
        if let Some(r) = Self::convert_linear(num, unit, DISTANCE) {
            return Some(r);
        }
        // Weight
        const WEIGHT: &[(&str, &str, f64)] = &[
            ("mg", "mg", 0.000001),
            ("g", "g", 0.001),
            ("kg", "kg", 1.0),
            ("t", "t", 1000.0),
            ("oz", "oz", 0.0283495),
            ("lb", "lb", 0.453592),
            ("st", "st", 6.35029),
        ];
        if let Some(r) = Self::convert_linear(num, unit, WEIGHT) {
            return Some(r);
        }
        // Volume
        const VOLUME: &[(&str, &str, f64)] = &[
            ("ml", "mL", 0.001),
            ("l", "L", 1.0),
            ("cup", "cups", 0.236588),
            ("pt", "pt", 0.473176),
            ("qt", "qt", 0.946353),
            ("gal", "gal", 3.78541),
            ("floz", "fl oz", 0.0295735),
        ];
        if let Some(r) = Self::convert_linear(num, unit, VOLUME) {
            return Some(r);
        }
        // Data
        const DATA: &[(&str, &str, f64)] = &[
            ("b", "B", 1.0),
            ("kb", "KB", 1024.0),
            ("mb", "MB", 1048576.0),
            ("gb", "GB", 1073741824.0),
            ("tb", "TB", 1099511627776.0),
        ];
        if let Some(r) = Self::convert_linear(num, unit, DATA) {
            return Some(r);
        }
        // Time
        const TIME: &[(&str, &str, f64)] = &[
            ("s", "s", 1.0),
            ("min", "min", 60.0),
            ("h", "h", 3600.0),
            ("d", "d", 86400.0),
            ("w", "wk", 604800.0),
            ("y", "yr", 31536000.0),
        ];
        if let Some(r) = Self::convert_linear(num, unit, TIME) {
            return Some(r);
        }
        // Speed
        const SPEED: &[(&str, &str, f64)] = &[
            ("ms", "m/s", 1.0),
            ("kmh", "km/h", 0.277778),
            ("mph", "mph", 0.44704),
            ("kn", "kn", 0.514444),
        ];
        if let Some(r) = Self::convert_linear(num, unit, SPEED) {
            return Some(r);
        }
        // Currency
        if let Some(r) = Self::convert_currency(num, unit) {
            return Some(r);
        }
        None
    }

    fn compute_dict_entries(&self, query: &str) -> Option<Vec<AppEntry>> {
        let q = query.trim_start();
        let lower = q.to_lowercase();

        let word_part = if let Some(rest) = lower.strip_prefix("def ") {
            rest
        } else if let Some(rest) = lower.strip_prefix("define ") {
            rest
        } else if let Some(rest) = lower.strip_prefix("dict ") {
            rest
        } else if let Some(rest) = lower.strip_prefix("dictionary ") {
            rest
        } else if lower == "def" || lower == "define" || lower == "dict" || lower == "dictionary" {
            // Show hint
            return Some(vec![AppEntry {
                name: "Dictionary — type a word after 'def '".to_string(),
                exec: "dict:__hint__".to_string(),
                icon: ">".to_string(),
                icon_path: None,
            }]);
        } else {
            return None;
        };

        let word = word_part.split_whitespace().next().unwrap_or("");
        if word.len() < 2 {
            return Some(vec![AppEntry {
                name: "Dictionary — type a word after 'def '".to_string(),
                exec: "dict:__hint__".to_string(),
                icon: ">".to_string(),
                icon_path: None,
            }]);
        }

        if let Some(defs) = self.dict_cache.get(word) {
            return Some(
                defs.iter()
                    .map(|d| {
                        let truncated = if d.definition.chars().count() > 80 {
                            let mut s: String = d.definition.chars().take(77).collect();
                            s.push_str("...");
                            s
                        } else {
                            d.definition.clone()
                        };
                        AppEntry {
                            name: format!("[{}] {}", d.part_of_speech, truncated),
                            exec: format!("dict:{}", d.definition),
                            icon: ">".to_string(),
                            icon_path: None,
                        }
                    })
                    .collect(),
            );
        }

        // Not cached — show pending placeholder
        Some(vec![AppEntry {
            name: format!("Looking up '{}'...", word),
            exec: "dict:__pending__".to_string(),
            icon: ">".to_string(),
            icon_path: None,
        }])
    }

    fn dict_word_to_lookup(&self, query: &str) -> Option<String> {
        let q = query.trim_start();
        let lower = q.to_lowercase();
        let word_part = lower
            .strip_prefix("def ")
            .or_else(|| lower.strip_prefix("define "))
            .or_else(|| lower.strip_prefix("dict "))
            .or_else(|| lower.strip_prefix("dictionary "))?;
        let word = word_part.split_whitespace().next().unwrap_or("");
        if word.len() < 2 {
            return None;
        }
        if self.dict_cache.contains_key(word) {
            return None;
        }
        if self.dict_pending.as_deref() == Some(word) {
            return None;
        }
        Some(word.to_string())
    }

    /// Cached pass-store entries — reads the filesystem once per launcher session.
    fn get_pass_keys() -> &'static Vec<String> {
        static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        CACHE.get_or_init(PassOverlay::scan_passwords)
    }

    /// If the query starts with a pass-prefix keyword (`pass`, `passw`, `password`,
    /// `pw`, `passwd`, `cred`, `creds`), return all matching entries.
    /// Query `pass git` → entries containing "git".
    /// Query `pass` alone → all entries (capped at 30).
    fn compute_pass_entries(query: &str) -> Option<Vec<(String, String)>> {
        let q = query.trim().to_lowercase();
        let filter_term: &str = if matches!(
            q.as_str(),
            "pass"
                | "passw"
                | "password"
                | "pw"
                | "passwd"
                | "cred"
                | "creds"
                | "credential"
                | "credentials"
                | "secret"
                | "secrets"
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
        } else {
            let rest = q.strip_prefix("secrets ")?;
            rest.trim()
        };

        let keys = Self::get_pass_keys();
        let results: Vec<(String, String)> = keys
            .iter()
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
        if q.is_empty() {
            return vec![];
        }
        let has_operator = q.contains('+')
            || q.contains('-')
            || q.contains('*')
            || q.contains('/')
            || q.contains('(');
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
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());

        let mut dirs: Vec<PathBuf> = vec![
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
            PathBuf::from("/run/current-system/sw/share/applications"),
            PathBuf::from("/etc/profiles/per-user")
                .join(&user)
                .join("share/applications"),
            home.join(".nix-profile/share/applications"),
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
            home.join(".local/share/flatpak/exports/share/applications"),
            PathBuf::from("/run/flatpak/exports/share/applications"),
            PathBuf::from("/var/lib/snapd/desktop/applications"),
            home.join(".local/share/applications"),
        ];

        if let Ok(xdg) = std::env::var("XDG_DATA_DIRS") {
            for d in xdg.split(':') {
                if !d.is_empty() {
                    dirs.push(PathBuf::from(d).join("applications"));
                }
            }
        }

        let debug = std::env::var_os("MBS_LAUNCHER_DEBUG").is_some();
        let current_de = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase();

        let mut seen = std::collections::HashSet::new();
        let mut total_files = 0usize;
        let mut hidden = 0usize;
        let mut no_exec = 0usize;
        let mut de_filtered = 0usize;

        for dir in &dirs {
            if !dir.exists() {
                continue;
            }
            let entries = match fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut dir_count = 0usize;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                    continue;
                }
                total_files += 1;
                dir_count += 1;

                let Ok(content) = fs::read_to_string(&path) else {
                    continue;
                };

                let mut name: Option<String> = None;
                let mut exec: Option<String> = None;
                let mut icon_name: Option<String> = None;
                let mut nodisplay = false;
                let mut hidden_flag = false;
                let mut only_show_in = String::new();
                let mut not_show_in = String::new();
                let mut try_exec: Option<String> = None;
                let mut in_main_section = false;

                for line in content.lines() {
                    let line = line.trim();
                    if line.starts_with('[') {
                        in_main_section = line.eq_ignore_ascii_case("[Desktop Entry]");
                        continue;
                    }
                    if !in_main_section {
                        continue;
                    }
                    if let Some(v) = line.strip_prefix("Name=") {
                        if name.is_none() {
                            // Finding 45: cap Name before render.
                            name = Some(v.trim().chars().take(256).collect::<String>());
                        }
                    } else if let Some(v) = line.strip_prefix("Exec=") {
                        if exec.is_none() {
                            let raw = v.trim();
                            // Finding 39: spec-compliant %-code stripping
                            // (previous split('%').next() lost everything
                            // after the first %, breaking multi-arg Exec).
                            let cleaned = Self::strip_field_codes(raw).trim().to_string();
                            // Finding 45: cap Exec at 4 KB before render.
                            exec = Some(cleaned.chars().take(4096).collect::<String>());
                        }
                    } else if let Some(v) = line.strip_prefix("Icon=") {
                        if icon_name.is_none() {
                            // Finding 45: cap Icon before passing to resolve_icon.
                            icon_name = Some(v.trim().chars().take(128).collect::<String>());
                        }
                    } else if let Some(v) = line.strip_prefix("OnlyShowIn=") {
                        only_show_in = v.trim().to_lowercase();
                    } else if let Some(v) = line.strip_prefix("NotShowIn=") {
                        not_show_in = v.trim().to_lowercase();
                    } else if let Some(v) = line.strip_prefix("TryExec=") {
                        if try_exec.is_none() {
                            try_exec = Some(v.trim().to_string());
                        }
                    } else if line.eq_ignore_ascii_case("NoDisplay=true") {
                        nodisplay = true;
                    } else if line.eq_ignore_ascii_case("Hidden=true") {
                        hidden_flag = true;
                    }
                }

                if nodisplay || hidden_flag {
                    hidden += 1;
                    if debug {
                        eprintln!(
                            "[launcher-scan]   skip hidden: {:?}",
                            path.file_name().unwrap_or_default()
                        );
                    }
                    continue;
                }

                if !only_show_in.is_empty() && !current_de.is_empty() {
                    let allowed = only_show_in
                        .split(';')
                        .any(|d| !d.trim().is_empty() && current_de.contains(d.trim()));
                    if !allowed {
                        de_filtered += 1;
                        if debug {
                            eprintln!(
                                "[launcher-scan]   skip OnlyShowIn: {:?}",
                                path.file_name().unwrap_or_default()
                            );
                        }
                        continue;
                    }
                }
                if !not_show_in.is_empty() && !current_de.is_empty() {
                    let denied = not_show_in
                        .split(';')
                        .any(|d| !d.trim().is_empty() && current_de.contains(d.trim()));
                    if denied {
                        de_filtered += 1;
                        if debug {
                            eprintln!(
                                "[launcher-scan]   skip NotShowIn: {:?}",
                                path.file_name().unwrap_or_default()
                            );
                        }
                        continue;
                    }
                }

                if let Some(te) = &try_exec {
                    let te_path = std::path::Path::new(te);
                    if te_path.is_absolute() && !te_path.exists() {
                        no_exec += 1;
                        if debug {
                            eprintln!(
                                "[launcher-scan]   skip TryExec missing: {:?} -> {}",
                                path.file_name().unwrap_or_default(),
                                te
                            );
                        }
                        continue;
                    }
                }

                if let (Some(n), Some(e)) = (name.clone(), exec.clone()) {
                    if e.is_empty() {
                        no_exec += 1;
                        continue;
                    }
                    let key = format!("{}|{}", n.to_lowercase(), e.to_lowercase());
                    if !seen.contains(&key) {
                        seen.insert(key);
                        let icon_path = icon_name.as_deref().and_then(Self::resolve_icon);
                        apps.push(AppEntry {
                            name: n,
                            exec: e,
                            icon: "🚀".to_string(),
                            icon_path,
                        });
                        if debug {
                            eprintln!(
                                "[launcher-scan]   ✓ {:?}",
                                path.file_name().unwrap_or_default()
                            );
                        }
                    }
                } else {
                    no_exec += 1;
                }
            }
            if debug && dir_count > 0 {
                eprintln!("[launcher-scan] {:?}: {} files", dir, dir_count);
            }
        }

        apps.sort_by_key(|a| a.name.to_lowercase());
        let with_icons = apps.iter().filter(|a| a.icon_path.is_some()).count();

        if (debug || std::env::var_os("MBS_DEBUG").is_some()) && total_files != 63 {
            eprintln!(
                "[launcher] {} .desktop files across {} dirs, {} hidden, {} tryExec-missing, {} de-filtered, {} apps, {} with icons",
                total_files, dirs.len(), hidden, no_exec, de_filtered, apps.len(), with_icons
            );
        }

        apps
    }

    fn resolve_icon(icon_name: &str) -> Option<String> {
        use std::path::Path;
        if icon_name.is_empty() {
            return None;
        }
        // Finding 08: same whitelist as notifications/ui.rs::resolve_icon_name.
        // .desktop Icon= values are attacker-controlled via
        // ~/.local/share/applications/.
        if icon_name.starts_with('/') {
            let known = [
                "/usr/share/icons/",
                "/usr/share/pixmaps/",
                "/usr/local/share/icons/",
                "/usr/local/share/pixmaps/",
                "/run/current-system/sw/share/icons/",
                "/run/current-system/sw/share/pixmaps/",
                "/var/lib/flatpak/exports/share/icons/",
                "/var/lib/flatpak/exports/share/pixmaps/",
                "/var/lib/snapd/desktop/icons/",
            ];
            let mut ok = known.iter().any(|p| icon_name.starts_with(p));
            if !ok {
                if let Some(home) = dirs::home_dir() {
                    let h = home.to_string_lossy();
                    ok = icon_name.starts_with(&format!("{}/.local/share/icons/", h))
                        || icon_name.starts_with(&format!("{}/.nix-profile/share/icons/", h))
                        || icon_name.starts_with(&format!(
                            "{}/.local/share/flatpak/exports/share/icons/",
                            h
                        ));
                }
            }
            if !ok {
                crate::dbg_log!(
                    "[launcher-icon] reject absolute icon outside known roots: {:?}",
                    icon_name
                );
                return None;
            }
            return if Path::new(icon_name).exists() {
                Some(icon_name.to_string())
            } else {
                None
            };
        }
        if icon_name.contains("..") || icon_name.contains('/') || icon_name.contains('\\') {
            crate::dbg_log!("[launcher-icon] reject unsafe icon name: {:?}", icon_name);
            return None;
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let roots = [
            PathBuf::from("/run/current-system/sw/share/icons/hicolor"),
            PathBuf::from("/usr/share/icons/hicolor"),
            PathBuf::from("/usr/local/share/icons/hicolor"),
            home.join(".local/share/icons/hicolor"),
            home.join(".nix-profile/share/icons/hicolor"),
        ];
        let sizes = [
            "48x48", "64x64", "128x128", "256x256", "512x512", "scalable", "symbolic",
        ];
        let exts = ["png", "svg", "xpm"];
        for root in &roots {
            for size in &sizes {
                for ext in &exts {
                    let p = root
                        .join(size)
                        .join("apps")
                        .join(format!("{}.{}", icon_name, ext));
                    if p.exists() {
                        // Finding 40: canonicalize + root check, same as
                        // tray/watcher.rs (finding 27). Without this, a
                        // symlink in ~/.local/share/icons/ → /etc/passwd
                        // is followed and rendered.
                        if let Some(canon) =
                            crate::libraries::process::safe_icon_candidate(&p, root)
                        {
                            return Some(canon.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        for pix in ["/run/current-system/sw/share/pixmaps", "/usr/share/pixmaps"] {
            let pix_root = PathBuf::from(pix);
            for ext in &exts {
                let p = pix_root.join(format!("{}.{}", icon_name, ext));
                if p.exists() {
                    if let Some(canon) =
                        crate::libraries::process::safe_icon_candidate(&p, &pix_root)
                    {
                        return Some(canon.to_string_lossy().into_owned());
                    }
                }
            }
        }
        None
    }

    pub fn update(&mut self, message: LauncherMessage) -> iced::Task<LauncherMessage> {
        match message {
            LauncherMessage::Refocus => {
                return iced::widget::operation::focus(LAUNCHER_SEARCH_ID);
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
            LauncherMessage::DictLookupReady(word, defs) => {
                eprintln!(
                    "[launcher-dict] lookup done: {:?} → {} definitions",
                    word,
                    defs.len()
                );
                self.dict_cache.insert(word.clone(), defs);
                if self.dict_pending.as_deref() == Some(&word) {
                    self.dict_pending = None;
                }
                let q = self.search_query.clone();
                return self.update(LauncherMessage::SearchChanged(q));
            }
            LauncherMessage::SelectDown => {
                if !self.filtered_apps.is_empty() {
                    let math_count = self
                        .filtered_apps
                        .iter()
                        .filter(|a| a.exec.starts_with("math:"))
                        .count();
                    // Grid row = +3, app list = +1
                    let step = if math_count > 0 && self.selected_index < math_count {
                        3
                    } else {
                        1
                    };
                    self.selected_index =
                        (self.selected_index + step).min(self.filtered_apps.len() - 1);
                }
                return iced::Task::none();
            }
            LauncherMessage::SelectUp => {
                if !self.filtered_apps.is_empty() {
                    let math_count = self
                        .filtered_apps
                        .iter()
                        .filter(|a| a.exec.starts_with("math:"))
                        .count();
                    let step = if math_count > 0 && self.selected_index < math_count {
                        3
                    } else {
                        1
                    };
                    self.selected_index = self.selected_index.saturating_sub(step);
                }
                return iced::Task::none();
            }
            LauncherMessage::SearchChanged(q) => {
                crate::dbg_log!("[launcher-input] {:?}", q);
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

                // Dictionary search — takes priority if the query starts with 'def ', 'dict ', etc.
                if let Some(dict_entries) = self.compute_dict_entries(&q) {
                    crate::dbg_log!("[launcher-dict] {:?} → {} entries", q, dict_entries.len());
                    let mut dict_items = dict_entries;
                    dict_items.append(&mut filtered);
                    filtered = dict_items;

                    // Fire lookup if word not cached and not pending
                    if let Some(word) = self.dict_word_to_lookup(&q) {
                        self.dict_pending = Some(word.clone());
                        self.filtered_apps = filtered;
                        let w1 = word.clone();
                        let w2 = word.clone();
                        return iced::Task::perform(
                            async move { DictionaryOverlay::lookup_word(&w1).await },
                            move |defs| LauncherMessage::DictLookupReady(w2.clone(), defs),
                        );
                    }
                }

                // Password store search — takes priority if the query starts with a pass-prefix
                if let Some(pass_entries) = Self::compute_pass_entries(&q) {
                    crate::dbg_log!("[launcher-pass] {:?} → {} entries", q, pass_entries.len());
                    let mut pass_items: Vec<AppEntry> = pass_entries
                        .into_iter()
                        .map(|(name, exec)| AppEntry {
                            name,
                            exec,
                            icon: ">".to_string(),
                            icon_path: None,
                        })
                        .collect();
                    pass_items.append(&mut filtered);
                    filtered = pass_items;
                }

                // Multi-conversion + math result at top of list
                let conversions = Self::compute_multi_conversions(&q);
                if !conversions.is_empty() {
                    crate::dbg_log!("[launcher-multi] {:?} → {} results", q, conversions.len());
                    let mut math_entries: Vec<AppEntry> = conversions
                        .into_iter()
                        .map(|(label, exec)| AppEntry {
                            name: label,
                            exec,
                            icon: "=".to_string(),
                            icon_path: None,
                        })
                        .collect();
                    math_entries.append(&mut filtered);
                    filtered = math_entries;
                } else {
                    crate::dbg_log!("[launcher-multi] {:?} → 0 results", q);
                }

                crate::dbg_log!("[launcher-final] {} entries:", filtered.len());
                if std::env::var_os("MBS_DEBUG").is_some() {
                    for (i, e) in filtered.iter().enumerate().take(5) {
                        eprintln!("  [{}] {} -> {}", i, e.name, e.exec);
                    }
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
            "cfg" | "set" | "settings" | "config" | "prefs" | "preferences" | "options"
            | "settings & options" | "preference" | "configuration" => {
                ("Settings & Options", "internal:settings")
            }
            // Clipboard
            "clip" | "clipboard" | "clipboard manager" | "clipboard history" | "paste"
            | "pastebin" => ("Clipboard Manager", "internal:clip"),
            // Todo
            "todo" | "td" | "task" | "tasks" | "tasks & todos" | "task list" => {
                ("Tasks & Todos", "internal:todo")
            }
            // Notes
            "note" | "notes" | "quick notes" | "quicknote" => ("Quick Notes", "internal:notes"),
            // Power
            "power" | "pwr" | "session" | "power & session" | "logout" | "reboot" | "shutdown"
            | "lock" | "lockscreen" => ("Power & Session", "internal:power"),
            // Reminders
            "remind" | "reminder" | "reminders" | "alarm" | "alarms" | "reminders & alarms"
            | "timer" | "timers" => ("Reminders & Alarms", "internal:reminders"),
            // Unicode
            "unicode" | "glyph" | "glyphs" | "symbol" | "symbols" | "emoji" | "emojis"
            | "unicode glyphs" => ("Unicode", "internal:unicode"),
            // Email
            "email" | "mail" | "em" | "mail client" | "inbox" => ("Email", "internal:email"),
            // RNG / Dice
            "rng" | "dice" | "roll" | "random" | "rand" | "d20" => ("RNG / Dice", "internal:rng"),
            // Web search
            "web" | "search" | "web search" | "google" | "ddg" | "duckduckgo" => {
                ("Web Search", "internal:web")
            }
            // Amogus
            "amogus" | "amongus" | "among us" | "sus" => ("Among Us", "internal:amogus"),
            // Screenshot
            "shot" | "screenshot" | "capture" | "screenshot (quickshot)" => {
                ("Screenshot", "moonbeauty --screenshot --annotate")
            }
            _ => return None,
        };
        Some(AppEntry {
            name: name.into(),
            exec: exec.into(),
            icon: ">".into(),
            icon_path: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn parse_exec_for_test(exec: &str) -> Option<(String, Vec<String>)> {
        Self::parse_exec(exec)
    }

    #[cfg(test)]
    pub(crate) fn strip_field_codes_for_test(raw: &str) -> String {
        Self::strip_field_codes(raw)
    }
    /// Finding 39: strip .desktop %-field-codes per spec. Only %%
    /// survives (as literal %); everything else (%f %F %u %U %i %c %k
    /// plus deprecated variants) is dropped. Previously this used
    /// `raw.split('%').next()`, which lost every character after the
    /// first %, silently breaking `Exec=app %f --flag`.
    fn strip_field_codes(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut it = raw.chars().peekable();
        while let Some(c) = it.next() {
            if c == '%' {
                match it.next() {
                    Some('%') => out.push('%'),
                    Some(_) => {} // drop %f/%F/%u/%U/%i/%c/%k/%d/%D/%n/%N/%v/%m
                    None => out.push('%'),
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Finding 39: tokenize a .desktop Exec line without invoking a
    /// shell. Implements the subset of the spec that real .desktop
    /// files use: whitespace separation, single/double quotes, and
    /// backslash escape outside quotes. Rejects unbalanced quotes by
    /// returning None (caller logs and refuses to launch).
    fn parse_exec(exec: &str) -> Option<(String, Vec<String>)> {
        let mut tokens: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut it = exec.chars().peekable();
        while let Some(c) = it.next() {
            match (c, in_single, in_double) {
                ('\\', false, false) => {
                    if let Some(&n) = it.peek() {
                        cur.push(n);
                        it.next();
                    } else {
                        cur.push('\\');
                    }
                }
                ('\'', false, false) => in_single = true,
                ('\'', true, _) => in_single = false,
                ('"', false, false) => in_double = true,
                ('"', _, true) => in_double = false,
                (' ', false, false) | ('\t', false, false) => {
                    if !cur.is_empty() {
                        tokens.push(std::mem::take(&mut cur));
                    }
                }
                _ => cur.push(c),
            }
        }
        if in_single || in_double {
            return None;
        }
        if !cur.is_empty() {
            tokens.push(cur);
        }
        let mut tok_it = tokens.into_iter();
        let prog = tok_it.next()?;
        Some((prog, tok_it.collect()))
    }

    pub async fn launch(exec: &str) {
        if exec.is_empty() {
            return;
        }
        // Finding 39: NEVER invoke a shell on .desktop Exec content.
        // Tokenize + exec directly. A hostile .desktop with
        // `Exec=foo; rm -rf ~` now becomes program="foo" with args
        // [";", "rm", "-rf", "~"] — shell metachars are inert.
        let Some((prog, args)) = Self::parse_exec(exec) else {
            eprintln!("[launcher] refusing to launch: unparseable Exec {:?}", exec);
            return;
        };
        let _ = Command::new("systemd-run")
            .args(["--user", "--collect", "--quiet", "--slice=app.slice", "--"])
            .arg(&prog)
            .args(&args)
            .spawn();
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, LauncherMessage> {
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
            .size((theme.font_size_overlay) as f32)
            .width(Length::Fill)
            .style(move |_t, status| {
                let border_color = match status {
                    text_input::Status::Focused { .. } => theme.focus,
                    text_input::Status::Hovered => theme.border,
                    _ => theme.border,
                };
                let border_width = match status {
                    text_input::Status::Focused { .. } => theme.control_border_width + 0.5,
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

        let input_row = container(input).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: search_inward,
            bottom: 0.0,
            left: search_inward,
        });

        // Split math results from apps — math gets hex grid, apps get list
        let math_count = self
            .filtered_apps
            .iter()
            .filter(|a| a.exec.starts_with("math:"))
            .count();
        let math_items: Vec<&AppEntry> = self
            .filtered_apps
            .iter()
            .filter(|a| a.exec.starts_with("math:"))
            .take(40)
            .collect();
        let app_items: Vec<&AppEntry> = self
            .filtered_apps
            .iter()
            .filter(|a| !a.exec.starts_with("math:"))
            .take(40)
            .collect();

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

                    let bg = if is_selected {
                        theme.surface_alt
                    } else {
                        theme.background
                    };
                    let border_c = if is_selected {
                        theme.text
                    } else {
                        theme.border
                    };

                    let tile_bg = ShapeBox::new()
                        .shape(ShapeType::Hexagon { cut: 8 })
                        .background(bg)
                        .border(border_c, theme.control_border_width)
                        .size(Length::Fixed(tile_w), Length::Fixed(tile_h));

                    let tile_content = container(
                        column![
                            text(copy_val)
                                .size((theme.font_size_overlay + 1) as f32)
                                .bold()
                                .color(theme.text_strong)
                                .align_x(Alignment::Center)
                                .wrapping(iced::widget::text::Wrapping::None),
                            text(input_part)
                                .size((theme.font_size_normal.saturating_sub(2)) as f32)
                                .color(theme.muted)
                                .align_x(Alignment::Center)
                                .wrapping(iced::widget::text::Wrapping::None),
                        ]
                        .spacing(2)
                        .align_x(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .padding([0, 20]);

                    let tile_inner =
                        container(Stack::new().push(tile_bg.view()).push(tile_content))
                            .width(Length::Fixed(tile_w))
                            .height(Length::Fixed(tile_h))
                            .clip(true);

                    let tile = button(tile_inner)
                        .on_press(LauncherMessage::ExecuteApp(exec_cmd))
                        .padding(0)
                        .width(Length::Fixed(tile_w))
                        .height(Length::Fixed(tile_h))
                        .style(|_t, _s| button::Style {
                            background: None,
                            ..Default::default()
                        });

                    grid_row = grid_row.push(tile);
                }
                for _ in chunk.len()..3 {
                    grid_row = grid_row.push(
                        container("")
                            .width(Length::Fixed(tile_w))
                            .height(Length::Fixed(tile_h)),
                    );
                }
                grid_col = grid_col.push(grid_row);
            }
            content_col = content_col.push(grid_col);
        }

        // App rows (unchanged logic, but global index offset by math_count)
        if !app_items.is_empty() {
            // One app per row. Each row is an extended hexagon "bar"
            // containing the icon (in its own small hexagon) on the
            // left and the app name + exec path on the right.
            let gap = 12.0;
            let inner_pad_x = 20.0;
            let inner_pad_y = 10.0;

            // Icon size comes from Settings → Overlays & Windows.
            // The hexagon must be big enough that its cut corners don't
            // clip the icon's own corners.
            //   interior_width = hex_size - 2*cut
            //   need interior_width ≥ icon_size
            //   cut = 10% of hex_size  →  interior = 0.80 * hex_size
            //   so hex_size ≥ icon_size / 0.80 = 1.25 * icon_size.
            // Use 1.40 to give visible margin + clamp cut to ≥ 8px so
            // it stays visibly octagonal at the smallest slider values.
            let icon_size = settings.launcher_icon_size as f32;
            let icon_hex_size = icon_size * 1.40;
            let icon_hex_cut: u16 = ((icon_hex_size * 0.10) as u16).max(8);
            let row_h = icon_hex_size + inner_pad_y * 2.0;

            let row_w = win_w - list_side_inset * 2.0;

            let mut grid_col = column![].spacing(gap);

            for (idx_offset, app) in app_items.iter().enumerate() {
                let idx = math_count + idx_offset;
                let exec_cmd = app.exec.clone();
                let is_selected = idx == self.selected_index;

                // Row-level extended hexagon bar
                let row_bg = ShapeBox::new()
                    .shape(ShapeType::Hexagon { cut: 18 })
                    .background(if is_selected {
                        theme.surface_alt
                    } else {
                        theme.background
                    })
                    .border(
                        if is_selected {
                            theme.text
                        } else {
                            theme.border
                        },
                        theme.control_border_width,
                    )
                    .size(Length::Fixed(row_w), Length::Fixed(row_h));

                // Icon rendering (SVG-aware)
                let icon_elem: Element<'_, LauncherMessage> = if let Some(ref path) = app.icon_path
                {
                    if path.ends_with(".svg") {
                        iced::widget::svg(iced::widget::svg::Handle::from_path(path.clone()))
                            .width(Length::Fixed(icon_size))
                            .height(Length::Fixed(icon_size))
                            .into()
                    } else {
                        iced::widget::image(iced::widget::image::Handle::from_path(path.clone()))
                            .width(Length::Fixed(icon_size))
                            .height(Length::Fixed(icon_size))
                            .content_fit(iced::ContentFit::Contain)
                            .into()
                    }
                } else {
                    container(text(&app.icon).size(64))
                        .width(Length::Fixed(icon_size))
                        .height(Length::Fixed(icon_size))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .into()
                };

                // Small hexagon wrapping the icon
                let icon_bg = ShapeBox::new()
                    .shape(ShapeType::Hexagon { cut: icon_hex_cut })
                    .background(theme.background)
                    .border(
                        if is_selected {
                            theme.text
                        } else {
                            theme.border
                        },
                        theme.control_border_width,
                    )
                    .size(Length::Fixed(icon_hex_size), Length::Fixed(icon_hex_size));

                let icon_stack = Stack::new().push(icon_bg.view()).push(
                    container(icon_elem)
                        .width(Length::Fixed(icon_hex_size))
                        .height(Length::Fixed(icon_hex_size))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                );

                // Name + exec on the right
                let name_txt = text(&app.name).size(18.0).bold().color(if is_selected {
                    theme.text_strong
                } else {
                    theme.text
                });

                let exec_txt = text(&app.exec)
                    .size(12.0)
                    .font(iced::Font::MONOSPACE)
                    .color(theme.muted);

                let info_col = column![name_txt, exec_txt]
                    .spacing(4)
                    .align_x(Alignment::Start)
                    .width(Length::Fill);

                let row_inner = row![
                    icon_stack,
                    container(info_col)
                        .width(Length::Fill)
                        .padding(iced::Padding {
                            top: 0.0,
                            right: 20.0,
                            bottom: 0.0,
                            left: 20.0,
                        }),
                ]
                .spacing(0)
                .align_y(iced::alignment::Vertical::Center)
                .width(Length::Fill);

                let row_content = container(row_inner)
                    .width(Length::Fixed(row_w))
                    .height(Length::Fixed(row_h))
                    .padding(iced::Padding {
                        top: inner_pad_y,
                        right: inner_pad_x,
                        bottom: inner_pad_y,
                        left: inner_pad_x,
                    });

                let row_stack = Stack::new().push(row_bg.view()).push(row_content);

                let row_btn = button(row_stack)
                    .on_press(LauncherMessage::ExecuteApp(exec_cmd))
                    .padding(0)
                    .width(Length::Fixed(row_w))
                    .height(Length::Fixed(row_h))
                    .style(|_t, _s| button::Style {
                        background: None,
                        ..Default::default()
                    });

                grid_col = grid_col.push(row_btn);
            }

            content_col = content_col.push(grid_col);
        }

        if math_items.is_empty() && app_items.is_empty() {
            content_col = content_col.push(
                container(
                    text("No results")
                        .size((theme.font_size_overlay) as f32)
                        .color(theme.muted),
                )
                .width(Length::Fill)
                .center_x(Length::Fill)
                .padding(20),
            );
        }

        let list_container = container(
            scrollable(content_col)
                .height(Length::Fill)
                .width(Length::Fill),
        )
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
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        let card_content = container(
            column![input_row, list_container, bottom_footer,]
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

        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod parse_exec_tests {
    use super::AppLauncher;

    #[test]
    fn simple_command() {
        let (prog, args) = AppLauncher::parse_exec_for_test("firefox").unwrap();
        assert_eq!(prog, "firefox");
        assert!(args.is_empty());
    }

    #[test]
    fn command_with_args() {
        let (prog, args) = AppLauncher::parse_exec_for_test("firefox --new-window").unwrap();
        assert_eq!(prog, "firefox");
        assert_eq!(args, vec!["--new-window"]);
    }

    #[test]
    fn double_quoted_argument() {
        let (prog, args) = AppLauncher::parse_exec_for_test(r#"foo "bar baz""#).unwrap();
        assert_eq!(prog, "foo");
        assert_eq!(args, vec!["bar baz"]);
    }

    #[test]
    fn single_quoted_argument() {
        let (prog, args) = AppLauncher::parse_exec_for_test("foo 'bar baz'").unwrap();
        assert_eq!(prog, "foo");
        assert_eq!(args, vec!["bar baz"]);
    }

    #[test]
    fn shell_metachars_are_literal_args() {
        // Finding 39 regression: ; | & $ ( ) must NOT be interpreted.
        let (prog, args) = AppLauncher::parse_exec_for_test("foo; rm -rf ~").unwrap();
        assert_eq!(prog, "foo;");
        assert_eq!(args, vec!["rm", "-rf", "~"]);
    }

    #[test]
    fn dollar_command_substitution_literal() {
        let (prog, args) = AppLauncher::parse_exec_for_test("firefox $(id)").unwrap();
        assert_eq!(prog, "firefox");
        assert_eq!(args, vec!["$(id)"]);
    }

    #[test]
    fn unbalanced_quote_rejected() {
        assert!(AppLauncher::parse_exec_for_test(r#"foo "bar"#).is_none());
        assert!(AppLauncher::parse_exec_for_test("foo 'bar").is_none());
    }

    #[test]
    fn empty_returns_none() {
        assert!(AppLauncher::parse_exec_for_test("").is_none());
        assert!(AppLauncher::parse_exec_for_test("   ").is_none());
    }

    #[test]
    fn backslash_escape() {
        let (prog, args) = AppLauncher::parse_exec_for_test(r"foo\bar").unwrap();
        assert_eq!(prog, "foobar");
        assert!(args.is_empty());
    }

    #[test]
    fn percent_codes_stripped() {
        assert_eq!(
            AppLauncher::strip_field_codes_for_test("firefox %u"),
            "firefox "
        );
        assert_eq!(
            AppLauncher::strip_field_codes_for_test("app %f --flag"),
            "app  --flag"
        );
        assert_eq!(
            AppLauncher::strip_field_codes_for_test("echo 100%%"),
            "echo 100%"
        );
    }
}
