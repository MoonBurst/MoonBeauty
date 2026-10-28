use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeatherAlert {
    None,
    Upcoming(String),
    Danger(String),
}

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temp_f: i32,
    pub temp_c: i32,
    pub icon: &'static str,
    pub desc: &'static str,
    pub alert: WeatherAlert,
    pub tooltip_lines: Vec<String>,
}

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: Option<CurrentMeteo>,
    hourly: Option<HourlyMeteo>,
}

#[derive(Deserialize)]
struct CurrentMeteo {
    time: Option<String>,
    temperature_2m: Option<f32>,
    weather_code: Option<u16>,
    wind_speed_10m: Option<f32>,
}

#[derive(Deserialize)]
struct HourlyMeteo {
    time: Vec<String>,
    temperature_2m: Vec<f32>,
    weather_code: Vec<u16>,
    precipitation_probability: Option<Vec<u8>>,
    precipitation: Option<Vec<f32>>,
    wind_speed_10m: Option<Vec<f32>>,
}

pub struct WeatherBackend;

impl WeatherBackend {
    pub async fn fetch() -> Result<WeatherData, Box<dyn std::error::Error>> {
        let (lat, lon) = Self::detect_coords().await;
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,wind_speed_10m&hourly=temperature_2m,precipitation_probability,precipitation,weather_code,wind_speed_10m&forecast_days=2&timezone=auto",
            lat, lon
        );

        let output = Command::new("curl")
            .args(["-s", "--max-time", "6", &url])
            .output()
            .await?;

        if !output.status.success() {
            return Err("curl failed".into());
        }

        let res: OpenMeteoResponse = serde_json::from_slice(&output.stdout)?;
        let cur = res.current.ok_or("No current")?;
        let hourly = res.hourly.ok_or("No hourly")?;

        let cur_c = cur.temperature_2m.unwrap_or(20.0).round() as i32;
        let cur_f = (cur_c as f32 * 9.0 / 5.0 + 32.0).round() as i32;
        let (cur_desc, cur_icon) = Self::wmo_info(cur.weather_code.unwrap_or(0));
        let cur_wind = cur.wind_speed_10m.unwrap_or(0.0) as i32;

        let now_iso = cur
            .time
            .as_deref()
            .unwrap_or("")
            .get(0..13)
            .unwrap_or("")
            .to_string();
        let start_idx = hourly
            .time
            .iter()
            .position(|t| t.get(0..13) == Some(&now_iso))
            .unwrap_or(0);

        let mut alert = WeatherAlert::None;
        let mut lines = Vec::new();

        // Format "Now" line
        lines.push(format!(
            "Now:      {} {}°F / {}°C, {} (Wind {}km/h)",
            cur_icon, cur_f, cur_c, cur_desc, cur_wind
        ));

        // Scan 24h and format lines
        for offset in 1..=24 {
            let idx = start_idx + offset;
            if idx >= hourly.time.len() {
                break;
            }

            let tc = hourly
                .temperature_2m
                .get(idx)
                .copied()
                .unwrap_or(20.0)
                .round() as i32;
            let tf = (tc as f32 * 9.0 / 5.0 + 32.0).round() as i32;
            let code = hourly.weather_code.get(idx).copied().unwrap_or(0);
            let (desc, icon) = Self::wmo_info(code);
            let prob = hourly
                .precipitation_probability
                .as_ref()
                .and_then(|p| p.get(idx).copied())
                .unwrap_or(0);
            let p_mm = hourly
                .precipitation
                .as_ref()
                .and_then(|p| p.get(idx).copied())
                .unwrap_or(0.0);
            let wind = hourly
                .wind_speed_10m
                .as_ref()
                .and_then(|w| w.get(idx).copied())
                .unwrap_or(0.0);

            // Evaluate hazards (ported from Lua/Python)
            let (hazard, cause) = Self::evaluate_hazard(tf, code, p_mm, prob, wind);

            // Update global alert
            if hazard == "danger" {
                if offset <= 1 {
                    alert = WeatherAlert::Danger(cause.clone());
                } else if offset <= 3 && alert == WeatherAlert::None {
                    alert = WeatherAlert::Upcoming(format!("{} in {}h", cause, offset));
                }
            } else if hazard == "warning" && offset <= 3 && alert == WeatherAlert::None {
                alert = WeatherAlert::Upcoming(format!("{} in {}h", cause, offset));
            }

            let t_raw = &hourly.time[idx];
            let hour_str = t_raw.get(11..16).unwrap_or("12:00");
            let prefix = if hazard == "danger" {
                "⚠ "
            } else if hazard == "warning" {
                "· "
            } else {
                ""
            };

            lines.push(format!(
                "{}: {}{} {}°F / {}°C, {} ({}%)",
                hour_str, prefix, icon, tf, tc, desc, prob
            ));
        }

