//! Crate-based "unified" system checks — no shell subprocess for the common
//! cases. Each item check uses an async function that talks to /proc, sockets,
//! D-Bus, or std::fs directly. Only the escape-hatch `Command` variants shell
//! out, and those are opt-in.

use crate::libraries::config::settings::{UnifiedCheck, UnifiedItem, UnifiedToggle};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Debug, Clone)]
pub struct UnifiedItemStatus {
    pub name: String,
    pub is_ok: bool,
    pub message: String,
    pub has_toggle: bool,
}

pub async fn check_unified_items(items: &[UnifiedItem]) -> Vec<UnifiedItemStatus> {
    let mut out = Vec::new();
    for item in items {
        let (is_ok, message) = check_item(&item.check).await;
        out.push(UnifiedItemStatus {
            name: item.name.clone(),
            is_ok,
            message,
            has_toggle: item.toggle.is_some(),
        });
    }
    out
}

async fn check_item(check: &UnifiedCheck) -> (bool, String) {
    match check {
        UnifiedCheck::Process { name } => {
            let ok = process_running(name);
            (ok, if ok { String::new() } else { format!("{} not running", name) })
        }
        UnifiedCheck::PortOpen { port } => {
            let ok = port_open(*port).await;
            (ok, if ok { String::new() } else { format!("port {} closed", port) })
        }
        UnifiedCheck::SystemdActive { unit } => match systemd_state(unit).await {
            Ok(state) => {
                let ok = state == "active";
                (ok, if ok { String::new() } else { format!("{}", state) })
            }
            Err(e) => (false, truncate(&e, 40)),
        },
        UnifiedCheck::SystemdInactive { unit } => match systemd_state(unit).await {
            Ok(state) => {
                let ok = state == "inactive" || state == "failed";
                (ok, if ok { String::new() } else { format!("{}", state) })
            }
            Err(e) => (false, truncate(&e, 40)),
        },
        UnifiedCheck::File { path } => {
            let ok = std::path::Path::new(path).is_file();
            (ok, if ok { String::new() } else { format!("missing: {}", path) })
        }
        UnifiedCheck::Directory { path } => {
            let ok = std::path::Path::new(path).is_dir();
            (ok, if ok { String::new() } else { format!("no dir: {}", path) })
        }
        UnifiedCheck::Mounted { path } => {
            let ok = is_mounted(path);
            (ok, if ok { String::new() } else { format!("unmounted: {}", path) })
        }
        UnifiedCheck::Command { command } => {
            match tokio::process::Command::new("sh").args(["-c", command]).output().await {
                Ok(o) => {
                    let ok = o.status.success();
                    let msg = if ok {
                        String::new()
                    } else {
                        truncate(String::from_utf8_lossy(&o.stderr).trim(), 40)
                    };
                    (ok, msg)
                }
                Err(e) => (false, truncate(&e.to_string(), 40)),
            }
        }
    }
}

pub async fn toggle_item(item: &UnifiedItem) -> Result<(), String> {
    let Some(toggle) = &item.toggle else { return Err("no toggle".into()); };
    match toggle {
        UnifiedToggle::Systemctl { action, unit } => {
            let method = match action.as_str() {
                "start" => "StartUnit",
                "stop" => "StopUnit",
                "restart" => "RestartUnit",
                "reload" => "ReloadUnit",
                _ => return Err(format!("unknown action: {}", action)),
            };
            let conn = zbus::Connection::system().await.map_err(|e| e.to_string())?;
            let mgr = zbus::Proxy::new(
                &conn,
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
            ).await.map_err(|e| e.to_string())?;
            mgr.call_method(method, &(unit.as_str(), "replace")).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        UnifiedToggle::Podman { action, container } => {
            let status = tokio::process::Command::new("podman")
                .args([action.as_str(), container.as_str()])
                .status().await.map_err(|e| e.to_string())?;
            if status.success() { Ok(()) } else { Err(format!("podman {} failed", action)) }
        }
        UnifiedToggle::Docker { action, container } => {
            let status = tokio::process::Command::new("docker")
                .args([action.as_str(), container.as_str()])
                .status().await.map_err(|e| e.to_string())?;
            if status.success() { Ok(()) } else { Err(format!("docker {} failed", action)) }
        }
        UnifiedToggle::Command { command } => {
            tokio::process::Command::new("sh").args(["-c", command]).output().await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

fn process_running(name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else { return false; };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(pid_str) = fname.to_str() else { continue; };
        if !pid_str.bytes().all(|b| b.is_ascii_digit()) { continue; }
        if let Ok(comm) = std::fs::read_to_string(format!("/proc/{}/comm", pid_str)) {
            if comm.trim() == name {
                return true;
            }
        }
    }
    false
}

async fn port_open(port: u16) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    timeout(Duration::from_millis(500), TcpStream::connect(&addr))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
}

async fn systemd_state(unit: &str) -> Result<String, String> {
    let conn = zbus::Connection::system().await.map_err(|e| e.to_string())?;
    let mgr = zbus::Proxy::new(
        &conn,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    ).await.map_err(|e| e.to_string())?;

    let reply = mgr.call_method("GetUnit", &(unit,)).await.map_err(|e| e.to_string())?;
    let unit_path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize()
        .map_err(|e| format!("{:?}", e))?;

    let unit_proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.systemd1",
        unit_path.as_str(),
        "org.freedesktop.systemd1.Unit",
    ).await.map_err(|e| e.to_string())?;

    let state: String = unit_proxy.get_property("ActiveState").await.map_err(|e| e.to_string())?;
    Ok(state)
}

fn is_mounted(path: &str) -> bool {
    let Ok(content) = std::fs::read_to_string("/proc/mounts") else { return false; };
    content.lines().any(|line| line.split_whitespace().nth(1) == Some(path))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max).collect() }
}
