pub mod protocol;

pub use protocol::{
    get_annotator_socket_path, get_socket_path, send_client_command, AnnotatorCommand, IpcCommand,
    OverlayTarget,
};

pub mod cli;

pub fn send_ipc_sync(cmd: &IpcCommand) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let path = std::env::var("XDG_RUNTIME_DIR")
        .map(|d| std::path::PathBuf::from(d).join("moonbeauty.sock"))
        .map_err(|e| format!("XDG_RUNTIME_DIR: {e}"))?;
    let mut stream = UnixStream::connect(&path)?;
    // Read timeout so a hung bar cannot block the CLI indefinitely.
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let encoded = serde_json::to_vec(cmd)?;
    stream.write_all(&encoded)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp)?;
    Ok(resp)
}
