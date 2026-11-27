//! Unified cross-process notification runtime state and command IPC.
//!
//! Consolidates the per-session runtime state shared between the bar,
//! `--notif-surface`, and CLI tools (`moonbeauty-notif-state.json` and
//! `moonbeauty-notif-cmd.json`) under `$XDG_RUNTIME_DIR` with 0600 permissions
//! and `flock` synchronization.

use super::ui::NotificationToast;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CMD_BYTES: u64 = 64 * 1024;
const ACTIVE_COUNT_STALE_SECS: u64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotifRuntimeCmd {
    DismissOldest,
    ActivateLatest,
    Activate {
        id: u32,
        token: Option<String>,
        action_key: Option<String>,
        is_resident: bool,
        #[serde(default)]
        close_notification: bool,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotifRuntimeState {
    #[serde(default)]
    pub dnd_active: bool,
    #[serde(default)]
    pub active_count: usize,
    #[serde(default)]
    pub active_count_epoch: u64,
    #[serde(default)]
    pub held_notifs: Vec<NotificationToast>,
}

/// Per-instance mtime-cached reader owned by `NotificationOverlay` (no global static).
#[derive(Debug, Default)]
pub struct NotifStateReader {
    last_mtime: Option<SystemTime>,
    cached: NotifRuntimeState,
}

impl NotifStateReader {
    pub fn new() -> Self {
        Self {
            last_mtime: None,
            cached: NotifRuntimeState::default(),
        }
    }

    pub fn read(&mut self) -> &NotifRuntimeState {
        let path = state_path();
        let mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        if mtime != self.last_mtime {
            self.cached = load_state_at(&path);
            self.last_mtime = mtime;
        }
        &self.cached
    }

    pub fn invalidate(&mut self) {
        self.last_mtime = None;
    }
}

pub fn notif_cmd_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("moonbeauty-notif-cmd.json")
}

pub fn state_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("moonbeauty-notif-state.json")
}

fn state_lock_path_for(path: &Path) -> PathBuf {
    path.with_extension("lock")
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[allow(unsafe_code)]
fn with_state_lock<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::os::unix::io::AsRawFd as _;
    let lock_path = state_lock_path_for(path);
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)
        .ok();
    if let Some(ref lf) = file {
        unsafe {
            libc::flock(lf.as_raw_fd(), libc::LOCK_EX);
        }
    }
    let res = f();
    if let Some(ref lf) = file {
        unsafe {
            libc::flock(lf.as_raw_fd(), libc::LOCK_UN);
        }
    }
    res
}

pub fn send_notif_cmd_to(path: &Path, cmd: &NotifRuntimeCmd) -> std::io::Result<()> {
    let json = serde_json::to_vec(cmd).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("tmp");
    crate::libraries::process::private_write_file(&tmp, &json)?;
    fs::rename(&tmp, path)
}

pub fn send_notif_cmd(cmd: &NotifRuntimeCmd) -> std::io::Result<()> {
    send_notif_cmd_to(&notif_cmd_path(), cmd)
}

pub fn take_notif_cmd_from(path: &Path) -> Option<NotifRuntimeCmd> {
    let cmd = crate::libraries::process::read_json_capped::<NotifRuntimeCmd>(path, MAX_CMD_BYTES);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    cmd
}

pub fn take_notif_cmd() -> Option<NotifRuntimeCmd> {
    take_notif_cmd_from(&notif_cmd_path())
}

fn load_state_at(path: &Path) -> NotifRuntimeState {
    crate::libraries::process::read_json_capped::<NotifRuntimeState>(path, MAX_STATE_BYTES)
        .unwrap_or_default()
}

fn save_state_at(path: &Path, state: &NotifRuntimeState) {
    if let Ok(json) = serde_json::to_vec(state) {
        let tmp = path.with_extension("tmp");
        if crate::libraries::process::private_write_file(&tmp, &json).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

fn mutate_state_at<R>(path: &Path, f: impl FnOnce(&mut NotifRuntimeState) -> R) -> R {
    with_state_lock(path, || {
        let mut state = load_state_at(path);
        let out = f(&mut state);
        save_state_at(path, &state);
        out
    })
}

fn valid_active_count(state: &NotifRuntimeState) -> usize {
    if state.active_count_epoch == 0 {
        return 0;
    }
    if now_epoch_secs().saturating_sub(state.active_count_epoch) > ACTIVE_COUNT_STALE_SECS {
        return 0;
    }
    state.active_count
}

/// Read `(dnd_active, held_count + active_count)` in a single pass.
pub fn read_bar_status() -> (bool, usize) {
    let state = load_state_at(&state_path());
    let total = state.held_notifs.len() + valid_active_count(&state);
    (state.dnd_active, total)
}

pub fn dnd_active() -> bool {
    load_state_at(&state_path()).dnd_active
}

pub fn toggle_dnd() -> bool {
    let next = mutate_state_at(&state_path(), |state| {
        state.dnd_active = !state.dnd_active;
        state.dnd_active
    });
    eprintln!("[dnd] paused={}", next);
    next
}

pub fn held_notifs_count() -> usize {
    load_state_at(&state_path()).held_notifs.len()
}

pub fn load_held_notifs() -> Vec<NotificationToast> {
    load_state_at(&state_path()).held_notifs
}

pub fn save_held_notifs(toasts: &[NotificationToast]) {
    mutate_state_at(&state_path(), |state| {
        state.held_notifs = toasts.to_vec();
    });
}

pub fn take_held_notifs() -> Vec<NotificationToast> {
    mutate_state_at(&state_path(), |state| std::mem::take(&mut state.held_notifs))
}

pub fn write_active_count(count: usize) {
    let now = now_epoch_secs();
    mutate_state_at(&state_path(), |state| {
        state.active_count = count;
        state.active_count_epoch = now;
    });
}

pub fn read_active_count() -> usize {
    let state = load_state_at(&state_path());
    valid_active_count(&state)
}

#[cfg(test)]
mod tests {
    use super::{
        load_state_at, mutate_state_at, send_notif_cmd_to, take_notif_cmd_from, valid_active_count,
        NotifRuntimeCmd, NotifRuntimeState,
    };

    #[test]
    fn notif_cmd_roundtrip_and_consume() {
        let dir = std::env::temp_dir().join(format!(
            "mbs-notif-cmd-test-{}",
            crate::libraries::process::random_token()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("moonbeauty-notif-cmd.json");

        let cmd = NotifRuntimeCmd::Activate {
            id: 42,
            token: Some("tok".into()),
            action_key: Some("default".into()),
            is_resident: true,
            close_notification: false,
        };
        send_notif_cmd_to(&path, &cmd).unwrap();
        assert_eq!(take_notif_cmd_from(&path), Some(cmd));
        assert_eq!(take_notif_cmd_from(&path), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn notif_state_mutation_and_stale_active_count() {
        let dir = std::env::temp_dir().join(format!(
            "mbs-notif-state-test-{}",
            crate::libraries::process::random_token()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("moonbeauty-notif-state.json");

        let paused = mutate_state_at(&path, |s| {
            s.dnd_active = true;
            s.active_count = 3;
            s.active_count_epoch = super::now_epoch_secs();
            s.dnd_active
        });
        assert!(paused);
        let loaded = load_state_at(&path);
        assert!(loaded.dnd_active);
        assert_eq!(valid_active_count(&loaded), 3);

        let stale = NotifRuntimeState {
            active_count: 5,
            active_count_epoch: 1,
            ..Default::default()
        };
        assert_eq!(valid_active_count(&stale), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
