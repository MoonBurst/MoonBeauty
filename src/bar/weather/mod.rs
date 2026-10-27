//! Weather capsule + tooltip.
//!
//! - `client`  : WeatherBackend (Open-Meteo polling via curl), WeatherData,
//!               WeatherAlert (danger/warning severity classification)
//! - `tooltip` : the standalone layer-shell popup spawned by `--weather-tooltip`
//!               from main.rs when the weather capsule is clicked
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

pub mod client;
pub mod tooltip;

pub use client::{WeatherAlert, WeatherBackend, WeatherData};
pub use tooltip::run as run_weather_tooltip;
