//! Process utilities.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static SIGTERM_FLAG: AtomicBool = AtomicBool::new(false);
static WATCHDOG_ARMED: AtomicBool = AtomicBool::new(false);

extern "C" fn sigterm_handler(_sig: libc::c_int) {
    // Async-signal-safe: single atomic store.
    SIGTERM_FLAG.store(true, Ordering::SeqCst);
}

/// Install a SIGTERM handler that sets a process-global flag. Idempotent.
/// Call once at app startup. Must be followed by `was_sigterm_signaled()`
/// polling in a tick loop; the handler itself cannot wake async timers.
pub fn install_sigterm_handler() {
    #[allow(unsafe_code)]
    unsafe {
        // fn -> *const () -> sighandler_t: the pointer indirection
        // silences rustc's "fn_ptr_cast" lint. The signal handler
        // signature (extern "C" fn(c_int)) matches sighandler_t.
        let handler = sigterm_handler as *const () as libc::sighandler_t;
        libc::signal(libc::SIGTERM, handler);
    }
    // Watchdog: force-exit 1.5s after SIGTERM if the async loop never
    // polls was_sigterm_signaled(). Without this, SIGTERM is a no-op on
    // any process whose event loop is idle or stuck.
    if WATCHDOG_ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if SIGTERM_FLAG.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(1400));
            #[allow(unsafe_code)]
            unsafe {
                libc::_exit(0);
            }
        }
    });
}

/// True after SIGTERM was received. Poll from a Tick handler.
pub fn was_sigterm_signaled() -> bool {
    SIGTERM_FLAG.load(Ordering::SeqCst)
}

/// Sanitize a folder name derived from an untrusted source (email
/// `From:` display name, IMAP folder name) before use as a path
/// component.
///
/// Every character not in `[A-Za-z0-9 _\-().\[\]]` is replaced with
/// `_`. Leading dots are stripped, residual `..` substrings are collapsed
/// to `_`, and result length is capped at 64 bytes. Empty becomes `"unnamed"`.
pub fn sanitize_folder_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric()
                || matches!(c, ' ' | '_' | '-' | '(' | ')' | '.' | '[' | ']')
            {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Strip leading dots: no hidden files, `.` and `..` impossible.
    out = out.trim_start_matches('.').to_string();
    // Collapse any residual `..` (e.g. `a..b`) into `_`.
    while out.contains("..") {
        out = out.replace("..", "_");
    }
    out = out.trim().to_string();
    if out.is_empty() {
        out = "unnamed".to_string();
    }
    if out.len() > 64 {
        out.truncate(64);
    }
    out
}

/// Ensure `path` exists and is a directory at mode 0700. If it already
/// exists with looser permissions, tightens them.
fn ensure_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    let mut perms = std::fs::metadata(path)?.permissions();
    if perms.mode() & 0o777 != 0o700 {
        perms.set_mode(0o700);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn chmod_mode(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = std::fs::metadata(path)?.permissions();
    if perms.mode() & 0o777 != mode {
        perms.set_mode(mode);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Overwrite `path` with `bytes` at mode 0600.
///
/// Unlike `std::fs::write` (which inherits umask), this enforces 0600
/// even on pre-existing files via an explicit `chmod_mode` after open.
pub fn private_write_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    let _ = chmod_mode(path, 0o600);
    f.write_all(bytes)
}

/// Read N bytes from /dev/urandom, falling back to a time+pid mix if unreadable.
fn urandom_bytes<const N: usize>() -> [u8; N] {
    use std::io::Read;
    let mut buf = [0u8; N];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let combined = nanos.rotate_left(64) ^ pid;
    let bytes = combined.to_ne_bytes();
    let copy = N.min(bytes.len());
    buf[..copy].copy_from_slice(&bytes[..copy]);
    buf
}

/// 16-hex-char random token suitable for unpredictable filename suffixes.
pub fn random_token() -> String {
    let bytes = urandom_bytes::<8>();
    let mut out = String::with_capacity(16);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// `$XDG_RUNTIME_DIR/mbs-tmp`, created at 0700.
pub fn mbs_tmp_dir() -> std::path::PathBuf {
    mbs_subdir("mbs-tmp")
}

/// Generate a unique path under `mbs_tmp_dir()`.
/// Callers must open via `private_create_new` / `private_write_new` (`O_CREAT | O_EXCL`).
pub fn mbs_unique_tmp(prefix: &str, ext: &str) -> std::path::PathBuf {
    mbs_tmp_dir().join(format!("{}-{}.{}", prefix, random_token(), ext))
}

/// True iff `path` resolves under `$XDG_RUNTIME_DIR/mbs-tmp/`.
/// Used by `--freeze-fragment` and the annotator handoff to reject
/// arbitrary paths. Canonicalizes parent when the file does not yet exist.
pub fn path_is_under_mbs_tmp(path: &std::path::Path) -> bool {
    let root = mbs_tmp_dir();
    let Ok(canon_root) = std::fs::canonicalize(&root) else {
        return false;
    };
    let canon = if path.exists() {
        std::fs::canonicalize(path).ok()
    } else {
        path.parent()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .and_then(|canon_parent| path.file_name().map(|n| canon_parent.join(n)))
    };
    canon.is_some_and(|c| c.starts_with(&canon_root))
}

/// Open `path` for writing with `O_CREAT | O_EXCL` at mode 0600.
/// Fails with `AlreadyExists` if any file or symlink exists at `path`.
pub fn private_create_new(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// `private_create_new` + write all bytes.
pub fn private_write_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut f = private_create_new(path)?;
    f.write_all(bytes)
}

/// Return `$XDG_RUNTIME_DIR` (ensured 0700). Panics if unset — refuses `/tmp` fallback.
pub fn mbs_runtime_dir() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            panic!(
                "XDG_RUNTIME_DIR is not set — refusing to fall back to /tmp \
                 for per-session state"
            )
        });
    let _ = ensure_private_dir(&dir);
    dir
}

