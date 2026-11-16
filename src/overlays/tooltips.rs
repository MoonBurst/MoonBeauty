//! Tooltip dispatchers: --sys-tooltip, --weather-tooltip, --info-tooltip.
//! Each calls into overlays::*::run() and exits on its result.

pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--sys-tooltip") {
        match crate::overlays::system_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[sys-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--weather-tooltip") {
        match crate::bar::weather::tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[weather-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--info-tooltip") {
        match crate::overlays::info_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[info-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }
}
