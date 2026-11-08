//! Multi-monitor lock coordinator.
//!
//! Coordinator owns the password buffer. Each child is a pure display —
//! it forwards every keystroke over IPC and renders whatever dot count
//! the coordinator broadcasts. Result: typing on DP-1 makes dots appear
//! on DP-1 AND DP-2 simultaneously. Enter from any monitor triggers auth.
//!
//! Child protocol:
//!   →  Key(String)      single char or IME cluster
//!   →  Backspace
//!   →  Submit
//!   ←  BufferUpdate(usize)  current char count in coordinator's buffer
//!   ←  Reset                auth failed, clear everything
//!   ←  Success              auth succeeded, prepare to be killed
//!
//! Hotplug: subscribe to `swaymsg -t subscribe -m '["output"]'`.
//! `added` → spawn child; `removed` → kill child.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LockIpc {
    // ── Client → Coordinator ──
    Key(String),
    Backspace,
    Submit,
    // ── Coordinator → Client ──
    BufferUpdate(usize),
    Reset,
    Success,
}

struct Peer {
    id: usize,
    writer: OwnedWriteHalf,
}

/// Resolve the effective user name. Uses `id -un` (the source of truth
/// for the calling process's euid) before falling back to env vars.
/// `$USER` is not always set — e.g. when launched via `nix develop -c`,
/// `systemd --user`, or certain sway exec contexts. If we fell back to
/// "root" here, pamtester would try to authenticate as root and always fail.
fn resolve_user() -> String {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let id_bin = find_bin("id").unwrap_or_else(|| std::path::PathBuf::from("id"));
            if let Ok(out) = std::process::Command::new(&id_bin).arg("-un").output() {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !s.is_empty() {
                        return s;
                    }
                }
            }
            std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .unwrap_or_else(|_| "root".into())
        })
        .clone()
}

/// First 8 hex chars of SHA-256(input). Used so the coordinator log shows
/// a stable fingerprint of the captured password — the user can compute the
/// same hash of their real password to definitively prove the buffer is
/// correct (or not) without ever logging the plaintext.
fn sha256_prefix(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let r = h.finalize();
    format!("{:02x}{:02x}{:02x}{:02x}", r[0], r[1], r[2], r[3])
}

/// Locate a binary reliably even when PATH is empty. NixOS puts most tools
/// in /run/current-system/sw/bin, wrappers in /run/wrappers/bin, and user
/// nix profile in ~/.nix-profile/bin. Fall back to PATH last.
fn find_bin(name: &str) -> Option<std::path::PathBuf> {
    let fallbacks: &[&str] = match name {
        "pamtester" => &[
            "/run/current-system/sw/bin/pamtester",
            "/usr/bin/pamtester",
            "/usr/local/bin/pamtester",
            "/nix/var/nix/profiles/default/bin/pamtester",
        ],
        "sudo" => &["/run/wrappers/bin/sudo", "/usr/bin/sudo", "/bin/sudo"],
        "su" => &["/run/wrappers/bin/su", "/usr/bin/su", "/bin/su"],
        "swaymsg" => &["/run/current-system/sw/bin/swaymsg", "/usr/bin/swaymsg"],
        "id" => &["/usr/bin/id", "/bin/id"],
        "script" => &["/usr/bin/script", "/bin/script"],
        "which" => &["/usr/bin/which", "/bin/which"],
        _ => &[],
    };
    for p in fallbacks {
        if std::path::Path::new(p).exists() {
            return Some(std::path::PathBuf::from(p));
        }
    }
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in path_env.split(':') {
            let c = std::path::Path::new(dir).join(name);
            if c.exists() {
                return Some(c);
            }
        }
    }
    None
}

/// Sweep stale lockscreen sockets whose embedded PID is not a live
/// coordinator. Called from spawn() before binding the new socket so
/// the sweep can't race with our own freshly-created file.
fn sweep_stale_lockscreen_sockets(runtime_dir: &str, current_pid: u32) {
    let Ok(entries) = std::fs::read_dir(runtime_dir) else {
        return;
    };
    let mut cleaned = 0u32;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(s) = name.to_str() else {
            continue;
        };
        let Some(rest) = s.strip_prefix("moon-beauty-lock-") else {
            continue;
        };
        let Some(pid_str) = rest.strip_suffix(".sock") else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        let cmdline = std::fs::read(format!("/proc/{}/cmdline", pid)).ok();
        let is_live = cmdline
            .as_ref()
            .map(|c| c.split(|b| *b == 0).any(|part| part == b"--lockscreen"))
            .unwrap_or(false);
        if is_live {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            cleaned += 1;
            eprintln!(
                "[lock-coord] cleaned stale socket {:?} (pid {} no longer a coordinator)",
                entry.path(),
                pid
            );
        }
    }
    if cleaned > 0 {
        eprintln!("[lock-coord] swept {} stale socket file(s)", cleaned);
    }
}

