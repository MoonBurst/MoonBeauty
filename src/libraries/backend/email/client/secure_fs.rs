//! Secure filesystem helpers for the email cache.
//!
//! Every write to the email cache, drafts file, and folders list must
//! land at mode 0600 (owner RW, no group/other access).  Prior code used
//! std::fs::write / tokio::fs::write / bare OpenOptions, which inherit
//! umask (default 022 -> 0644).  On a multi-user host that made every
//! cached email body, attachment, and draft readable by other local
//! users.  Sink-level fix: all writes go through these helpers.
//!
//! Note: OpenOptions.mode() only applies to NEW files.  If the file
//! already exists at 0644, mode() is a no-op -- so we also chmod
//! explicitly after opening.  That is what `chmod_private` exists for.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

pub(crate) fn chmod_private(path: &std::path::Path, is_dir: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = if is_dir { 0o700 } else { 0o600 };
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)
}

/// Overwrite `path` with `bytes` at mode 0600, creating parent dirs at
/// 0700. Safe to call on an existing file with looser permissions: the
/// explicit chmod after open handles that case.
pub(crate) fn secure_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        let _ = chmod_private(parent, true);
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    let _ = chmod_private(path, false);
    f.write_all(bytes)
}

pub(crate) async fn secure_write_async(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt as _;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
        let _ = chmod_private(parent, true);
    }
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .await?;
    let _ = chmod_private(path, false);
    f.write_all(bytes).await
}

/// Append-only variant for logs (`drafts.txt`). Same permission model
/// as secure_write.
pub(crate) fn secure_append(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        let _ = chmod_private(parent, true);
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    let _ = chmod_private(path, false);
    f.write_all(bytes)
}

/// Recursively chmod every directory under `root` to 0700 and every
/// file to 0600. Idempotent; called once per process by the real
/// entry point (`ensure_secure_cache_tree`), and directly by tests.
pub(crate) fn ensure_secure_cache_tree_at(root: &std::path::Path) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let _ = chmod_private(root, true);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                let _ = chmod_private(&p, true);
                stack.push(p);
            } else if ft.is_file() {
                let _ = chmod_private(&p, false);
            }
        }
    }
    Ok(())
}

/// One-shot migration on first call per process. Fixes any pre-existing
/// cache entries that were written at 0644 by earlier builds. Cheap
/// (guarded by OnceLock); subsequent calls are no-ops.
pub(crate) fn ensure_secure_cache_tree() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let root = base.join("moon-beauty-shell/email_cache");
        if !root.exists() {
            return;
        }
        match ensure_secure_cache_tree_at(&root) {
            Ok(()) => eprintln!("[email] secure-cache migration: applied to {:?}", root),
            Err(e) => eprintln!("[email] secure-cache migration failed: {e}"),
        }
    });
}

#[cfg(test)]
mod secure_fs_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn mode_of(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mbs-sec-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn secure_write_creates_0600() {
        let dir = unique_dir("write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sub").join("file.txt");
        secure_write(&path, b"secret").unwrap();
        assert_eq!(mode_of(&path), 0o600, "file mode");
        assert_eq!(mode_of(&path.parent().unwrap()), 0o700, "parent mode");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secure_write_downgrades_existing_loose_file() {
        let dir = unique_dir("downgrade");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("file.txt");
        std::fs::write(&path, b"x").unwrap();
        let mut p = std::fs::metadata(&path).unwrap().permissions();
        p.set_mode(0o644);
        std::fs::set_permissions(&path, p).unwrap();
        assert_eq!(mode_of(&path), 0o644, "precondition");
        secure_write(&path, b"y").unwrap();
        assert_eq!(mode_of(&path), 0o600, "post-fix");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secure_append_preserves_content_and_tightens_mode() {
        let dir = unique_dir("append");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("drafts.txt");
        secure_append(&path, b"one\n").unwrap();
        secure_append(&path, b"two\n").unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body, "one\ntwo\n");
        assert_eq!(mode_of(&path), 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_tree_chmods_recursively() {
        let dir = unique_dir("tree");
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("INBOX");
        std::fs::create_dir_all(&sub).unwrap();
        let f1 = dir.join("index.json");
        let f2 = sub.join("mail.eml");
        std::fs::write(&f1, b"[]").unwrap();
        std::fs::write(&f2, b"body").unwrap();
        for (p, m) in [(&dir, 0o755_u32), (&sub, 0o755), (&f1, 0o644), (&f2, 0o644)] {
            let mut pm = std::fs::metadata(p).unwrap().permissions();
            pm.set_mode(m);
            std::fs::set_permissions(p, pm).unwrap();
        }
        ensure_secure_cache_tree_at(&dir).unwrap();
        assert_eq!(mode_of(&dir), 0o700, "root");
        assert_eq!(mode_of(&sub), 0o700, "subdir");
        assert_eq!(mode_of(&f1), 0o600, "file1");
        assert_eq!(mode_of(&f2), 0o600, "file2");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