/// `mbs_runtime_dir().join(name)`, created at 0700 if missing.
pub fn mbs_subdir(name: &str) -> std::path::PathBuf {
    let path = mbs_runtime_dir().join(name);
    let _ = ensure_private_dir(&path);
    path
}

/// Prune standalone log files from `mbs_tmp_dir()`.
/// Keep the 20 most recent, delete anything older than 1 hour.
pub fn prune_standalone_logs() {
    let dir = mbs_tmp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut logs: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            if name.starts_with("mbs-standalone-") && name.ends_with(".log") {
                let mtime = e.metadata().ok()?.modified().ok()?;
                Some((mtime, path))
            } else {
                None
            }
        })
        .collect();
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    logs.retain(|(mtime, path)| {
        if *mtime < cutoff {
            let _ = std::fs::remove_file(path);
            false
        } else {
            true
        }
    });
    if logs.len() > 20 {
        logs.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
        for (_, path) in logs.into_iter().skip(20) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Walk `$XDG_RUNTIME_DIR` and tighten every shell-owned (`mbs-*` / `moonbeauty*`)
/// file and directory to 0700 / 0600 on startup.
pub fn tighten_existing_runtime_state() {
    let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    tighten_existing_runtime_state_at(&xdg);
}

/// Testable variant: operates on an explicit root.
fn tighten_existing_runtime_state_at(xdg: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(xdg) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !(name.starts_with("mbs-") || name.starts_with("moonbeauty")) {
            continue;
        }
        if path.is_dir() {
            let _ = ensure_private_dir(&path);
            if let Ok(files) = std::fs::read_dir(&path) {
                for f in files.flatten() {
                    let fp = f.path();
                    if fp.is_file() {
                        let _ = chmod_mode(&fp, 0o600);
                    }
                }
            }
        } else if path.is_file() {
            let _ = chmod_mode(&path, 0o600);
        }
    }
}

pub fn is_safe_app_id(s: &str) -> bool {
    if s.is_empty() || s.contains("..") {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Canonicalize an icon candidate and verify it stays under its
/// declared root (or `/nix/store/` on NixOS). Blocks symlink escapes.
pub fn safe_icon_candidate(candidate: &std::path::Path, root: &std::path::Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(candidate).ok()?;
    let canon_root = std::fs::canonicalize(root).ok()?;
    // Also accept canonical targets under /nix/store/ because NixOS icons
    // are symlinks into the read-only content-addressed store.
    const NIX_STORE: &str = "/nix/store/";
    if canon.starts_with(&canon_root) || canon.starts_with(NIX_STORE) {
        Some(canon)
    } else {
        eprintln!(
            "[icon] symlink escape blocked: {:?} -> {:?}",
            candidate, canon
        );
        None
    }
}

/// Read a JSON file with a size cap. Returns None on missing / oversized / malformed.
pub fn read_json_capped<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
    max_bytes: u64,
) -> Option<T> {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > max_bytes => {
            eprintln!(
                "[json] {} is {} bytes, exceeds {} cap; skipping",
                path.display(),
                m.len(),
                max_bytes
            );
            None
        }
        Ok(_) => {
            let raw = std::fs::read_to_string(path).ok()?;
            serde_json::from_str(&raw).ok()
        }
        Err(_) => None,
    }
}

