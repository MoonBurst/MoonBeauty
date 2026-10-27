use chrono::{Datelike, Local, NaiveDate, Utc};

#[derive(Debug, Clone)]
pub struct WorldZone {
    pub name: &'static str,
    pub code: &'static str,
    pub offset_hours: f32,
}

#[derive(Debug, Clone)]
pub struct WorldZoneColumn {
    pub title: &'static str,
    pub zones: Vec<(&'static str, &'static str, String, bool)>,
}

#[derive(Debug, Clone)]
pub struct CalendarViewData {
    pub date_str: String,
    pub month_label: String,
    pub year: i32,
    pub month: u32,
    pub first_day_offset: usize,
    pub days_in_month: u32,
    pub today_day: u32,
}

pub struct ClockBackend;

impl ClockBackend {
    pub fn get_bar_time() -> String {
        Local::now().format("%I:%M %p").to_string()
    }

    pub fn get_world_matrix() -> Vec<WorldZoneColumn> {
        let now_utc = Utc::now();
        let local_code = Local::now().format("%Z").to_string();

        let americas = vec![
            ("Pacific Time", "US/PT", -7.0),
            ("Mountain Time", "US/MT", -6.0),
            ("Central Time", "US/CT", -5.0),
            ("Eastern Time", "US/ET", -4.0),
            ("São Paulo", "BRT", -3.0),
        ];

        let europe = vec![
            ("UTC / GMT", "UTC", 0.0),
            ("London", "BST", 1.0),
            ("Berlin / Paris", "CEST", 2.0),
            ("Cairo", "EEST", 3.0),
            ("Moscow", "MSK", 3.0),
        ];

        let asia = vec![
            ("Dubai", "GST", 4.0),
            ("India", "IST", 5.5),
            ("Singapore", "SGT", 8.0),
            ("Tokyo", "JST", 9.0),
            ("Sydney", "AEST", 10.0),
        ];

        let render_column = |title: &'static str, zones: Vec<(&'static str, &'static str, f32)>| {
            let rendered = zones
                .into_iter()
                .map(|(name, code, offset)| {
                    let zone_time = now_utc + chrono::Duration::minutes((offset * 60.0) as i64);
                    let formatted = zone_time.format("%I:%M %p").to_string();
                    let is_me = code == local_code;
                    (name, code, formatted, is_me)
                })
                .collect();

            WorldZoneColumn {
                title,
                zones: rendered,
            }
        };

        vec![
            render_column("AMERICAS", americas),
            render_column("EUROPE & AFRICA", europe),
            render_column("ASIA & PACIFIC", asia),
        ]
    }

    pub fn compute_calendar(year: i32, month: u32) -> CalendarViewData {
        let now = Local::now();
        let y = if year > 0 { year } else { now.year() };
        let m = if (1..=12).contains(&month) {
            month
        } else {
            now.month()
        };

        let first_day = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_default();
        let first_day_offset = first_day.weekday().num_days_from_sunday() as usize;

        let days_in_month = if m == 12 {
            NaiveDate::from_ymd_opt(y + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(y, m + 1, 1)
        }
        .unwrap_or_default()
        .signed_duration_since(first_day)
        .num_days() as u32;

        let today_day = if y == now.year() && m == now.month() {
            now.day()
        } else {
            0
        };

        CalendarViewData {
            date_str: now.format("%a, %b %d").to_string(),
            month_label: first_day.format("%B %Y").to_string(),
            year: y,
            month: m,
            first_day_offset,
            days_in_month,
            today_day,
        }
    }
}
