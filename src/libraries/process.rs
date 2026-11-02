//! Process utilities.

use std::path::PathBuf;

/// Resolve the current executable path, stripping the " (deleted)" suffix
/// that `/proc/PID/exe` appends after `cargo build` replaces a running binary.
/// Whitelist acceptable characters for an app_id / desktop-entry ID.
///
/// Freedesktop app_id values are reverse-DNS names like
/// `org.mozilla.firefox`, plus legacy names like `firefox` or
/// `steam_app_1234`. All of these consist only of `[A-Za-z0-9._-]`.
/// Anything outside that set is a red flag:
///
/// * swaymsg command mode uses `[app_id="..."]` syntax. A `"` or `]`
///   in the id breaks out of the criteria brackets and lets an
///   attacker append arbitrary sway commands (`exec sh -c evil`).
/// * gtk-launch treats its argument as a desktop-entry ID under the
///   standard XDG applications dirs. A path separator or `--` prefix
///   is at best an unrelated file lookup, at worst a flag injection
///   for downstream tools.
///
/// Additionally reject `..` so an attacker can't point at a file
/// outside the desktop-entries roots if any downstream tool ever
/// canonicalizes the path.
/// Sanitize a folder name derived from an untrusted source (email
/// `From:` display name, IMAP folder name) before use as a path
/// component.
///
/// Every character not in `[A-Za-z0-9 _\-().\[\]]` is replaced with
/// `_`. Leading dots are stripped (kills hidden files, `.` and `..`),
/// and residual `..` substrings are collapsed to `_`. Result is capped
/// at 64 bytes. Empty input becomes `"unnamed"`.
///
/// Known-good names like `"Steam"`, `"Acme Corp."`, or `"[Gmail]"`
/// pass through unchanged. Non-ASCII characters become `_` (senders
/// are almost always ASCII; acceptable tradeoff).
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
/// exists with looser permissions (0755, 0750, …), tightens them. Does
/// not fail if the directory is already 0700.
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

/// Return `$XDG_RUNTIME_DIR`.
///
/// **Panics if `XDG_RUNTIME_DIR` is unset.** No silent fallback to
/// `/tmp`: that directory is world-listable and readable, so any file
/// placed there is visible to other local users, and predictable names
/// enable symlink-squatting attacks. `XDG_RUNTIME_DIR` is 0700 per the
/// XDG spec, guaranteed on any conformant Wayland session. If it is
/// missing, the session is broken and the shell cannot safely write
/// per-session state.
///
/// Creates the directory at 0700 if somehow absent (conformant sessions
/// always pre-create it, but defense-in-depth costs nothing). Tightens
/// any pre-existing looser mode.
///
/// This matches the behavior already implemented in
/// `ipc::protocol::get_socket_path` and
/// `overlays::lockscreen::coordinator::spawn` — those modules refused
/// to fall back for exactly the same reason.
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
/// Use this for shell-owned runtime files under `$XDG_RUNTIME_DIR`.
/// Unlike `std::fs::write`, which inherits umask (022 → 0644), this
/// guarantees 0600 so contents stay private even if the parent
/// directory mode ever drifts.
///
/// Handles pre-existing files at 0644: `OpenOptions::mode()` only
/// applies at file creation, so an explicit `chmod_mode` after open is
/// required to fix stale permissions.
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

/// Read N bytes from /dev/urandom. Falls back to a time+pid mix if
/// /dev/urandom is unreadable (should never happen on Linux; portable
/// code shouldn't panic on missing special files, but the fallback is
/// only for collision resistance, not crypto).
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

/// 16-hex-char random token suitable for filename suffixes.
///
/// 8 bytes from `/dev/urandom` → 16 hex chars. Collision probability is
/// negligible; the point is unpredictability, so a same-user sibling
/// process cannot symlink-squat a path generated from this token.
pub fn random_token() -> String {
    let bytes = urandom_bytes::<8>();
    let mut out = String::with_capacity(16);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

/// `$XDG_RUNTIME_DIR/mbs-tmp`, created at 0700.
///
/// All transient shell files (screenshots, tray-menu handoffs, annotator
/// intermediates) live here rather than directly in `/tmp`. Isolation
/// comes from two layers: this directory is 0700, and every file within
/// it gets a random suffix (see `mbs_unique_tmp`).
pub fn mbs_tmp_dir() -> std::path::PathBuf {
    mbs_subdir("mbs-tmp")
}

/// Generate a unique path under `mbs_tmp_dir()`.
///
/// Format: `<mbs-tmp>/<prefix>-<16-hex>.<ext>`.
///
/// Does **not** create the file — callers must use `private_create_new`
/// (or `private_write_new`) so the open is `O_CREAT | O_EXCL`, which
/// atomically rejects symlinks and pre-existing files. Do not use
/// `File::create()` on the returned path: that follows symlinks and
/// would defeat the entire purpose.
pub fn mbs_unique_tmp(prefix: &str, ext: &str) -> std::path::PathBuf {
    mbs_tmp_dir().join(format!("{}-{}.{}", prefix, random_token(), ext))
}

/// Open `path` for writing with `O_CREAT | O_EXCL` at mode 0600.
///
/// Fails with `AlreadyExists` if anything (regular file, symlink,
/// directory) currently exists at `path`. This is the safe primitive for
/// writes to attacker-predictable names: the syscall cannot be tricked
/// into following a symlink or truncating a pre-placed file.
pub fn private_create_new(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// `private_create_new` + write all bytes. Convenience wrapper for the
/// common "one-shot write to a unique path" case.
pub fn private_write_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut f = private_create_new(path)?;
    f.write_all(bytes)
}

pub fn mbs_runtime_dir() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            panic!(
                "XDG_RUNTIME_DIR is not set — refusing to fall back to /tmp \
                 for per-session state (see findings 04, 11, 12)"
            )
        });
    let _ = ensure_private_dir(&dir);
    dir
}

