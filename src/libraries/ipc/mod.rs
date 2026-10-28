pub mod protocol;

pub use protocol::{
    get_annotator_socket_path, get_socket_path, send_annotator_show, send_client_command,
    AnnotatorCommand, IpcCommand, IpcServer, OverlayTarget,
};
