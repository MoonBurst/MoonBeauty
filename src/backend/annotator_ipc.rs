//! Persistent Unix-socket listener for `--annotator-surface`.
//! Pushes incoming AnnotatorCommand messages to an mpsc channel
//! consumed by the iced runtime via PollAnnotator subscription.

use std::sync::mpsc::Sender;

use crate::ipc::protocol::{get_annotator_socket_path, AnnotatorCommand};

pub fn spawn(tx: Sender<AnnotatorCommand>) {
    std::thread::spawn(move || {
        use std::io::Read;
        use std::os::unix::net::UnixListener;

        let path = get_annotator_socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => {
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
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                    let mut buf = Vec::new();
                    if stream.read_to_end(&mut buf).is_err() {
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
