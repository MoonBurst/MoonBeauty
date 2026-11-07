//! --toggle <overlay> and bare --<overlay> (implicit IPC toggle).

use moonbeauty::libraries::ipc::{IpcCommand, OverlayTarget};

use crate::send_ipc_sync;

pub fn run(args: &[String]) {
    // Generic IPC toggle: moonbeauty --toggle <overlay>
    // e.g. sway: bindsym $mod+Tab exec moonbeauty --toggle history
    if let Some(idx) = args.iter().position(|a| a == "--toggle") {
        let target_str = args.get(idx + 1).cloned().unwrap_or_default();
        match OverlayTarget::from_str(&target_str) {
            Some(target) => match send_ipc_sync(&IpcCommand::Toggle(target)) {
                Ok(_) => std::process::exit(0),
                Err(e) => {
                    eprintln!("[toggle] IPC failed: {e}");
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("[toggle] unknown target: {}", target_str);
                std::process::exit(2);
            }
        }
    }

    // Bare `--<overlay>` (e.g. `moonbeauty --history`) = implicit IPC
    // toggle, equivalent to `--toggle <overlay>`.  Explicit standalone
    // spawn is `--overlay <target>` — used internally by the bar to
    // spawn overlay children; not meant for user invocation.
    if let Some(arg) = args.get(1) {
        if let Some(name) = arg.strip_prefix("--") {
            if let Some(target) = OverlayTarget::from_str(name) {
                match send_ipc_sync(&IpcCommand::Toggle(target)) {
                    Ok(_) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("[toggle] IPC failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
