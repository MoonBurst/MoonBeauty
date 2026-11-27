//! Notification CLI handlers: --notify, --activate-latest-notif,
//! --dismiss-notif, --notif-history.
//!
//! Extracted from `main.rs`. All branches are terminal (exit on match,
//! no-op otherwise), so combined execution is safe regardless of which
//! other CLI block would have run first in main().

use super::state::{send_notif_cmd, NotifRuntimeCmd};
use crate::libraries::ipc::{send_ipc_sync, IpcCommand};

pub fn run(args: &[String]) {
    fn dispatch_notif_cmd(cmd: NotifRuntimeCmd, tag: &str) -> ! {
        if let Err(e) = send_notif_cmd(&cmd) {
            eprintln!("[{tag}] failed to write command: {e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    // Notification CLI actions
    if args.iter().any(|a| a == "--activate-latest-notif") {
        dispatch_notif_cmd(NotifRuntimeCmd::ActivateLatest, "activate-latest");
    }
    if args.iter().any(|a| a == "--dismiss-notif") {
        dispatch_notif_cmd(NotifRuntimeCmd::DismissOldest, "dismiss-notif");
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
