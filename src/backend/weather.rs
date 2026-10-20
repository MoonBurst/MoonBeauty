use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeatherAlert {
    None,
    Upcoming(String),
    Danger(String),
}

#[derive(Debug, Clone)]
pub struct HourlyForecast {
    pub time_label: String,
    pub icon: &'static str,
    pub temp_f: i32,
    pub temp_c: i32,
    pub desc: &'static str,
    pub precip_prob: u8,
    pub is_danger: bool,
}

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temp_f: i32,
    pub temp_c: i32,
    pub icon: &'static str,
    pub desc: &'static str,
    pub alert: WeatherAlert,
    pub hourly: Vec<HourlyForecast>,
}

#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: Option<CurrentMeteo>,
    hourly: Option<HourlyMeteo>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct CurrentMeteo {
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
    wind_speed_10m: Option<Vec<f32>>,
}

pub struct WeatherBackend;

impl WeatherBackend {
    pub async fn fetch() -> Result<WeatherData, Box<dyn std::error::Error>> {
        let (lat, lon) = Self::detect_coords().await;
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,wind_speed_10m&hourly=temperature_2m,precipitation_probability,weather_code,wind_speed_10m&forecast_days=2&timezone=auto",
            lat, lon
        );

        let output = Command::new("curl")
            .args(["-s", "--max-time", "6", &url])
            .output()
            .await?;

        let res: OpenMeteoResponse = serde_json::from_slice(&output.stdout)?;
        let cur = res.current.ok_or("No current weather data")?;
        let hourly = res.hourly.ok_or("No hourly data")?;

        let cur_c = cur.temperature_2m.unwrap_or(20.0).round() as i32;
        let cur_f = (cur_c as f32 * 9.0 / 5.0 + 32.0).round() as i32;
        let (cur_desc, cur_icon) = Self::wmo_info(cur.weather_code.unwrap_or(0));

        let mut alert = WeatherAlert::None;
        let mut timeline = Vec::new();
        let total = hourly.time.len().min(24);

        for i in 0..total {
            let tc = hourly.temperature_2m.get(i).copied().unwrap_or(20.0).round() as i32;
            let tf = (tc as f32 * 9.0 / 5.0 + 32.0).round() as i32;
            let code = hourly.weather_code.get(i).copied().unwrap_or(0);
            let (desc, icon) = Self::wmo_info(code);
            let prob = hourly.precipitation_probability.as_ref().and_then(|p| p.get(i).copied()).unwrap_or(0);
            let wind = hourly.wind_speed_10m.as_ref().and_then(|w| w.get(i).copied()).unwrap_or(0.0);

            let is_danger = tf >= 95 || tf <= 30 || code >= 95 || wind >= 60.0;
            if is_danger && i <= 3 && alert == WeatherAlert::None {
                alert = WeatherAlert::Danger(format!("Severe Weather ({}h)", i));
            } else if prob >= 60 && i <= 3 && alert == WeatherAlert::None {
                alert = WeatherAlert::Upcoming(format!("Rain in {}h", i));
            }

            let hour_raw = hourly.time[i].split('T').nth(1).unwrap_or("12:00");
            timeline.push(HourlyForecast {
                time_label: hour_raw.to_string(),
                icon,
                temp_f: tf,
                temp_c: tc,
                desc,
                precip_prob: prob,
                is_danger,
            });
        }

        Ok(WeatherData {
            temp_f: cur_f,
            temp_c: cur_c,
            icon: cur_icon,
            desc: cur_desc,
            alert,
            hourly: timeline,
        })
    }

    async fn detect_coords() -> (String, String) {
        if let Ok(out) = Command::new("curl")
            .args(["-s", "--max-time", "3", "http://ip-api.com/json"])
            .output()
            .await
        {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let (Some(lat), Some(lon)) = (val["lat"].as_f64(), val["lon"].as_f64()) {
                    return (lat.to_string(), lon.to_string());
                }
            }
        }
        ("40.7128".into(), "-74.0060".into())
    }

    fn wmo_info(code: u16) -> (&'static str, &'static str) {
        match code {
            0 => ("Clear", "☀️"),
            1 => ("Mainly Clear", "🌤️"),
            2 => ("Partly Cloudy", "⛅"),
            3 => ("Overcast", "☁️"),
            45 | 48 => ("Fog", "🌫️"),
            51..=55 => ("Drizzle", "🌦️"),
            61..=65 => ("Rain", "🌧️"),
            71..=77 => ("Snow", "❄️"),
            80..=82 => ("Showers", "⛈️"),
            95..=99 => ("Thunderstorm", "⛈️"),
            _ => ("Fair", "🌤️"),
        }
    }
}
