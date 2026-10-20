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
    Quickshot,
    NotificationsHistory,
    Lockscreen,
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
            Self::Quickshot => "quickshot",
            Self::NotificationsHistory => "history",
            Self::Lockscreen => "lockscreen",
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
            "quickshot" | "screenshot" => Some(Self::Quickshot),
            "history" | "notif_history" => Some(Self::NotificationsHistory),
            "lockscreen" | "lock" => Some(Self::Lockscreen),
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
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/user-{}", std::process::id()));
    PathBuf::from(runtime_dir).join("moon-beauty-shell.sock")
}

pub async fn send_client_command(cmd: IpcCommand) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener })
    }

    pub async fn accept(&self) -> Result<(UnixStream, IpcCommand), Box<dyn std::error::Error + Send + Sync>> {
        let (mut stream, _) = self.listener.accept().await?;
        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).await?;
        let command: IpcCommand = serde_json::from_slice(&buffer)?;
        Ok((stream, command))
    }
}