/// Spawn a command and reap it in a detached thread to prevent zombie accumulation.
pub fn spawn_and_reap(cmd: &mut std::process::Command) -> std::io::Result<u32> {
    let mut child = cmd.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

/// Like `spawn_and_reap`, but sets `PR_SET_PDEATHSIG(SIGKILL)` so the
/// child cannot outlive the parent even on crash.
pub fn spawn_and_reap_pdeathsig(cmd: &mut std::process::Command) -> std::io::Result<u32> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        #[allow(unsafe_code)]
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

/// Resolve the current executable path, stripping the " (deleted)" suffix
/// that `/proc/PID/exe` appends after `cargo build` replaces a running binary.
pub fn resolve_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            if let Some(stripped) = s.strip_suffix(" (deleted)") {
                PathBuf::from(stripped)
            } else {
                p
            }
        })
        .unwrap_or_else(|| PathBuf::from("moonbeauty"))
}

/// Write a restrictive ImageMagick policy.xml to a private per-session dir
/// and return that dir so child processes can inherit `MAGICK_CONFIGURE_PATH`.
pub fn ensure_imagemagick_policy() -> Option<PathBuf> {
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")?;
    let dir = PathBuf::from(xdg).join("mbs-imagemagick");
    let _ = ensure_private_dir(&dir);
    let policy = dir.join("policy.xml");
    const CONTENT: &str = r#"<policymap>
  <!-- Block delegate + filter execution (ghostscript, rsvg, etc.) -->
  <policy domain="delegate" rights="none" pattern="*" />
  <policy domain="filter" rights="none" pattern="*" />

  <!-- Deny coders with RCE history or file-fetching semantics -->
  <policy domain="coder" rights="none" pattern="MVG" />
  <policy domain="coder" rights="none" pattern="MSL" />
  <policy domain="coder" rights="none" pattern="MSL2" />
  <policy domain="coder" rights="none" pattern="MSVG" />
  <policy domain="coder" rights="none" pattern="URL" />
  <policy domain="coder" rights="none" pattern="HTTPS" />
  <policy domain="coder" rights="none" pattern="HTTP" />
  <policy domain="coder" rights="none" pattern="FTP" />
  <policy domain="coder" rights="none" pattern="EPHEMERAL" />
  <policy domain="coder" rights="none" pattern="TMP" />
  <policy domain="coder" rights="none" pattern="PLT" />
  <policy domain="coder" rights="none" pattern="PS" />
  <policy domain="coder" rights="none" pattern="EPS" />
  <policy domain="coder" rights="none" pattern="PDF" />
  <policy domain="coder" rights="none" pattern="XPS" />

  <!-- Block @file inclusion (info disclosure) -->
  <policy domain="path" rights="none" pattern="@*" />
</policymap>
"#;
    // Only rewrite when content differs — avoid churn on every startup.
    let stale = match std::fs::read_to_string(&policy) {
        Ok(existing) => existing != CONTENT,
        Err(_) => true,
    };
    if stale {
        if let Err(e) = private_write_file(&policy, CONTENT.as_bytes()) {
            eprintln!("[imagemagick] policy write failed: {e}");
            return None;
        }
    }
    Some(dir)
}

#[cfg(test)]
mod safe_app_id_tests {
    use super::is_safe_app_id;

    #[test]
    fn safe_app_ids_accepted() {
        for s in &[
            "firefox",
            "org.mozilla.firefox",
            "org.gnome.Nautilus",
            "steam_app_1234",
            "code-url-handler",
            "a",
            "A.B_C-D",
        ] {
            assert!(is_safe_app_id(s), "expected safe: {:?}", s);
        }
    }

