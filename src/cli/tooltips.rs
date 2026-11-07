//! Tooltip dispatchers: --sys-tooltip, --weather-tooltip, --info-tooltip.
//! Each calls into overlays::*::run() and exits on its result.

pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--sys-tooltip") {
        match moonbeauty::overlays::system_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[sys-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--weather-tooltip") {
        match moonbeauty::bar::weather::tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[weather-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--info-tooltip") {
        match moonbeauty::overlays::info_tooltip::run() {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("[info-tooltip] {e}");
                std::process::exit(1);
            }
        }
    }
}
