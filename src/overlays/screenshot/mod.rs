//! Screenshot capture + annotator.
//!
//! Contents:
//! - `capture`   : grim wrappers (fullscreen / per-output PNG capture)
//! - `output`    : swaymsg output listing
//! - `fragment`  : per-monitor freeze-frame selector (cross-monitor drag state)
//! - `annotator` : annotation UI (rect/ellipse/arrow/pen/text/counter/redact)
//! - `ipc`       : Unix socket listener for --annotator-surface commands


pub(crate) fn magick_or_convert() -> Option<&'static str> {
    if std::process::Command::new("magick")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("magick")
    } else if std::process::Command::new("convert")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        Some("convert")
    } else {
        None
    }
}

pub mod annotator;
pub mod capture;
pub mod fragment;
pub mod ipc;
pub mod output;

pub use annotator::{
    grim_capture, save_annotated_image, slurp_region, AnnotatorMessage, AnnotatorOverlay,
};
pub use ipc::spawn as spawn_annotator_ipc;

pub mod watermark;

pub use watermark::{add_plate_watermark, reveal};