/// `mbs_runtime_dir().join(name)`, created at 0700 if missing.
///
/// Use this instead of `std::env::temp_dir()` or
/// `std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp")` for
/// any per-session file the shell writes. Common callers:
/// `mbs_subdir("mbs-tray")`, `mbs_subdir("mbs-email-images")`, etc.
pub fn mbs_subdir(name: &str) -> std::path::PathBuf {
    let path = mbs_runtime_dir().join(name);
    let _ = ensure_private_dir(&path);
    path
}

/// Walk `$XDG_RUNTIME_DIR` and tighten every `mbs-*` subdirectory
/// (and its immediate children) to 0700/0600.
///
/// Does **not** create new dirs — only fixes pre-existing ones left
/// over from builds that predated the `mbs_subdir` migration. Cheap
/// (one readdir); call once at process startup.
///
/// This closes the gap left by the lazy `mbs_subdir` path: a
/// directory from a pre-fix build stays at 0755 forever if its code
/// path (e.g. `read_message` with images, `grim_capture`) never runs.
/// Startup sweep fixes them unconditionally.
/// Walk `$XDG_RUNTIME_DIR` and tighten every shell-owned file and
/// directory to 0700 (dirs) / 0600 (files).
///
/// Shell-owned = name starts with `mbs-` or `moonbeauty`.
/// Covers:
///   - mbs-tray/, mbs-clip/, mbs-email-images/, mbs-annotator/ (dirs)
///     and their immediate children
///   - moonbeauty-{dnd,held-notifs.json,active-count,notif-cmd}
///   - mbs-email-notified.txt
///   - moon-beauty-lock-*.sock
///
/// Why this exists (findings 11, 12): a subdir created by a pre-fix
/// build stays at 0755 forever if its code path never runs again (e.g.
/// mbs-email-images only touched when opening an email with images).
/// Startup sweep fixes them unconditionally.
///
/// Called from `MoonBeautyShellApp::new_app` in every process the
/// shell spawns (bar, notif-surface, each standalone overlay).
/// Prune standalone log files from `mbs_tmp_dir()`.
///
/// Each `spawn_standalone` call creates `mbs-standalone-<token>.log`.
/// Without pruning, months of use accumulate hundreds of files on tmpfs.
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
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs(3600);
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
/// declared root. Blocks symlink-to-sensitive-file — same class as the
/// tray/watcher.rs version added for finding 27. Used by launcher.rs
/// and any future icon resolver that wants the same guarantee.
pub fn safe_icon_candidate(candidate: &std::path::Path, root: &std::path::Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(candidate).ok()?;
    let canon_root = std::fs::canonicalize(root).ok()?;
    // Finding 64: also accept canonical targets under /nix/store/.
    // NixOS icons are symlinks from /run/current-system/sw/share/icons
    // (or /usr/share/icons) into /nix/store/.../share/icons. The store
    // is read-only and content-addressed — no attacker can drop files
    // there. A hostile symlink to /etc/passwd or ~/.ssh/id_rsa still
    // canonicalizes outside both roots and gets rejected.
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

/// Read a JSON file with a size cap. Returns None on missing / oversized
/// / malformed. Used by every user-state loader (settings, todos,
/// reminders, held notifs, dictionaries, caches) to prevent a hostile
/// same-user process from OOMing the shell at next launch with a
/// multi-GB file.
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

/// Spawn a command and reap it in a detached thread. Prevents zombie
/// accumulation when the Child handle is dropped without wait (finding
/// 48). Suitable for fire-and-forget spawns like notify-send, xdg-open,
/// wl-copy.
pub fn spawn_and_reap(cmd: &mut std::process::Command) -> std::io::Result<u32> {
    let mut child = cmd.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

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

/// Finding 07 defense-in-depth: write a restrictive ImageMagick
/// policy.xml to a private per-session dir and return that dir. The
/// caller sets `MAGICK_CONFIGURE_PATH` to it so all ImageMagick child
/// processes inherit the restriction.
///
/// Denylist: coders with RCE history (MVG, MSL, MSL2, MSVG, PS/EPS/PDF/
/// XPS via ghostscript delegate, URL-fetching pseudo-coders, PLT), plus
/// a blanket delegate+filter block and `@file` inclusion. Whitelisted
/// coders (PNG, MPR, XC, etc.) are untouched, so annotator/watermark
/// continue to work.
///
/// Idempotent. Returns `None` only if `$XDG_RUNTIME_DIR` is unset — the
/// caller skips setting the env var in degraded sessions rather than
/// crashing the whole shell.
pub fn ensure_imagemagick_policy() -> Option<PathBuf> {
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")?;
    let dir = PathBuf::from(xdg).join("mbs-imagemagick");
    let _ = ensure_private_dir(&dir);
    let policy = dir.join("policy.xml");
    const CONTENT: &str = r#"<policymap>
  <!-- Finding 07: block delegate + filter execution (ghostscript, rsvg, etc.) -->
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

    /// Only meaningful when running under a real Wayland session (test
    /// runner inherits XDG_RUNTIME_DIR). Under `nix develop` it may be
    /// set; in CI it may not. Skip if absent — the two pure tests above
    /// already prove the helper, and the runtime sweep in Command B is
    /// verified separately at the shell level.
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
