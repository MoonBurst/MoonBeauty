//! Notification CLI handlers: --notify, --activate-latest-notif,
//! --dismiss-notif, --notif-history.
//!
//! Extracted from `main.rs`. All branches are terminal (exit on match,
//! no-op otherwise), so combined execution is safe regardless of which
//! other CLI block would have run first in main().

use moonbeauty::libraries::ipc::IpcCommand;

use crate::send_ipc_sync;

pub fn run(args: &[String]) {
    // Notification CLI actions
    if args.iter().any(|a| a == "--activate-latest-notif") {
        let cmd_path =
            moonbeauty::libraries::process::mbs_runtime_dir().join("moonbeauty-notif-cmd");
        match moonbeauty::libraries::process::private_write_file(&cmd_path, b"activate-latest") {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[activate-latest] write {:?}: {e}", cmd_path);
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|a| a == "--dismiss-notif") {
        let cmd_path =
            moonbeauty::libraries::process::mbs_runtime_dir().join("moonbeauty-notif-cmd");
        match moonbeauty::libraries::process::private_write_file(&cmd_path, b"dismiss") {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[dismiss-notif] write {:?}: {e}", cmd_path);
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|a| a == "--notif-history") {
        match send_ipc_sync(&IpcCommand::ToggleHistory) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("[notif-history] IPC failed: {e}");
                std::process::exit(1);
            }
        }
    }

    // CLI Notification Push: moonbeauty --notify "LusterDawn" "Message"
    if args.len() >= 3
        && (args[1] == "--notify" || (args.len() >= 4 && args[1] == "--ipc" && args[2] == "notify"))
    {
        let summary = if args[1] == "--notify" {
            &args[2]
        } else {
            &args[3]
        };
        let body = if args[1] == "--notify" {
            args.get(3).map(|s| s.as_str()).unwrap_or("")
        } else {
            args.get(4).map(|s| s.as_str()).unwrap_or("")
        };
        let _ = std::process::Command::new("notify-send")
            .args(["-a", "Desktop", "--", summary, body])
            .spawn();
        std::process::exit(0);
    }
}
