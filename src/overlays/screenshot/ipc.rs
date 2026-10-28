//! Persistent Unix-socket listener for `--annotator-surface`.
//! Pushes incoming AnnotatorCommand messages to an mpsc channel
//! consumed by the iced runtime via PollAnnotator subscription.

use std::sync::mpsc::Sender;

use crate::libraries::ipc::protocol::{get_annotator_socket_path, AnnotatorCommand};

pub fn spawn(tx: Sender<AnnotatorCommand>) {
    std::thread::spawn(move || {
        use std::io::Read;
        use std::os::unix::net::UnixListener;

        let path = get_annotator_socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => {
                let _ = std::fs::set_permissions(&path, {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::Permissions::from_mode(0o600)
                });
                eprintln!("[annotator-ipc] listening on {:?}", path);
                l
            }
            Err(e) => {
                eprintln!("[annotator-ipc] bind failed: {e}");
                return;
            }
        };

        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Finding 49: 1s timeout (was 3s).
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(1)));
                    // Finding 51: 4MB Take cap (same as finding 37 for
                    // the two main IPC accept paths). Without this, a
                    // same-user client sends 1GB JSON-shaped bytes →
                    // OOM. Currently this code path has no caller, but
                    // it compiles and would ship unhardened if wired up.
                    const MAX_MSG: u64 = 4 * 1024 * 1024;
                    let mut buf = Vec::new();
                    let mut limited = std::io::Read::take(&mut stream, MAX_MSG);
                    if limited.read_to_end(&mut buf).is_err() {
                        continue;
                    }
                    match serde_json::from_slice::<AnnotatorCommand>(&buf) {
                        Ok(cmd) => {
                            eprintln!("[annotator-ipc] received {:?}", cmd);
                            if tx.send(cmd).is_err() {
                                break;
                            }
                        }
                        Err(e) => eprintln!("[annotator-ipc] parse: {e}"),
                    }
                }
                Err(e) => eprintln!("[annotator-ipc] accept: {e}"),
            }
        }
    });
}