    #[test]
    fn injection_attempts_rejected() {
        for s in &[
            "",
            r#"x"] exec sh -c evil #"#,
            r#"x"] focus"#,
            "../etc/passwd",
            "a b",
            "a;b",
            "a&b",
            "a|b",
            "a$(id)",
            "a`id`",
            "\u{1F600}",
            "..",
            "a..b",
        ] {
            assert!(!is_safe_app_id(s), "expected unsafe: {:?}", s);
        }
        // Control characters — newline is a sway-command separator,
        // null truncates C strings.
        assert!(!is_safe_app_id("a\nb"), "newline must be rejected");
        assert!(!is_safe_app_id("a\x00b"), "null byte must be rejected");
    }
}

#[cfg(test)]
mod sanitize_folder_tests {
    use super::sanitize_folder_name;

    #[test]
    fn benign_names_unchanged() {
        for s in &[
            "Steam",
            "GitHub",
            "Acme Corp.",
            "Foo (local)",
            "a-b_c d",
            "version 1.2",
            "[Gmail]",
        ] {
            assert_eq!(sanitize_folder_name(s), *s, "{:?} changed", s);
        }
    }

    #[test]
    fn path_traversal_rejected() {
        for s in &[
            "../../.config/autostart/evil",
            "..",
            ".",
            "/etc/passwd",
            "\\windows\\system32",
            "a/b",
            "a\\b",
            "a..b",
        ] {
            let out = sanitize_folder_name(s);
            assert!(!out.contains('/'), "{:?} → {:?}", s, out);
            assert!(!out.contains('\\'), "{:?} → {:?}", s, out);
            assert!(!out.contains(".."), "{:?} → {:?}", s, out);
            assert!(!out.starts_with('.'), "{:?} → {:?}", s, out);
            assert!(!out.is_empty(), "{:?} → empty", s);
        }
    }

    #[test]
    fn empty_falls_back() {
        assert_eq!(sanitize_folder_name(""), "unnamed");
        assert_eq!(sanitize_folder_name("   "), "unnamed");
        assert_eq!(sanitize_folder_name("..."), "unnamed");
    }

    #[test]
    fn length_capped() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_folder_name(&long).len(), 64);
    }
}

