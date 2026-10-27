//! Screenshot capture + annotator.
//!
//! Contents:
//! - `capture`   : grim wrappers (fullscreen / per-output PNG capture)
//! - `output`    : swaymsg output listing
//! - `fragment`  : per-monitor freeze-frame selector (cross-monitor drag state)
//! - `annotator` : annotation UI (rect/ellipse/arrow/pen/text/counter/redact)
//! - `ipc`       : Unix socket listener for --annotator-surface commands

pub mod annotator;
pub mod capture;
pub mod output;
pub mod fragment;
pub mod ipc;

pub use annotator::{
    AnnotatorMessage, AnnotatorOverlay,
    save_annotated_image, slurp_region, grim_capture,
};
pub use ipc::spawn as spawn_annotator_ipc;
