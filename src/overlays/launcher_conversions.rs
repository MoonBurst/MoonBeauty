use super::launcher::AppEntry;
use super::*;
use crate::overlays::calc::CalculatorOverlay;

impl AppLauncher {
    pub(super) fn parse_number_unit(query: &str) -> Option<(f64, String)> {
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

    pub(super) fn fmt_num(v: f64) -> String {
        if v.fract().abs() < 0.001 {
            format!("{:.0}", v)
        } else {
            let s = format!("{:.4}", v);
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    }

    pub(super) fn convert_temp(value: f64, from: char) -> Vec<(String, String)> {
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

    pub(super) fn convert_linear(
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

    pub(super) fn convert_currency(value: f64, code: &str) -> Option<Vec<(String, String)>> {
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
    pub(super) fn get_rates() -> &'static std::collections::HashMap<String, f64> {
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

    pub(super) fn try_convert(num: f64, unit: &str) -> Option<Vec<(String, String)>> {
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

    pub(super) fn compute_dict_entries(&self, query: &str) -> Option<Vec<AppEntry>> {
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

    pub(super) fn dict_word_to_lookup(&self, query: &str) -> Option<String> {
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
    pub(super) fn get_pass_keys() -> &'static Vec<String> {
        static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        CACHE.get_or_init(PassOverlay::scan_passwords)
    }

    /// If the query starts with a pass-prefix keyword (`pass`, `passw`, `password`,
    /// `pw`, `passwd`, `cred`, `creds`), return all matching entries.
    /// Query `pass git` → entries containing "git".
    /// Query `pass` alone → all entries (capped at 30).
    pub(super) fn compute_pass_entries(query: &str) -> Option<Vec<(String, String)>> {
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
    pub(super) fn compute_multi_conversions(query: &str) -> Vec<(String, String)> {
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
}
