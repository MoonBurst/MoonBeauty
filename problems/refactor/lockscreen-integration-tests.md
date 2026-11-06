# Lockscreen coordinator integration tests

**Effort:** 2–3h
**Subsystem:** lockscreen
**Depends on:** —
**Good first issue:** no

## Current state

`--test-lock-auth` is a manual CLI diagnostic. No automated coverage
for the coordinator's testable pure functions or shutdown paths.

The bar-side watchdog tests that were originally in this tracker
were removed along with the watchdog itself when the coordinator
moved to a systemd unit with `Restart=on-failure`. The unit is now
the watchdog; testing systemd itself is out of scope for cargo tests.

## Desired state

Three unit tests that do NOT require root or an active Wayland
session:

1. `sweep_stale_lockscreen_sockets()`
   - Create temp dir, drop three files:
     - `moon-beauty-lock-{current_pid}.sock` (own socket, must NOT
       be swept)
     - `moon-beauty-lock-99999.sock` (PID 99999 is dead, must be
       swept)
     - `unrelated.sock` (doesn't match pattern, must be ignored)
   - Call sweep, assert only the 99999 file removed.

2. `shutdown_lockscreen_children()`
   - Spawn 2 children via `Command::new("sleep").arg("60").spawn()`,
     insert into `Arc<Mutex<HashMap<String, Child>>>`
   - Create a fake socket file at the expected path
   - Call `shutdown_lockscreen_children`
   - Poll `try_wait()` for up to 500ms on each child; assert both
     exited and socket removed.

3. `sudo_is_nopasswd()`
   - Skip test if `find_bin("sudo")` returns None (documented).
   - Otherwise assert the function returns bool without panicking.
   - Cannot test the NOPASSWD-specific branch without manipulating
     sudoers, which requires root. Document this in a comment. The
     function's correctness for a known config is verified by
     `--test-lock-auth`.

## Out of scope (manual verification only)

- Full `systemctl --user start/stop` cycle — requires a live systemd
  user session.
- Full auth flow against real PAM — `--test-lock-auth` covers this.
- Multi-monitor rendering — requires a real Wayland session.
- TTY-switch recovery — requires real `/dev/ttyN`; see
  `lockscreen-coordinator-split.md` manual verification for the
  current procedure.

## Caution

Tests 1 and 2 create files and processes. Both must clean up in a
`Drop` guard or at end of test, and must use `tempfile::tempdir()`
(or `$XDG_RUNTIME_DIR` via `mbs_subdir`) — NOT
`std::env::temp_dir()` which is world-writable. Follow the existing
test hygiene in `libraries::process::runtime_dir_tests`.

Test 2 spawns processes — poll with a short sleep loop (up to 500
ms) before asserting, since `try_wait()` may not return immediately
after SIGKILL.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` — 76 passing + 3 new
- New tests run in <1s each
