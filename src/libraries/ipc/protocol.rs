use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OverlayTarget {
    Launcher,
    Calc,
    Clipboard,
    Dictionary,
    Unicode,
    Notes,
    Pass,
    Power,
    Todo,
    Settings,
    Web,
    Email,
    Reminders,
    Rng,
    Amogus,
    NotificationsHistory,
    Lockscreen,
    Annotator,
}

impl OverlayTarget {
    pub fn as_arg(&self) -> &'static str {
        match self {
            Self::Launcher => "launcher",
            Self::Calc => "calc",
            Self::Clipboard => "clipboard",
            Self::Dictionary => "dict",
            Self::Unicode => "unicode",
            Self::Notes => "notes",
            Self::Pass => "pass",
            Self::Power => "power",
            Self::Todo => "todo",
            Self::Settings => "settings",
            Self::Web => "web",
            Self::Email => "email",
            Self::Reminders => "reminders",
            Self::Rng => "rng",
            Self::Amogus => "amogus",
            Self::NotificationsHistory => "history",
            Self::Lockscreen => "lockscreen",
            Self::Annotator => "annotator",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "launcher" | "app" | "run" => Some(Self::Launcher),
            "calc" | "calculator" => Some(Self::Calc),
            "clipboard" | "clip" => Some(Self::Clipboard),
            "dictionary" | "dict" | "def" => Some(Self::Dictionary),
            "unicode" => Some(Self::Unicode),
            "notes" | "note" => Some(Self::Notes),
            "pass" | "password" => Some(Self::Pass),
            "power" | "pwr" => Some(Self::Power),
            "todo" | "td" => Some(Self::Todo),
            "settings" | "cfg" => Some(Self::Settings),
            "web" | "search" => Some(Self::Web),
            "email" | "mail" => Some(Self::Email),
            "reminders" | "remind" => Some(Self::Reminders),
            "rng" | "dice" | "roll" => Some(Self::Rng),
            "amogus" | "amongus" | "sus" => Some(Self::Amogus),
            "history" | "notif_history" => Some(Self::NotificationsHistory),
            "lockscreen" | "lock" => Some(Self::Lockscreen),
            "annotator" | "annotate" => Some(Self::Annotator),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcCommand {
    Toggle(OverlayTarget),
    Open(OverlayTarget, Option<String>),
    Close(OverlayTarget),
    CloseAll,
    DismissLatestNotif,
    JumpToLatestNotif,
    ToggleHistory,
    ReloadSettings,
    TestNotification,
    IncomingNotif {
        app_name: String,
        summary: String,
        body: String,
        is_urgent: bool,
    },
    Ping,
}

pub fn get_socket_path() -> PathBuf {
    // XDG_RUNTIME_DIR is user-owned 0700. Falling back to /tmp is unsafe:
    // any local user can race for the socket or read/write it.
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        panic!("XDG_RUNTIME_DIR is not set; refusing to place IPC socket in shared /tmp");
    });
    PathBuf::from(runtime_dir).join("moon-beauty-shell.sock")
}

pub async fn send_client_command(
    cmd: IpcCommand,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let path = get_socket_path();
    let mut stream = UnixStream::connect(path).await?;
    let encoded = serde_json::to_vec(&cmd)?;
    stream.write_all(&encoded).await?;
    stream.shutdown().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    pub fn bind() -> Result<Self, std::io::Error> {
        let path = get_socket_path();
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path)?;
        // Defense in depth: XDG_RUNTIME_DIR is 0700, but explicitly chmod
        // the socket to 0600 in case the dir mode ever regresses.
        let _ = std::fs::set_permissions(&path, {
            use std::os::unix::fs::PermissionsExt;
            std::fs::Permissions::from_mode(0o600)
        });
        Ok(Self { listener })
    }

    pub async fn accept(
        &self,
    ) -> Result<(UnixStream, IpcCommand), Box<dyn std::error::Error + Send + Sync>> {
        // Finding 37: cap read at 4 MB.
        const MAX_MSG: u64 = 4 * 1024 * 1024;
        let (mut stream, _) = self.listener.accept().await?;
        let mut buffer = Vec::new();
        (&mut stream)
            .take(MAX_MSG)
            .read_to_end(&mut buffer)
            .await?;
        let command: IpcCommand = serde_json::from_slice(&buffer)?;
        Ok((stream, command))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AnnotatorCommand {
    Show {
        path: String,
        watermark_cli: Option<String>,
    },
    Close,
}

pub fn get_annotator_socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        panic!("XDG_RUNTIME_DIR is not set; refusing to place annotator socket in shared /tmp");
    });
    std::path::PathBuf::from(runtime_dir).join("mbs-annotator-ctl.sock")
}

pub fn send_annotator_show(
    path: String,
    watermark_cli: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    let sock_path = get_annotator_socket_path();
    let mut stream = UnixStream::connect(sock_path)?;
    let cmd = AnnotatorCommand::Show {
        path,
        watermark_cli,
    };
    let json = serde_json::to_vec(&cmd)?;
    stream.write_all(&json)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(())
}
