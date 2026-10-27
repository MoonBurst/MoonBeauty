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
        "pamtester" => &["/run/current-system/sw/bin/pamtester",
                         "/usr/bin/pamtester", "/usr/local/bin/pamtester",
                         "/nix/var/nix/profiles/default/bin/pamtester"],
        "sudo"      => &["/run/wrappers/bin/sudo", "/usr/bin/sudo", "/bin/sudo"],
        "su"        => &["/run/wrappers/bin/su", "/usr/bin/su", "/bin/su"],
        "swaymsg"   => &["/run/current-system/sw/bin/swaymsg", "/usr/bin/swaymsg"],
        "id"        => &["/usr/bin/id", "/bin/id"],
        "script"    => &["/usr/bin/script", "/bin/script"],
        "which"     => &["/usr/bin/which", "/bin/which"],
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
            match tokio::process::Command::new("which").arg("sudo").output().await {
                Ok(out) => eprintln!("sudo = {:?}", String::from_utf8_lossy(&out.stdout).trim()),
                Err(_) => eprintln!("sudo = NOT FOUND"),
            }
            match tokio::process::Command::new("which").arg("pamtester").output().await {
                Ok(out) => {
                    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if p.is_empty() { eprintln!("pamtester = not installed"); }
                    else { eprintln!("pamtester = {:?}", p); }
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
            let password = input.trim_end_matches(['\r', '\n']).to_string();
            eprintln!("len={} first={:?} last={:?}",
                password.chars().count(),
                password.chars().next(),
                password.chars().last());
            eprintln!();

            // Run authenticate with full logging
            eprintln!("── Running strategies ──");
            let ok = Self::authenticate(&password).await;
            eprintln!();
            if ok {
                eprintln!("RESULT: ✓ auth succeeded");
                std::process::exit(0);
            } else {
                eprintln!("RESULT: ✗ all strategies failed");
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
        let socket_path = PathBuf::from(&runtime_dir)
            .join(format!("moon-beauty-lock-{}.sock", pid));
        let _ = std::fs::remove_file(&socket_path);

        if let Ok(v) = std::env::var("MBS_LOCK_DEBUG_PASS") {
            eprintln!("[lock-coord] WARNING: MBS_LOCK_DEBUG_PASS is set (len={}); bypass armed", v.len());
        }

        {
            let u = resolve_user();
            eprintln!("[lock-coord] user resolved via id -un → {}", u);
        }

        let children: Arc<Mutex<HashMap<String, Child>>> =
            Arc::new(Mutex::new(HashMap::new()));


        // Startup sanity: verify sudo is on PATH
        {
            let check = std::process::Command::new("which").arg("sudo").output();
            match check {
                Ok(out) => {
                    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if path.is_empty() {
                        eprintln!("[lock-coord] WARN: sudo not found on PATH");
                    } else {
                        eprintln!("[lock-coord] sudo binary: {}", path);
                    }
                }
                Err(e) => eprintln!("[lock-coord] WARN: which sudo failed: {}", e),
            }
        }

        eprintln!("[lock-coord-v225] binding socket at {:?}", socket_path);
        let listener = match std::os::unix::net::UnixListener::bind(&socket_path) {
            Ok(l) => {
                let _ = std::fs::set_permissions(
                    &socket_path,
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::Permissions::from_mode(0o600)
                    },
                );
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
            Self::spawn_child("", &socket_path, &children);
        } else {
            eprintln!("[lock-coord] spawning {} children for {:?}", outputs.len(), outputs);
            for output in &outputs {
                eprintln!("[lock-coord-v225] spawning child for output={:?}", output);
            Self::spawn_child(output, &socket_path, &children);
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

            // Hotplug watcher
            let children_watch = children.clone();
            let socket_watch = socket_path.clone();
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
                            Self::spawn_child(&output, &socket_watch, &children_watch);
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


            // ── Watchdog: respawn dead children ──
            // A dead child = dead lock surface = visible desktop = the
            // security hole the user reported. This polls every 200ms;
            // worst-case unlock window is ~200ms + child boot time.
            // Mitigation, not cure — check /tmp/mbs-standalone-lockscreen.log
            // for the underlying panic (RUST_BACKTRACE is set on children).
            let children_watchdog = children.clone();
            let socket_watchdog = socket_path.clone();
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
                        eprintln!("[lock-coord] WATCHDOG: respawning {:?}", output);
                        Self::spawn_child(&output, &socket_watchdog, &children_watchdog);
                    }
                }
            });

            // ── Signal handlers ──
            // logind/PAM/session events sometimes send SIGUSR1/USR2/SIGHUP.
            // Default disposition is TERMINATE. That would kill the
            // coordinator, orphan every child, and unlock the screen.
            // Ignore them (log for diagnosis).
            tokio::spawn(async move {
                use tokio::signal::unix::{signal, SignalKind};
                let Ok(mut usr1) = signal(SignalKind::user_defined1()) else { return; };
                let Ok(mut usr2) = signal(SignalKind::user_defined2()) else { return; };
                let Ok(mut hup)  = signal(SignalKind::hangup())      else { return; };
                loop {
                    tokio::select! {
                        _ = usr1.recv() => eprintln!("[lock-coord] SIGUSR1 ignored"),
                        _ = usr2.recv() => eprintln!("[lock-coord] SIGUSR2 ignored"),
                        _ = hup.recv()  => eprintln!("[lock-coord] SIGHUP ignored"),
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
                                let added = s.chars().count();
                                for c in s.chars() {
                                    buffer.push(c);
                                }
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
                            LockIpc::Backspace => {
                                buffer.pop();
                                let n = buffer.chars().count();
                                Self::broadcast(&mut peers, &LockIpc::BufferUpdate(n)).await;
                            }
                            LockIpc::Submit => {
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
                                    std::process::exit(0);
                                } else {
                                    eprintln!("[lock-coord] auth failed");
                                    buffer.clear();
                                    Self::broadcast(&mut peers, &LockIpc::Reset).await;
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

    fn spawn_child(output: &str, socket_path: &PathBuf, children: &Arc<Mutex<HashMap<String, Child>>>) {
        if let Ok(map) = children.lock() {
            if map.contains_key(output) {
                return;
            }
        }
        let Ok(exe) = std::env::current_exe() else { return; };
        let mut cmd = Command::new(&exe);
        cmd.arg("--lockscreen-instance")
            .env("MBS_LOCK_SOCKET", socket_path)
            .env("RUST_BACKTRACE", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if !output.is_empty() {
            cmd.arg(output).env("MBS_LOCK_OUTPUT", output);
        }
        match cmd.spawn() {
            Ok(child) => {
                eprintln!("[lock-coord] spawned pid={} for output={:?}", child.id(), output);
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

        // ── Debug bypass (requires BOTH MBS_DEBUG and MBS_LOCK_DEBUG_PASS) ──
        if let Ok(debug_pass) = std::env::var("MBS_LOCK_DEBUG_PASS") {
            if !debug_pass.is_empty() {
                if std::env::var_os("MBS_DEBUG").is_none() {
                    eprintln!("[lock-coord] WARNING: MBS_LOCK_DEBUG_PASS set without MBS_DEBUG; bypass DISABLED");
                } else if debug_pass == clean {
                    eprintln!("[lock-coord] DEBUG BYPASS: matched");
                    return true;
                } else {
                    eprintln!(
                        "[lock-coord] DEBUG BYPASS: mismatch (buf len={}, debug len={})",
                        clean.len(), debug_pass.len()
                    );
                }
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

        let pass_line = format!("{}\n", clean);

        // ── Strategy 0: sudo -S -k -v ──
        // Wheel users: sudo is the most reliable auth. pamtester requires
        // `security.pam.services.login` to be configured, which many minimal
        // NixOS configs don't have. Try sudo first.
        {
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
                    match tokio::time::timeout(std::time::Duration::from_secs(5), c.wait_with_output()).await {
                        Ok(Ok(out)) => {
                            let success = out.status.success();
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            eprintln!(
                                "[lock-coord] sudo -v exit={:?} success={} stderr={:?}",
                                out.status.code(), success,
                                stderr.trim().chars().take(200).collect::<String>()
                            );
                            if success { return true; }
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

        // ── Strategy 1: pamtester ──
        {
            let user = resolve_user();
            let pam_bin = find_bin("pamtester").unwrap_or_else(|| std::path::PathBuf::from("pamtester"));
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
                    match tokio::time::timeout(std::time::Duration::from_secs(5), c.wait_with_output()).await {
                        Ok(Ok(out)) => {
                            let success = out.status.success();
                            let err = String::from_utf8_lossy(&out.stderr);
                            eprintln!(
                                "[lock-coord] pamtester exit={:?} success={} stderr={:?}",
                                out.status.code(), success,
                                err.trim().chars().take(200).collect::<String>()
                            );
                            if success { return true; }
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

        // ── Strategy 2: sudo via script(1) PTY ──
        {
            let script_bin = find_bin("script").unwrap_or_else(|| std::path::PathBuf::from("script"));
            eprintln!("[lock-coord] trying sudo via script(1)");
            match tokio::process::Command::new(&script_bin)
                .args(["-q", "-e", "-c", "sudo -k -v", "/dev/null"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut c) => {
                    if let Some(mut stdin) = c.stdin.take() {
                        let _ = stdin.write_all(pass_line.as_bytes()).await;
                        drop(stdin);
                    }
                    match tokio::time::timeout(std::time::Duration::from_secs(5), c.wait_with_output()).await {
                        Ok(Ok(out)) => {
                            let success = out.status.success();
                            eprintln!("[lock-coord] script+sudo exit={:?} success={}", out.status.code(), success);
                            if success { return true; }
                        }
                        Ok(Err(e)) => eprintln!("[lock-coord] script wait: {}", e),
                        Err(_) => eprintln!("[lock-coord] script+sudo TIMEOUT"),
                    }
                }
                Err(e) => eprintln!("[lock-coord] script spawn: {}", e),
            }
        }

        // ── Strategy 3: su via script(1) PTY ──
        {
            let user = resolve_user();
            eprintln!("[lock-coord] trying su via script(1)");
            match tokio::process::Command::new("script")
                .args(["-q", "-e", "-c", &format!("su -c true {}", user), "/dev/null"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut c) => {
                    if let Some(mut stdin) = c.stdin.take() {
                        let _ = stdin.write_all(pass_line.as_bytes()).await;
                        drop(stdin);
                    }
                    match tokio::time::timeout(std::time::Duration::from_secs(5), c.wait_with_output()).await {
                        Ok(Ok(out)) => {
                            let success = out.status.success();
                            eprintln!("[lock-coord] su-script exit={:?} success={}", out.status.code(), success);
                            return success;
                        }
                        Ok(Err(e)) => eprintln!("[lock-coord] su-script wait: {}", e),
                        Err(_) => eprintln!("[lock-coord] su-script TIMEOUT"),
                    }
                }
                Err(e) => eprintln!("[lock-coord] su-script spawn: {}", e),
            }
        }

        eprintln!("[lock-coord] ALL AUTH STRATEGIES FAILED");
        false
    }

    fn list_outputs() -> Vec<String> {
        let sway_bin = find_bin("swaymsg").unwrap_or_else(|| std::path::PathBuf::from("swaymsg"));
        let out = Command::new(&sway_bin)
            .args(["-t", "get_outputs", "-r"])
            .output()
            .ok();
        let Some(out) = out else { return Vec::new(); };
        if !out.status.success() {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
            return Vec::new();
        };
        let Some(arr) = v.as_array() else { return Vec::new(); };
        arr.iter()
            .filter(|o| o.get("active").and_then(|a| a.as_bool()) != Some(false))
            .filter_map(|o| o.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect()
    }
}
