# Lockscreen auth integration tests

**Effort:** 3–4h
**Subsystem:** lockscreen
**Depends on:** —
**Good first issue:** no

## Current state

`--test-lock-auth` is a manual CLI diagnostic. No automated coverage
for:
- `authenticate()` success / failure paths
- Watchdog respawn (crashed child → respawn; clean exit → don't)
- Multi-monitor coordination

## Desired state

At least two tests:

1. `authenticate()` against a mock PAM (or a test PAM service that
   always accepts / rejects). Verify both paths return the expected
   boolean.
2. Watchdog logic as unit test: given a `standalone_child` that reports
   `Some(exit_zero)`, verify the coordinator does NOT respawn. Given
   `Some(exit_nonzero)`, verify it DOES. Use
   `std::process::Command::new("sh").args(["-c", "exit 0"])` as a fake
   child — no PAM required.

## Caution

Test (2) spawns processes. Poll with a short sleep loop (up to 500 ms)
before asserting, since `try_wait()` may not return immediately.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` — 67 passing + N new
- New tests should not require root or an active Wayland session
