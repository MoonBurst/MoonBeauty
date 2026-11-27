//! Weather capsule + tooltip.
//!
//! - `client`  : WeatherBackend (Open-Meteo polling via curl), WeatherData,
//!               WeatherAlert (danger/warning severity classification)
//! - `tooltip` : the in-bar layer-shell tooltip rendered when the weather
//!               capsule is clicked
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

pub mod client;
pub mod tooltip;

pub use client::{WeatherAlert, WeatherBackend, WeatherData};
