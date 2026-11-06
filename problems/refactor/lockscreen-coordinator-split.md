# Split `overlays/lockscreen/coordinator.rs` into a folder

**Effort:** 2–3h
**Subsystem:** lockscreen
**Depends on:** lockscreen-integration-tests.md
**Good first issue:** no

## Current state

`src/overlays/lockscreen/coordinator.rs` is ~1009 lines. Contains the
main event loop, IPC broker, hotplug watcher, TTY-return watcher,
signal handlers (SIGTERM/SIGINT/USR1/USR2/HUP), stale-socket sweep,
two auth strategies (pamtester, guarded sudo), subprocess management,
and a CLI diagnostic.

The bar-side watchdog (`check_lockscreen_alive`) was removed when the
coordinator moved to a systemd unit with `Restart=on-failure` — the
unit is now the watchdog.

## Desired state

    src/overlays/lockscreen/coordinator/
    ├── mod.rs         LockCoordinator struct + LockIpc + spawn()
    ├── auth.rs        authenticate(), find_bin(), resolve_user(),
    │                  sudo_is_nopasswd()
    ├── ipc.rs         peer management, broadcast, write_msg
    ├── hotplug.rs     swaymsg output subscription
    ├── tty_watch.rs   /sys/class/tty/tty0/active polling + respawn
    ├── signals.rs     SIGTERM/SIGINT/USR1/USR2/HUP handlers, shutdown
    ├── sweep.rs       sweep_stale_lockscreen_sockets()
    └── diag.rs        run_auth_test()

## Caution

Do this AFTER `lockscreen-integration-tests.md` lands — the tests
prove the split didn't change behavior of the auth loop, socket
sweep, or SIGTERM shutdown. Without tests, it's a leap of faith on
security-critical code.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (76 passing + new lockscreen tests)
- Manual:
  1. `moonbeauty --toggle lockscreen`, wrong password + Enter →
     "Incorrect password"; correct password + Enter → unlocks
  2. Lock again, `kill -9` a `--lockscreen-instance` child → new
     child appears within 1s (the unit's default
     KillMode=control-group respawns it), screen stays locked
  3. Unlock normally, verify unit is `inactive` within 2s
  4. Lock again, `systemctl --user kill -s SIGTERM moonbeauty-lockscreen.service`
     → exits 0, socket removed, unit goes `inactive`
