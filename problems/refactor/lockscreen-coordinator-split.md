# Split `overlays/lockscreen/coordinator.rs` into a folder

**Effort:** 2–3h
**Subsystem:** lockscreen
**Depends on:** lockscreen-integration-tests.md
**Good first issue:** no

## Current state

`src/overlays/lockscreen/coordinator.rs` is ~848 lines. Contains the
main event loop, IPC broker, hotplug watcher, watchdog, signal
handlers, four auth strategies, subprocess management, and a CLI
diagnostic.

## Desired state

    src/overlays/lockscreen/coordinator/
    ├── mod.rs         LockCoordinator struct + LockIpc + spawn()
    ├── auth.rs        authenticate(), find_bin(), resolve_user()
    ├── ipc.rs         peer management, broadcast, write_msg
    ├── hotplug.rs     swaymsg output subscription
    ├── watchdog.rs    try_wait polling loop
    └── diag.rs        run_auth_test()

## Caution

Do this AFTER `lockscreen-integration-tests.md` lands — the tests
prove the split didn't change behavior of the auth loop or the
watchdog. Without tests, it's a leap of faith on security-critical code.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing + lockscreen tests)
- Manual:
  1. `moonbeauty --toggle lockscreen`, wrong password + Enter →
     "Incorrect password"; correct password + Enter → unlocks
  2. Lock again, `kill -9` a `--lockscreen-instance` child → new child
     appears within 1s, screen stays locked
  3. Unlock normally, wait 5s → watchdog does NOT respawn