/// Kill all children, wait for them, remove the socket file. Called from
/// the SIGTERM/SIGINT handler before std::process::exit(0).
fn shutdown_lockscreen_children(
    children: &Arc<Mutex<HashMap<String, Child>>>,
    socket_path: &std::path::Path,
) {
    if let Ok(mut g) = children.lock() {
        for (_, mut child) in g.drain() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    let _ = std::fs::remove_file(socket_path);
    eprintln!("[lock-coord] shutdown complete");
}

fn sudo_is_nopasswd() -> bool {
    // Detect whether `sudo` requires a password on this host.
    //
    // Runs `sudo -k -n -v`:
    //   -k  invalidate any cached sudo timestamp (avoids false positives)
    //   -n  never prompt (fails immediately if a password would be needed)
    //   -v  validate (renew) the sudo timestamp
    //
    // Exit 0 = sudo did NOT need a password → NOPASSWD rule active.
    // Exit !=0 = sudo needs a password, or sudo is missing.
    //
    // On a NOPASSWD host, `sudo -v` in authenticate() would return 0
    // with ANY input, making sudo useless as an auth oracle. This check
    // short-circuits that strategy.
    let sudo_bin = find_bin("sudo").unwrap_or_else(|| std::path::PathBuf::from("sudo"));
    std::process::Command::new(&sudo_bin)
        .args(["-k", "-n", "-v"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub struct LockCoordinator;

impl LockCoordinator {
    /// Interactive auth test — run with `--test-lock-auth`. Reads password
    /// from stdin (masked), runs every strategy, reports results.
    pub fn run_auth_test() {
        use std::io::{stdin, BufRead};

        eprintln!("=== Lock auth test ===");
        eprintln!("This bypasses the lock screen and tests the auth logic directly.");
        eprintln!();

        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async {
            // Test sudo availability
            eprintln!("── Environment ──");
            eprintln!("USER = {:?}", std::env::var("USER"));
            match tokio::process::Command::new("which")
                .arg("sudo")
                .output()
                .await
            {
                Ok(out) => eprintln!("sudo = {:?}", String::from_utf8_lossy(&out.stdout).trim()),
                Err(_) => eprintln!("sudo = NOT FOUND"),
            }
            match tokio::process::Command::new("which")
                .arg("pamtester")
                .output()
                .await
            {
                Ok(out) => {
                    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if p.is_empty() {
                        eprintln!("pamtester = not installed");
                    } else {
                        eprintln!("pamtester = {:?}", p);
                    }
                }
                Err(_) => eprintln!("pamtester = not installed"),
            }
            eprintln!();

            // Prompt for password
            eprintln!("── Input ──");
            eprint!("Enter password (visible for debugging): ");
            use std::io::Write;
            std::io::stderr().flush().ok();
            let mut input = String::new();
            stdin().lock().read_line(&mut input).ok();
            let mut password =
                Zeroizing::new(input.trim_end_matches(['\r', '\n']).to_string());
            input.zeroize();
            eprintln!(
                "len={} first={:?} last={:?}",
                password.chars().count(),
                password.chars().next(),
                password.chars().last()
            );
            eprintln!();

            // Run authenticate with full logging
            eprintln!("── Running strategies ──");
            let ok = Self::authenticate(&password).await;
            eprintln!();
            if ok {
                eprintln!("RESULT: ✓ auth succeeded");
                password.zeroize();
                std::process::exit(0);
            } else {
                eprintln!("RESULT: ✗ all strategies failed");
                password.zeroize();
                std::process::exit(1);
            }
        });
    }

    pub fn spawn() -> ! {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
            eprintln!("[lock-coord] FATAL: XDG_RUNTIME_DIR is not set; refusing to place lockscreen socket in shared /tmp");
            std::process::exit(1);
        });
        eprintln!("[lock-coord-v225] START pid={}", std::process::id());
        let pid = std::process::id();
        let socket_path =
            PathBuf::from(&runtime_dir).join(format!("moon-beauty-lock-{}.sock", pid));
        sweep_stale_lockscreen_sockets(&runtime_dir, pid);
        let _ = std::fs::remove_file(&socket_path);

        #[cfg(not(feature = "lock-debug-bypass"))]
        if std::env::var_os("MBS_LOCK_DEBUG_PASS").is_some() {
            eprintln!("[lock-coord] NOTE: MBS_LOCK_DEBUG_PASS set but lock-debug-bypass feature is not compiled in; NO effect");
        }
        #[cfg(feature = "lock-debug-bypass")]
        if let Ok(v) = std::env::var("MBS_LOCK_DEBUG_PASS") {
            eprintln!(
                "[lock-coord] WARNING: MBS_LOCK_DEBUG_PASS is set (len={}); bypass armed (COMPILE-TIME FEATURE — DO NOT SHIP)",
                v.len()
            );
        }

        {
            let u = resolve_user();
            eprintln!("[lock-coord] user resolved via id -un → {}", u);
        }

        let children: Arc<Mutex<HashMap<String, Child>>> = Arc::new(Mutex::new(HashMap::new()));
        // Which output's child currently holds the Exclusive keyboard
        // grab. Only one child may be primary: Wayland allows one
        // Exclusive layer surface per seat, and spawning all three as
        // Exclusive lets sway rotate focus mid-typing, dropping chars
        // from the coordinator's buffer.
        let primary_output: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        // Startup diagnostic: report what auth strategies are usable.
        // Logs a clear WARNING when no strategy can succeed — the
        // lockscreen will fail closed (refuse all passwords) rather than
        // silently accept any input, and the warning tells the user
        // exactly which config line to fix.
        {
            let pam_bin = find_bin("pamtester");
            let sudo_bin = find_bin("sudo");
            let sudo_nopasswd = if sudo_bin.is_some() {
                sudo_is_nopasswd()
            } else {
                false
            };
            eprintln!(
                "[lock-coord] auth: pamtester={:?} sudo={:?} sudo_nopasswd={}",
                pam_bin, sudo_bin, sudo_nopasswd
            );
            let usable = pam_bin.is_some() as u32 + (sudo_bin.is_some() && !sudo_nopasswd) as u32;
            if usable == 0 {
                eprintln!(
                    "[lock-coord] WARNING: no working auth strategy. \
                     The lockscreen will FAIL CLOSED — even the correct \
                     password will not unlock. Fix one of: (a) install \
                     pamtester AND enable security.pam.services.login, \
                     (b) remove NOPASSWD from the invoking user's sudoers \
                     rule."
                );
            }
        }

        eprintln!("[lock-coord-v225] binding socket at {:?}", socket_path);
        let listener = match std::os::unix::net::UnixListener::bind(&socket_path) {
            Ok(l) => {
                let _ = std::fs::set_permissions(&socket_path, {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::Permissions::from_mode(0o600)
                });
                eprintln!("[lock-coord-v225] socket bound OK");
                l
            }
            Err(e) => {
                eprintln!("[lock-coord] bind failed: {}", e);
                Self::kill_all(&children);
                std::process::exit(1);
            }
        };
        listener.set_nonblocking(true).ok();

        // Initial spawn
        let outputs = Self::list_outputs();
        if outputs.is_empty() {
            eprintln!("[lock-coord] no outputs — spawning single instance");
            *primary_output.lock().unwrap() = Some(String::new());
            Self::spawn_child("", &socket_path, &children, true);
        } else {
            eprintln!(
                "[lock-coord] spawning {} children for {:?}",
                outputs.len(),
                outputs
            );
            for (idx, output) in outputs.iter().enumerate() {
                let is_primary = idx == 0;
                if is_primary {
                    *primary_output.lock().unwrap() = Some(output.clone());
                }
                eprintln!(
                    "[lock-coord-v225] spawning child for output={:?} primary={}",
                    output, is_primary
                );
                Self::spawn_child(output, &socket_path, &children, is_primary);
            }
        }

        let rt = tokio::runtime::Runtime::new().expect("rt");
        rt.block_on(async move {
            let listener = match tokio::net::UnixListener::from_std(listener) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[lock-coord] tokio listener: {}", e);
                    Self::kill_all(&children);
                    std::process::exit(1);
                }
            };

            let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<(usize, LockIpc)>(256);
            let mut peers: Vec<Peer> = Vec::new();
            let mut buffer = String::new();
            let mut next_id: usize = 1;
            // Finding 30: PAM/sudo account-lockout DoS mitigation.
            // A hostile same-user process can open the coordinator socket
            // and spam Submit to force N failed auths, tripping
            // pam_faillock/sudo lockout for the legitimate user.
            // Exponential backoff caps the attacker's rate to ~1/16s.
            let mut last_failed_submit: Option<std::time::Instant> = None;
            let mut fail_streak: u32 = 0;

            // Hotplug watcher
            let children_watch = children.clone();
            let socket_watch = socket_path.clone();
            let primary_output_watch = primary_output.clone();
            tokio::spawn(async move {
                let mut cmd = match tokio::process::Command::new("swaymsg")
                    .args(["-t", "subscribe", "-m", r#"["output"]"#])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[lock-coord] hotplug watcher: {}", e);
                        return;
                    }
                };
                let Some(out) = cmd.stdout.take() else { return; };
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    let change = v.get("change").and_then(|c| c.as_str()).unwrap_or("");
                    let output = v.get("output").and_then(|o| o.as_str()).unwrap_or("").to_string();
                    match change {
                        "added" => {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            // Kill any blocker/orphan before spawning fresh.
                            // Brief gap here is unavoidable; the important
                            // guarantee is that we get a working child.
                            let is_primary = {
                                let mut po = primary_output_watch.lock().unwrap();
                                if po.as_deref() == Some(output.as_str()) {
                                    // Replacing the primary; new one is
                                    // also primary.
                                    *po = None;
                                    true
                                } else if po.is_none() {
                                    *po = Some(output.clone());
                                    true
                                } else {
                                    false
                                }
                            };
                            if let Ok(mut map) = children_watch.lock() {
                                if let Some(mut old) = map.remove(&output) {
                                    eprintln!(
                                        "[lock-coord] hotplug: replacing child for {:?}",
                                        output
                                    );
                                    let _ = old.kill();
                                    let _ = old.wait();
                                }
                            }
                            Self::spawn_child(&output, &socket_watch, &children_watch, is_primary);
                        }
                        "removed" => {
                            // Do NOT kill the child. On TTY switch, sway
                            // may emit "removed" for outputs that are
                            // temporarily unavailable. Killing the child
                            // here creates an unlocked-screen window until
                            // "added" fires and a fresh child boots.
                            // Instead, leave the child alive as a blocker.
                            // The "added" branch below cleans it up before
                            // spawning a fresh one.
                            eprintln!(
                                "[lock-coord] hotplug: output removed: {:?} (child kept as blocker)",
                                output
                            );
                        }
                        _ => {}
                    }
                }
            });


            // ── TTY return watcher ──
            // When the user switches to another VT and back, sway drops
            // the layer surfaces held by our lockscreen children but does
            // not send Closed. The children stay alive with no visible
            // surface. We capture the desktop TTY at watcher-startup and
            // respawn on return-to-desktop. Switching away does nothing
            // (surfaces hidden anyway).
            //
            // Safe now that only the primary child is Exclusive: three
            // simultaneous Exclusive surfaces was what caused the earlier
            // focus-flap regression. With primary/secondary split, respawn
            // order is primary-first (300ms settle, then secondaries 200ms
            // apart) so sway has clean windows to composite.
            let children_tty = children.clone();
            let socket_tty = socket_path.clone();
            let primary_tty = primary_output.clone();
            tokio::spawn(async move {
                let read_tty = || {
                    std::fs::read_to_string("/sys/class/tty/tty0/active")
                        .ok()
                        .map(|s| s.trim().to_string())
                };
                let desktop_tty = match read_tty() {
                    Some(v) => v,
                    None => {
                        eprintln!(
                            "[lock-coord] TTY watcher disabled: \
                             /sys/class/tty/tty0/active unreadable"
                        );
                        return;
                    }
                };
                eprintln!(
                    "[lock-coord] TTY watcher active (desktop_tty={:?})",
                    desktop_tty
                );
                let mut last_tty = Some(desktop_tty.clone());
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    let now_tty = read_tty();
                    if now_tty == last_tty {
                        continue;
                    }
                    // Transition: only act on return-to-desktop.
                    if now_tty.as_deref() == Some(desktop_tty.as_str())
                        && last_tty.as_deref() != Some(desktop_tty.as_str())
                    {
                        eprintln!(
                            "[lock-coord] TTY return to desktop (from {:?}): respawning",
                            last_tty
                        );
                        let current_primary: Option<String> =
                            primary_tty.lock().ok().and_then(|g| g.clone());
                        let mut outputs: Vec<(String, bool)> = Vec::new();
                        if let Ok(mut g) = children_tty.lock() {
                            for (output, mut child) in g.drain() {
                                let _ = child.kill();
                                let _ = child.wait();
                                let is_primary =
                                    current_primary.as_deref() == Some(output.as_str());
                                outputs.push((output, is_primary));
                            }
                        }
                        // Primary first.
                        outputs.sort_by_key(|(_, is_primary)| !*is_primary);
                        // Let sway settle before spawning fresh surfaces.
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        for (idx, (output, is_primary)) in outputs.iter().enumerate() {
                            if idx > 0 {
                                tokio::time::sleep(Duration::from_millis(200)).await;
                            }
                            eprintln!(
                                "[lock-coord] TTY respawn: {:?} primary={}",
                                output, is_primary
                            );
                            Self::spawn_child(
                                output,
                                &socket_tty,
                                &children_tty,
                                *is_primary,
                            );
                        }
                    }
                    last_tty = now_tty;
                }
            });

            // ── Watchdog: respawn dead children ──
            // A dead child = dead lock surface = visible desktop = the
            // security hole the user reported. This polls every 200ms;
            // worst-case unlock window is ~200ms + child boot time.
            // Mitigation, not cure — check /tmp/mbs-standalone-lockscreen.log
            // for the underlying panic (RUST_BACKTRACE is set on children).
            let children_watchdog = children.clone();
            let socket_watchdog = socket_path.clone();
            let primary_output_watchdog = primary_output.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(200));
                interval.tick().await;
                loop {
                    interval.tick().await;
                    let mut dead: Vec<String> = Vec::new();
                    {
                        let mut map = children_watchdog.lock().unwrap();
                        for (output, child) in map.iter_mut() {
                            match child.try_wait() {
                                Ok(Some(status)) => {
                                    eprintln!(
                                        "[lock-coord] WATCHDOG: child output={:?} died: {:?}",
                                        output, status
                                    );
                                    dead.push(output.clone());
                                }
                                Ok(None) => {}
                                Err(e) => eprintln!(
                                    "[lock-coord] WATCHDOG: try_wait {:?} err: {}",
                                    output, e
                                ),
                            }
                        }
                        for output in &dead {
                            map.remove(output);
                        }
                    }
                    for output in dead {
                        let is_primary = primary_output_watchdog
                            .lock()
                            .unwrap()
                            .as_deref()
                            == Some(output.as_str());
                        eprintln!(
                            "[lock-coord] WATCHDOG: respawning {:?} primary={}",
                            output, is_primary
                        );
                        Self::spawn_child(
                            &output,
                            &socket_watchdog,
                            &children_watchdog,
                            is_primary,
                        );
                    }
                }
            });

            // ── Signal handlers ──
            // logind/PAM/session events sometimes send SIGUSR1/USR2/SIGHUP.
            // Default disposition is TERMINATE. That would kill the
            // coordinator, orphan every child, and unlock the screen.
            // Ignore them (log for diagnosis).
            let children_sig = children.clone();
            let socket_path_sig = socket_path.clone();
            tokio::spawn(async move {
                use tokio::signal::unix::{signal, SignalKind};
                let Ok(mut usr1) = signal(SignalKind::user_defined1()) else { return; };
                let Ok(mut usr2) = signal(SignalKind::user_defined2()) else { return; };
                let Ok(mut hup)  = signal(SignalKind::hangup())      else { return; };
                let Ok(mut term) = signal(SignalKind::terminate())   else { return; };
                let Ok(mut intr) = signal(SignalKind::interrupt())   else { return; };
                loop {
                    tokio::select! {
                        _ = usr1.recv() => eprintln!("[lock-coord] SIGUSR1 ignored"),
                        _ = usr2.recv() => eprintln!("[lock-coord] SIGUSR2 ignored"),
                        _ = hup.recv()  => eprintln!("[lock-coord] SIGHUP ignored"),
                        _ = term.recv() => {
                            eprintln!("[lock-coord] SIGTERM received — shutting down cleanly");
                            shutdown_lockscreen_children(&children_sig, &socket_path_sig);
                            std::process::exit(0);
                        },
                        _ = intr.recv() => {
                            eprintln!("[lock-coord] SIGINT received — shutting down cleanly");
                            shutdown_lockscreen_children(&children_sig, &socket_path_sig);
                            std::process::exit(0);
                        },
                    }
                }
            });

            loop {
                tokio::select! {
                    Ok((stream, _)) = listener.accept() => {
                        let id = next_id;
                        next_id += 1;
                        let (reader, writer) = stream.into_split();
                        let tx = event_tx.clone();
                        tokio::spawn(async move {
                            let mut lines = BufReader::new(reader).lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if let Ok(msg) = serde_json::from_str::<LockIpc>(&line) {
                                    if tx.send((id, msg)).await.is_err() { break; }
                                }
                            }
                        });
                        // Send current buffer immediately
                        let mut w = writer;
                        let msg = LockIpc::BufferUpdate(buffer.chars().count());
                        let _ = Self::write_msg(&mut w, &msg).await;
                        peers.push(Peer { id, writer: w });
                        eprintln!("[lock-coord] peer {} connected ({} total)", id, peers.len());
                    }

                    Some((peer_id, msg)) = event_rx.recv() => {
                        match msg {
                            LockIpc::Key(s) => {
                                // Finding 35: cap buffer + per-message length.
                                const MAX_BUF: usize = 256;
                                const MAX_PER_KEY: usize = 64;
                                let room = MAX_BUF.saturating_sub(buffer.chars().count());
                                if room == 0 {
                                    eprintln!(
                                        "[lock-coord] buffer at cap ({}), ignoring Key",
                                        MAX_BUF
                                    );
                                } else {
                                    let take = MAX_PER_KEY.min(room);
                                    let added: usize =
                                        s.chars().take(take).map(|c| buffer.push(c)).count();
                                    if std::env::var_os("MBS_LOCK_DEBUG").is_some() {
                                        eprintln!(
                                            "[lock-coord] key from peer {}: +{} chars → len={}",
                                            peer_id,
                                            added,
                                            buffer.chars().count()
                                        );
                                    }
                                    let n = buffer.chars().count();
                                    Self::broadcast(&mut peers, &LockIpc::BufferUpdate(n)).await;
                                }
                            }
                            LockIpc::Backspace => {
                                buffer.pop();
                                let n = buffer.chars().count();
                                Self::broadcast(&mut peers, &LockIpc::BufferUpdate(n)).await;
                            }
                            LockIpc::Submit => {
                                // Finding 30: rate-limit auth spawns to prevent
                                // hostile same-user Submit spam from tripping
                                // PAM/sudo account-lockout (pam_faillock).
                                //
                                // Empty buffer: rejected BEFORE the streak
                                // counter so an attacker sending empty Submit
                                // can't inflate fail_streak and rate-limit the
                                // legit user without any real PAM attempt.
                                //
                                // Backoff: min(1s * 2^fail_streak, 16s). First
                                // attempt is never rate-limited.
                                //
                                // Correction to earlier claim: this caps at 16s
                                // after 4 failures, but a same-user attacker
                                // holding the coordinator socket can still reach
                                // 5+ submits in ~3s (1+2+4), which trips
                                // pam_faillock's tighter Fedora/RHEL default of
                                // 3-per-10min. The backoff defends against rapid
                                // PAM spawn from a compromised shell, not against
                                // a determined same-user attacker who already has
                                // socket access. Mitigating that fully requires a
                                // hard cap at 2 failures (breaks legitimate UX)
                                // or a pam_faillock exemption for the pamtester
                                // service. Neither is implemented.
                                let now = std::time::Instant::now();
                                let exponent = fail_streak.min(4);
                                let min_delay = Duration::from_millis(
                                    1000u64.saturating_mul(1u64 << exponent),
                                );
                                let too_soon = last_failed_submit
                                    .map(|prev| now.duration_since(prev) < min_delay)
                                    .unwrap_or(false);
                                if buffer.is_empty() {
                                    eprintln!(
                                        "[lock-coord] ignoring empty Submit (finding 30)"
                                    );
                                } else if too_soon {
                                    eprintln!(
                                        "[lock-coord] Submit rate-limited (fail_streak={}, min_delay={:?})",
                                        fail_streak, min_delay
                                    );
                                } else {
                                    eprintln!(
                                        "[lock-coord] submit from peer {} (len={})",
                                        peer_id, buffer.chars().count()
                                    );
                                    let ok = Self::authenticate(&buffer).await;
                                    if ok {
                                        eprintln!("[lock-coord] SUCCESS");
                                        Self::broadcast(&mut peers, &LockIpc::Success).await;
                                        tokio::time::sleep(Duration::from_millis(200)).await;
                                        Self::kill_all(&children);
                                        let _ = std::fs::remove_file(&socket_path);
                                        // std::process::exit skips Drop. Zeroize
                                        // explicitly so the successful password
                                        // isn't left in the process's freed heap.
                                        buffer.zeroize();
                                        std::process::exit(0);
                                    } else {
                                        fail_streak = fail_streak.saturating_add(1);
                                        last_failed_submit = Some(now);
                                        eprintln!(
                                            "[lock-coord] auth failed (fail_streak={})",
                                            fail_streak
                                        );
                                        // Zeroize before clear: String::clear
                                        // only sets len=0, leaving the password
                                        // bytes in the freed-but-not-overwritten
                                        // capacity. A same-user process with
                                        // ptrace access can recover the password
                                        // from /proc/<pid>/mem otherwise.
                                        buffer.zeroize();
                                        buffer.clear();
                                        Self::broadcast(&mut peers, &LockIpc::Reset).await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    else => break,
                }
            }
            std::process::exit(0);
        })
    }

    async fn write_msg(w: &mut OwnedWriteHalf, msg: &LockIpc) -> std::io::Result<()> {
        let mut line = serde_json::to_string(msg).unwrap_or_default();
        line.push('\n');
        w.write_all(line.as_bytes()).await
    }

    async fn broadcast(peers: &mut Vec<Peer>, msg: &LockIpc) {
        let mut dead = Vec::new();
        for (i, peer) in peers.iter_mut().enumerate() {
            if Self::write_msg(&mut peer.writer, msg).await.is_err() {
                dead.push(i);
            }
        }
        for i in dead.into_iter().rev() {
            let p = peers.remove(i);
            eprintln!("[lock-coord] peer {} removed", p.id);
        }
    }

    fn spawn_child(
        output: &str,
        socket_path: &PathBuf,
        children: &Arc<Mutex<HashMap<String, Child>>>,
        is_primary: bool,
    ) {
        if let Ok(map) = children.lock() {
            if map.contains_key(output) {
                return;
            }
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = Command::new(&exe);
        cmd.arg("--lockscreen-instance")
            .env("MBS_LOCK_SOCKET", socket_path)
            .env("MBS_LOCK_PRIMARY", if is_primary { "1" } else { "0" })
            .env("RUST_BACKTRACE", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if !output.is_empty() {
            cmd.arg(output).env("MBS_LOCK_OUTPUT", output);
        }
        match cmd.spawn() {
            Ok(child) => {
                eprintln!(
                    "[lock-coord] spawned pid={} for output={:?}",
                    child.id(),
                    output
                );
                if let Ok(mut map) = children.lock() {
                    map.insert(output.to_string(), child);
                }
            }
            Err(e) => eprintln!("[lock-coord] spawn failed for {:?}: {}", output, e),
        }
    }

    fn kill_all(children: &Arc<Mutex<HashMap<String, Child>>>) {
        if let Ok(mut map) = children.lock() {
            for (_name, mut c) in map.drain() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }

    async fn authenticate(password: &str) -> bool {
        use tokio::io::AsyncWriteExt as _;
        let clean = password.trim();
        if clean.is_empty() {
            eprintln!("[lock-coord] auth: empty password");
            return false;
        }

        // ── Debug bypass: compile-time opt-in only (finding 05) ──
        // Release builds: no code path exists. To diagnose in a dev
        // build: cargo build --features lock-debug-bypass
        #[cfg(feature = "lock-debug-bypass")]
        if let Ok(debug_pass) = std::env::var("MBS_LOCK_DEBUG_PASS") {
            if !debug_pass.is_empty() && debug_pass == clean {
                eprintln!("[lock-coord] DEBUG BYPASS (compile-time feature): matched");
                return true;
            }
        }

        if std::env::var_os("MBS_LOCK_DEBUG").is_some() {
            eprintln!(
                "[lock-coord] auth attempt: len={} sha256_prefix={}",
                clean.chars().count(),
                sha256_prefix(clean),
            );
        } else {
            eprintln!("[lock-coord] auth attempt: len={}", clean.chars().count());
        }

        // Zeroizing: overwrites the password bytes on drop, including
        // through early returns and panics. Without this, the freshly
        // formatted String sits in freed heap until overwritten.
        let pass_line = Zeroizing::new(format!("{}\n", clean));

        // ── Strategy 0: pamtester ──
        // Talks to real PAM auth (the same stack login(1) uses). The only
        // strategy in this file guaranteed to require the actual password.
        // Try it first.
        {
            let user = resolve_user();
            let pam_bin =
                find_bin("pamtester").unwrap_or_else(|| std::path::PathBuf::from("pamtester"));
            eprintln!("[lock-coord] trying pamtester login {} authenticate", user);
            match tokio::process::Command::new(&pam_bin)
                .args(["login", &user, "authenticate"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut c) => {
                    if let Some(mut stdin) = c.stdin.take() {
                        let _ = stdin.write_all(pass_line.as_bytes()).await;
                        drop(stdin);
                    }
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        c.wait_with_output(),
                    )
                    .await
                    {
                        Ok(Ok(out)) => {
                            let success = out.status.success();
                            let err = String::from_utf8_lossy(&out.stderr);
                            eprintln!(
                                "[lock-coord] pamtester exit={:?} success={} stderr={:?}",
                                out.status.code(),
                                success,
                                err.trim().chars().take(200).collect::<String>()
                            );
                            if success {
                                return true;
                            }
                        }
                        Ok(Err(e)) => eprintln!("[lock-coord] pamtester wait: {}", e),
                        Err(_) => eprintln!("[lock-coord] pamtester TIMEOUT"),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("[lock-coord] pamtester not installed");
                }
                Err(e) => eprintln!("[lock-coord] pamtester spawn: {}", e),
            }
        }

        // ── Strategy 1: sudo -S -k -v, guarded by NOPASSWD check ──
        // `sudo -v` returns 0 with ANY input on hosts where the invoking
        // user has a NOPASSWD sudoers rule (common: wheel, docker,
        // podman). Detect this and skip sudo entirely — it cannot serve
        // as an auth oracle when it never asks for a password.
        if sudo_is_nopasswd() {
            eprintln!("[lock-coord] sudo is NOPASSWD — refusing to use it as auth oracle");
        } else {
            let sudo_bin = find_bin("sudo").unwrap_or_else(|| std::path::PathBuf::from("sudo"));
            eprintln!("[lock-coord] trying sudo -v");
            match tokio::process::Command::new(&sudo_bin)
                .args(["-S", "-k", "-v"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut c) => {
                    if let Some(mut stdin) = c.stdin.take() {
                        let _ = stdin.write_all(pass_line.as_bytes()).await;
                        drop(stdin);
                    }
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(5),
                        c.wait_with_output(),
                    )
                    .await
                    {
                        Ok(Ok(out)) => {
                            let success = out.status.success();
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            eprintln!(
                                "[lock-coord] sudo -v exit={:?} success={} stderr={:?}",
                                out.status.code(),
                                success,
                                stderr.trim().chars().take(200).collect::<String>()
                            );
                            if success {
                                return true;
                            }
                        }
                        Ok(Err(e)) => eprintln!("[lock-coord] sudo wait: {}", e),
                        Err(_) => eprintln!("[lock-coord] sudo TIMEOUT"),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("[lock-coord] sudo not installed");
                }
                Err(e) => eprintln!("[lock-coord] sudo spawn: {}", e),
            }
        }

        // Strategies 2 (script + sudo -k -v) and 3 (su -c true <self>)
        // are REMOVED:
        //   - Strategy 2 has identical bypass semantics to the sudo check
        //     above (a NOPASSWD rule makes both succeed with any input)
        //     and is redundant with the guarded sudo.
        //   - Strategy 3 succeeds WITHOUT prompting when <user> equals
        //     the invoking user (no user transition = no PAM check), so
        //     it is a bypass on every Linux host with util-linux su.

        eprintln!("[lock-coord] ALL AUTH STRATEGIES FAILED");
        false
    }

    fn list_outputs() -> Vec<String> {
        let sway_bin = find_bin("swaymsg").unwrap_or_else(|| std::path::PathBuf::from("swaymsg"));
        let out = Command::new(&sway_bin)
            .args(["-t", "get_outputs", "-r"])
            .output()
            .ok();
        let Some(out) = out else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
            return Vec::new();
        };
        let Some(arr) = v.as_array() else {
            return Vec::new();
        };
        arr.iter()
            .filter(|o| o.get("active").and_then(|a| a.as_bool()) != Some(false))
            .filter_map(|o| o.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect()
    }
}
