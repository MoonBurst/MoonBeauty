//! CLI flag handlers.
//!
//! Each module here is a one-shot dispatcher for a family of command-line
//! flags. Called from `main()` before the bar boots, does its work over
//! IPC or subprocesses, then exits. This is the *flags* layer.
//!
//! Names like `music_flags` and `notify_flags` here refer to `--music-*`
//! and `--notify` flags, NOT the bar widgets of the same name
//! (`bar/capsules/music.rs`, `bar/capsules/notify.rs`). They bookend the
//! same user feature at different layers:
//!
//!   main() -> cli::music_flags::run(&args) -> IPC -> bar receives -> bar/capsules::music
//!          (send command)                          (execute)        (render new state)
//!
//! Same pattern for `notify`, `tray`, `tooltips`. The `bar/capsules/*`
//! files are pure render functions; the `cli/*` files are argv parsers
//! that exit once their work is dispatched.

pub mod clipboard_ingest;
pub mod test_lock_auth;
pub mod test_email;
pub mod lockscreen_coordinator;
pub mod screenshot;
pub mod keys;
pub mod notify_flags;
pub mod music_flags;
pub mod toggle;
pub mod tray;
pub mod tooltips;
pub mod restart;
