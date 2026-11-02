# Rename IPC socket to `moonbeauty.sock`

**Effort:** 10 min + no-transition restart
**Subsystem:** IPC
**Depends on:** —
**Good first issue:** no

## Current state

Everything else has been migrated from `moon-beauty-shell` to
`moonbeauty` (crate, binary, config dir, data dir, cache files), but
the IPC socket is still `/run/user/1000/moon-beauty-shell.sock`.

Works fine because client (`main.rs`) and server (`app/new_app.rs`) both
call the same `get_socket_path()` which returns the old path. Purely
inconsistent naming.

## Desired state

`libraries/ipc/protocol.rs::get_socket_path()` returns
`$XDG_RUNTIME_DIR/moonbeauty.sock`. Update any literal
`moon-beauty-shell.sock` in source and scripts.

## Caution

Requires a **no-transition restart**: old client cannot talk to new
server socket mid-flight. Do when the shell isn't actively in use:
1. Apply source change
2. `cargo build --release`
3. `pkill -9 moonbeauty`
4. `systemctl --user start moonbeauty`
5. Verify `ls /run/user/1000/moonbeauty.sock` and
   `moonbeauty --toggle launcher`

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- `moonbeauty --toggle launcher` opens the launcher
- Old socket path no longer exists
