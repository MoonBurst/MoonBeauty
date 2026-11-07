# Unify freedesktop icon resolvers (Security + Tech Debt)

**Effort:** 30–45m
**Subsystem:** icon resolution
**Depends on:** —
**Good first issue:** no

## Current state

Three separate implementations of the freedesktop icon search:

1. `src/overlays/launcher.rs` :: `resolve_icon` (has `safe_icon_candidate` symlink check)
2. `src/bar/tray/watcher.rs` :: `resolve_icon` (has `safe_icon_candidate` symlink check + re-encoding/hashing)
3. `src/overlays/notifications/ui.rs` :: `resolve_icon_name` (**missing** the symlink check)

## Security issue

The notifications implementation is vulnerable to the symlink-escape attack that `safe_icon_candidate` was introduced to fix (Finding 27). A malicious desktop file could point an icon to `/etc/passwd`.

## Desired state

Extract a shared `libraries::process::find_icon_path(name: &str) -> Option<PathBuf>` that performs the directory loop and the `safe_icon_candidate` root check.

- `launcher.rs` and `notifications/ui.rs` use it directly.
- `tray/watcher.rs` uses it, then applies its own re-encode/hash logic on the returned path.
