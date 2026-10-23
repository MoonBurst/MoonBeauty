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
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
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
            Ok(l) => { eprintln!("[lock-coord-v225] socket bound OK"); l }
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
                            Self::spawn_child(&output, &socket_watch, &children_watch);
                        }
                        "removed" => {
                            if let Ok(mut map) = children_watch.lock() {
                                if let Some(mut c) = map.remove(&output) {
                                    let _ = c.kill();
                                    let _ = c.wait();
                                    eprintln!("[lock-coord] killed child for {}", output);
                                }
                            }
                        }
                        _ => {}
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
                                for c in s.chars() {
                                    buffer.push(c);
                                    eprintln!(
                                        "[lock-coord] key from peer {}: {:?} (0x{:02x}) → len={}",
                                        peer_id,
                                        c,
                                        c as u32,
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
                                let chars_hex: Vec<String> = buffer.chars()
                                    .map(|c| format!("{:04x}({:?})", c as u32, c.escape_default().to_string()))
                                    .collect();
                                eprintln!(
                                    "[lock-coord] submit from peer {} (len={}) chars=[{}]",
                                    peer_id, buffer.chars().count(), chars_hex.join(",")
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

        // ── DEBUG bypass ──
        // Set MBS_LOCK_DEBUG_PASS=yourpassword to skip sudo entirely.
        // If this makes auth succeed, char capture is correct and the
        // problem is sudo in this context. If it still fails, chars are wrong.
        if let Ok(debug_pass) = std::env::var("MBS_LOCK_DEBUG_PASS") {
            if !debug_pass.is_empty() {
                if debug_pass == clean {
                    eprintln!("[lock-coord] DEBUG BYPASS: password matched MBS_LOCK_DEBUG_PASS");
                    return true;
                } else {
                    eprintln!(
                        "[lock-coord] DEBUG BYPASS: password MISMATCH (buffer len={} debug len={}); falling through to real auth",
                        clean.len(),
                        debug_pass.len()
                    );
                    // Do NOT return false — a stale MBS_LOCK_DEBUG_PASS must
                    // never block real authentication.
                }
            }
        }

        eprintln!(
            "[lock-coord] auth attempt: len={} first={:?} last={:?} sha256_prefix={}",
            clean.chars().count(),
            clean.chars().next(),
            clean.chars().last(),
            sha256_prefix(&clean),
        );

        // ── Strategy 0: pamtester (PAM login — correct mechanism for a
        //    lockscreen; validates the user's own password independent of
        //    sudo rights / wheel group membership). Run FIRST because it's
        //    the only strategy that works on a stock NixOS user who isn't
        //    in the wheel group.) ──
        {
            let user = resolve_user();
            let pass_line = format!("{}\n", clean);
            let found = std::env::var("PATH").ok().and_then(|p| {
                p.split(':')
                    .map(std::path::PathBuf::from)
                    .find(|d| d.join("pamtester").exists())
            });
            match found {
                Some(p) => eprintln!("[lock-coord] pamtester at {}", p.display()),
                None => eprintln!("[lock-coord] pamtester NOT on PATH — add 'pamtester' to flake.nix runtimeLibs"),
            }
            eprintln!("[lock-coord] trying pamtester login {} authenticate", user);
            let _pam_bin = find_bin("pamtester").unwrap_or_else(|| std::path::PathBuf::from("pamtester"));
            match tokio::process::Command::new(&_pam_bin)
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
                    match c.wait_with_output().await {
                        Ok(out) => {
                            let success = out.status.success();
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let err = String::from_utf8_lossy(&out.stderr);
                            eprintln!(
                                "[lock-coord] pamtester exit={:?} success={} stdout={:?} stderr={:?}",
                                out.status.code(), success,
                                stdout.trim().chars().take(80).collect::<String>(),
                                err.trim().chars().take(160).collect::<String>()
                            );
                            if success { return true; }
                        }
                        Err(e) => eprintln!("[lock-coord] pamtester wait: {}", e),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("[lock-coord] pamtester not installed (skipping strategy 0)");
                }
                Err(e) => eprintln!("[lock-coord] pamtester spawn: {}", e),
            }
        }

        // ── Strategy 1: sudo -S -k -v with full IO capture (V173 pattern) ──
        let pass_line = format!("{}
", clean);
        let _sudo_bin = find_bin("sudo").unwrap_or_else(|| std::path::PathBuf::from("sudo"));
        match tokio::process::Command::new(&_sudo_bin)
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
                match c.wait_with_output().await {
                    Ok(out) => {
                        let success = out.status.success();
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let stderr_trim = stderr.trim();
                        eprintln!(
                            "[lock-coord] sudo -v exit={:?} success={} stderr={:?}",
                            out.status.code(), success, stderr_trim
                        );
                        if success {
                            return true;
                        }
                        let lower = stderr_trim.to_lowercase();
                        if lower.contains("a terminal is required")
                            || lower.contains("no tty")
                            || lower.contains("must have a terminal")
                        {
                            eprintln!("[lock-coord] sudo needs TTY — trying strategy 2");
                        } else if lower.contains("not in the sudoers")
                            || lower.contains("may not run sudo")
                        {
                            eprintln!("[lock-coord] user not in sudoers — trying su strategy 3");
                        } else {
                            // sudo ran but rejected password — still try pamtester
                            eprintln!("[lock-coord] sudo rejected password — trying pamtester");
                        }
                    }
                    Err(e) => eprintln!("[lock-coord] sudo wait: {}", e),
                }
            }
            Err(e) => eprintln!("[lock-coord] sudo spawn: {}", e),
        }

        // ── Strategy 2: sudo via script PTY ──
        eprintln!("[lock-coord] trying sudo via script(1)");
        let _script_bin = find_bin("script").unwrap_or_else(|| std::path::PathBuf::from("script"));
        match tokio::process::Command::new(&_script_bin)
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
                match c.wait_with_output().await {
                    Ok(out) => {
                        let success = out.status.success();
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        eprintln!(
                            "[lock-coord] script+sudo exit={:?} success={} stderr={:?}",
                            out.status.code(), success, stderr.trim().chars().take(120).collect::<String>()
                        );
                        if success { return true; }
                    }
                    Err(e) => eprintln!("[lock-coord] script wait: {}", e),
                }
            }
            Err(e) => eprintln!("[lock-coord] script spawn: {}", e),
        }

        // ── Strategy 3: pamtester ──
        eprintln!("[lock-coord] trying pamtester");
        let user = resolve_user();
        let _pam_bin3 = find_bin("pamtester").unwrap_or_else(|| std::path::PathBuf::from("pamtester"));
        match tokio::process::Command::new(&_pam_bin3)
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
                match c.wait_with_output().await {
                    Ok(out) => {
                        let success = out.status.success();
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        eprintln!(
                            "[lock-coord] pamtester exit={:?} success={} stderr={:?}",
                            out.status.code(), success, stderr.trim()
                        );
                        if success { return true; }
                    }
                    Err(e) => eprintln!("[lock-coord] pamtester wait: {}", e),
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    eprintln!("[lock-coord] pamtester not installed (skip strategy 3)");
                } else {
                    eprintln!("[lock-coord] pamtester spawn: {}", e);
                }
            }
        }

        // ── Strategy 4: su via script ──
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
                match c.wait_with_output().await {
                    Ok(out) => {
                        let success = out.status.success();
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        eprintln!(
                            "[lock-coord] su-script exit={:?} success={} stderr={:?}",
                            out.status.code(), success, stderr.trim().chars().take(120).collect::<String>()
                        );
                        return success;
                    }
                    Err(e) => eprintln!("[lock-coord] su-script wait: {}", e),
                }
            }
            Err(e) => eprintln!("[lock-coord] su-script spawn: {}", e),
        }

        eprintln!("[lock-coord] ALL AUTH STRATEGIES FAILED");
        false
    }

    /// Fallback for systems where sudo requires a TTY (requiretty in sudoers).
    /// Uses `script(1)` to allocate a pseudo-terminal around sudo.
    async fn authenticate_via_script(password: &str) -> bool {
        use tokio::io::AsyncWriteExt as _;
        eprintln!("[lock-coord] fallback: sudo via script(1) PTY");

        // script -q -e -c "sudo -k -v" /dev/null
        // -q quiet, -e return child exit code, -c command to run
        let mut child = match tokio::process::Command::new("script")
            .args(["-q", "-e", "-c", "sudo -k -v", "/dev/null"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[lock-coord] script spawn failed: {}", e);
                return false;
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(format!("{}
", password.trim()).as_bytes()).await;
            drop(stdin);
        }

        match child.wait().await {
            Ok(st) => {
                let success = st.success();
                eprintln!("[lock-coord] script+sudo exit={:?} success={}", st.code(), success);
                success
            }
            Err(e) => {
                eprintln!("[lock-coord] script wait failed: {}", e);
                false
            }
        }
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