        Ok(WeatherData {
            temp_f: cur_f,
            temp_c: cur_c,
            icon: cur_icon,
            desc: cur_desc,
            alert,
            tooltip_lines: lines,
        })
    }

    async fn detect_coords() -> (String, String) {
        // ip-api.com's free tier is HTTP-only (HTTPS is a paid upgrade).
        // Use ipapi.co (HTTPS on free tier) so a network attacker can't
        // inject a fake lat/lon. Response keys differ: latitude/longitude.
        if let Ok(out) = Command::new("curl")
            .args(["-s", "--max-time", "3", "https://ipapi.co/json/"])
            .output()
            .await
        {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let (Some(lat), Some(lon)) =
                    (val["latitude"].as_f64(), val["longitude"].as_f64())
                {
                    return (lat.to_string(), lon.to_string());
                }
            }
        }
        ("40.7128".into(), "-74.0060".into())
    }

    fn evaluate_hazard(
        temp_f: i32,
        code: u16,
        precip_mm: f32,
        precip_prob: u8,
        wind_kmh: f32,
    ) -> (&'static str, String) {
        if temp_f >= 90 {
            return ("danger", "Dangerous Heat".into());
        }
        if temp_f <= 32 {
            return ("danger", "Freezing Temp".into());
        }
        if code == 95 || code == 96 || code == 99 {
            return ("danger", "Severe Storm".into());
        }
        if code == 82 {
            return ("danger", "Violent Downpour".into());
        }
        if code == 66 || code == 67 {
            return ("danger", "Freezing Rain".into());
        }
        if code == 75 || code == 86 {
            return ("danger", "Blizzard".into());
        }
        if wind_kmh >= 60.0 {
            return ("danger", "Gale Winds".into());
        }
        if precip_mm >= 8.0 {
            return ("danger", "Flooding Risk".into());
        }

        if temp_f >= 85 {
            return ("warning", "Heat Alert".into());
        }
        if temp_f <= 40 {
            return ("warning", "Cold Alert".into());
        }
        if code == 65 {
            return ("warning", "Heavy Rain".into());
        }
        if code == 61 || code == 63 || code == 80 || code == 81 || code == 55 {
            return ("warning", "Rain Alert".into());
        }
        if (code == 51 || code == 53) && precip_prob >= 40 {
            return ("warning", "Drizzle".into());
        }
        if precip_prob >= 50 || precip_mm >= 1.0 {
            return ("warning", "Rain Likely".into());
        }
        if wind_kmh >= 45.0 {
            return ("warning", "High Winds".into());
        }

        ("none", String::new())
    }

    fn wmo_info(code: u16) -> (&'static str, &'static str) {
        match code {
            0 => ("Clear", "☀"),
            1 => ("Mainly Clear", "☀"),
            2 => ("Partly Cloudy", "☁"),
            3 => ("Overcast", "☁"),
            45 | 48 => ("Fog", "🌫"),
            51..=55 => ("Drizzle", "☂"),
            61..=65 => ("Rain", "☂"),
            71..=77 => ("Snow", "❄"),
            80..=82 => ("Showers", "☂"),
            95..=99 => ("Thunderstorm", "⚡"),
            _ => ("Fair", "☀"),
        }
    }
}
