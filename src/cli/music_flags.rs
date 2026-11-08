//! Music CLI flags: --music-cycle, --music-next, --music-previous,
//! --music-play-pause. All fire IpcCommand::Music* over the local socket.

use moonbeauty::libraries::ipc::IpcCommand;

use crate::send_ipc_sync;

pub fn run(args: &[String]) {
    // Music CLI flags — fired from the tooltip's Cycle/Prev/Play/Next
    // buttons and bindable to sway keybinds.
    for (flag, cmd) in [
        ("--music-cycle", IpcCommand::MusicCycle),
        ("--music-next", IpcCommand::MusicNext),
        ("--music-previous", IpcCommand::MusicPrevious),
        ("--music-play-pause", IpcCommand::MusicPlayPause),
    ] {
        if args.iter().any(|a| a == flag) {
            match send_ipc_sync(&cmd) {
                Ok(_) => std::process::exit(0),
                Err(e) => {
                    eprintln!("[{flag}] IPC failed: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