#[cfg(test)]
mod runtime_dir_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn mode(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn unique(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mbs-rt-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn ensure_private_dir_creates_0700() {
        let d = unique("create");
        let _ = std::fs::remove_dir_all(&d);
        ensure_private_dir(&d).unwrap();
        assert_eq!(mode(&d), 0o700);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ensure_private_dir_tightens_existing() {
        let d = unique("tighten");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut p = std::fs::metadata(&d).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&d, p).unwrap();
        assert_eq!(mode(&d), 0o755, "precondition");
        ensure_private_dir(&d).unwrap();
        assert_eq!(mode(&d), 0o700, "post-fix");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ensure_private_dir_idempotent() {
        let d = unique("idem");
        let _ = std::fs::remove_dir_all(&d);
        ensure_private_dir(&d).unwrap();
        ensure_private_dir(&d).unwrap();
        ensure_private_dir(&d).unwrap();
        assert_eq!(mode(&d), 0o700);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn mbs_subdir_lands_under_xdg() {
        let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") else {
            eprintln!("SKIP: XDG_RUNTIME_DIR not set");
            return;
        };
        let xdg = std::path::PathBuf::from(xdg);
        let result = mbs_subdir("mbs-test-subdir");
        assert!(result.starts_with(&xdg), "{:?} not under {:?}", result, xdg);
        assert_eq!(mode(&result), 0o700, "subdir mode");
        let _ = std::fs::remove_dir_all(&result);
    }
}

#[cfg(test)]
mod tighten_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn mode(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn unique(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("mbs-tighten-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn sweeps_dirs_and_files_and_ignores_foreign() {
        let root = unique("sweep");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let sub = root.join("mbs-tray");
        std::fs::create_dir_all(&sub).unwrap();
        let f1 = sub.join("icon.png");
        std::fs::write(&f1, b"png").unwrap();

        let f2 = root.join("moonbeauty-active-count");
        std::fs::write(&f2, b"3").unwrap();
        let f3 = root.join("mbs-email-notified.txt");
        std::fs::write(&f3, b"x").unwrap();

        let foreign_dir = root.join("pulse");
        std::fs::create_dir_all(&foreign_dir).unwrap();
        let foreign_file = root.join("other-user-state");
        std::fs::write(&foreign_file, b"leave me").unwrap();

        for d in [&sub, &foreign_dir] {
            let mut p = std::fs::metadata(d).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(d, p).unwrap();
        }
        for f in [&f1, &f2, &f3, &foreign_file] {
            let mut p = std::fs::metadata(f).unwrap().permissions();
            p.set_mode(0o644);
            std::fs::set_permissions(f, p).unwrap();
        }

        tighten_existing_runtime_state_at(&root);

        assert_eq!(mode(&sub), 0o700);
        assert_eq!(mode(&f1), 0o600);
        assert_eq!(mode(&f2), 0o600);
        assert_eq!(mode(&f3), 0o600);
        assert_eq!(mode(&foreign_dir), 0o755, "foreign dir must not be touched");
        assert_eq!(
            mode(&foreign_file),
            0o644,
            "foreign file must not be touched"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn idempotent_on_already_tight() {
        let root = unique("idem");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let sub = root.join("mbs-clip");
        std::fs::create_dir_all(&sub).unwrap();
        let f = sub.join("thumb.png");
        std::fs::write(&f, b"png").unwrap();

        tighten_existing_runtime_state_at(&root);
        tighten_existing_runtime_state_at(&root);

        assert_eq!(mode(&sub), 0o700);
        assert_eq!(mode(&f), 0o600);

        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod unique_tmp_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn mode(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mbs-uniq-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn token_is_16_hex_and_unique() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "successive tokens must differ");
    }

    #[test]
    fn private_create_new_succeeds_on_fresh_path() {
        let d = scratch("fresh");
        let p = d.join("file");
        let mut f = private_create_new(&p).unwrap();
        use std::io::Write as _;
        f.write_all(b"hello").unwrap();
        drop(f);
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        assert_eq!(mode(&p), 0o600);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn private_create_new_rejects_existing_file() {
        let d = scratch("exists");
        let p = d.join("file");
        std::fs::write(&p, b"original").unwrap();
        assert!(
            private_create_new(&p).is_err(),
            "create_new must reject existing regular file"
        );
        assert_eq!(std::fs::read(&p).unwrap(), b"original", "content unchanged");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn private_create_new_rejects_symlink_squat() {
        let d = scratch("symlink");
        let target = d.join("victim");
        std::fs::write(&target, b"untouched").unwrap();
        let squatted = d.join("squatted");
        std::os::unix::fs::symlink(&target, &squatted).unwrap();
        assert!(
            private_create_new(&squatted).is_err(),
            "create_new must reject pre-existing symlink"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"untouched",
            "symlink target must not be truncated or written"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn private_write_new_writes_and_sets_mode() {
        let d = scratch("write");
        let p = d.join("file");
        private_write_new(&p, b"content").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"content");
        assert_eq!(mode(&p), 0o600);
        assert!(
            private_write_new(&p, b"other").is_err(),
            "second call must fail"
        );
        assert_eq!(std::fs::read(&p).unwrap(), b"content");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn mbs_unique_tmp_yields_unique_paths() {
        if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
            eprintln!("SKIP: XDG_RUNTIME_DIR not set");
            return;
        }
        let a = mbs_unique_tmp("mbs-test", "png");
        let b = mbs_unique_tmp("mbs-test", "png");
        assert_ne!(a, b);
        let xdg = std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
        assert!(a.starts_with(&xdg), "{:?} not under {:?}", a, xdg);
        assert!(
            a.starts_with(xdg.join("mbs-tmp")),
            "{:?} not under mbs-tmp",
            a
        );
    }
}

/// Chmod the shell's user-content files and their parent directories to
/// safe permissions (0600 files, 0700 dirs). Idempotent.
pub fn tighten_user_state_files() {
    use std::os::unix::fs::PermissionsExt as _;
    fn chmod(p: &std::path::Path, mode: u32) {
        let Ok(meta) = std::fs::metadata(p) else {
            return;
        };
        let mut perms = meta.permissions();
        if perms.mode() & 0o777 != mode {
            perms.set_mode(mode);
            if std::fs::set_permissions(p, perms).is_ok() {
                eprintln!("[perms] tightened {} -> {:o}", p.display(), mode);
            }
        }
    }
    if let Some(config) = dirs::config_dir() {
        let base = config.join("moonbeauty");
        chmod(&base, 0o700);
        chmod(&base.join("settings.json"), 0o600);
    }
    if let Some(data) = dirs::data_dir() {
        let base = data.join("moonbeauty");
        chmod(&base, 0o700);
        for f in [
            "notes.json",
            "todos.json",
            "friend_profiles.json",
            "email_cache/local_sort.json",
            "clipboard/index.json",
        ] {
            chmod(&base.join(f), 0o600);
        }
        chmod(&base.join("clipboard"), 0o700);
        chmod(&data.join("quickshell/todos.json"), 0o600);
    }
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        let base = std::path::PathBuf::from(rt);
        chmod(&base.join("moon_reminders.json"), 0o600);
        chmod(&base.join("quickshell_reminders.json"), 0o600);
    }
}

/// Unified icon resolution across launcher, tray, and notifications.
/// Enforces the `safe_icon_candidate` symlink check on every path.
pub fn resolve_icon_path(icon_name: &str) -> Option<std::path::PathBuf> {
    if icon_name.is_empty() {
        return None;
    }

    // 1. Absolute path handling (with whitelist)
    if icon_name.starts_with('/') {
        let known = [
            "/usr/share/icons/",
            "/usr/share/pixmaps/",
            "/usr/local/share/icons/",
            "/usr/local/share/pixmaps/",
            "/run/current-system/sw/share/icons/",
            "/run/current-system/sw/share/pixmaps/",
            "/var/lib/flatpak/exports/share/icons/",
            "/var/lib/flatpak/exports/share/pixmaps/",
            "/var/lib/snapd/desktop/icons/",
        ];
        let mut ok = known.iter().any(|p| icon_name.starts_with(p));
        if !ok {
            if let Some(home) = dirs::home_dir() {
                let h = home.to_string_lossy();
                ok = icon_name.starts_with(&format!("{}/.local/share/icons/", h))
                    || icon_name.starts_with(&format!("{}/.nix-profile/share/icons/", h))
                    || icon_name.starts_with(&format!("{}/.local/share/flatpak/exports/share/icons/", h));
            }
        }
        if !ok {
            return None;
        }
        return if std::path::Path::new(icon_name).exists() {
            Some(std::path::PathBuf::from(icon_name))
        } else {
            None
        };
    }

    // 2. Relative name validation
    if icon_name.contains("..") || icon_name.contains('/') || icon_name.contains('\\') {
        return None;
    }
    if !icon_name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return None;
    }

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());

    let theme_roots = [
        std::path::PathBuf::from("/etc/profiles/per-user").join(&user).join("share/icons"),
        std::path::PathBuf::from("/run/current-system/sw/share/icons"),
        std::path::PathBuf::from("/usr/share/icons"),
        home.join(".local/share/icons"),
        home.join(".nix-profile/share/icons"),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/icons"),
        home.join(".local/share/flatpak/exports/share/icons"),
    ];
    let themes = ["hicolor", "Adwaita", "breeze", "Papirus", "Papirus-Dark", "Yaru", "Numix"];
    let sizes = ["256x256", "128x128", "512x512", "64x64", "48x48", "32x32", "scalable", "symbolic"];
    let exts = ["png", "svg", "xpm"];

    for root in &theme_roots {
        for theme in &themes {
            for size in &sizes {
                for ext in &exts {
                    let p = root.join(theme).join(size).join("apps").join(format!("{}.{}", icon_name, ext));
                    if p.exists() {
                        if let Some(canon) = safe_icon_candidate(&p, root) {
                            return Some(canon);
                        }
                    }
                }
            }
        }
    }

    let flat_dirs = vec![
        std::path::PathBuf::from("/run/current-system/sw/share/pixmaps"),
        std::path::PathBuf::from("/usr/share/pixmaps"),
        std::path::PathBuf::from("/etc/profiles/per-user").join(&user).join("share/pixmaps"),
        home.join(".nix-profile/share/pixmaps"),
    ];
    for dir in &flat_dirs {
        for ext in &exts {
            let p = dir.join(format!("{}.{}", icon_name, ext));
            if p.exists() {
                if let Some(canon) = safe_icon_candidate(&p, dir) {
                    return Some(canon);
                }
            }
        }
    }
    None
}
