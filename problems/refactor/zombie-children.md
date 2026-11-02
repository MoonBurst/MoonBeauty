# Reap tooltip / child processes (zombies)

**Effort:** 15 min
**Subsystem:** process management
**Depends on:** —
**Good first issue:** yes

## Current state

Several `std::process::Command::spawn()` sites drop the `Child` handle
without `wait()`, leaving `<defunct>` zombies until the bar restarts.
Observed: ~8 zombies after ~1 day of uptime. Cost: ~4 KB kernel struct
each (negligible RAM), but grows unbounded over long sessions.

Known sites (verify with `grep -n 'Command::new' src/main.rs` and
`src/app/new_app.rs`):
- `main.rs` — `--notif-surface` startup spawn
- `main.rs` — DND toggle respawn
- `app/new_app.rs` — `wl-paste --watch` spawn

Tracked children (`standalone_child`, `weather_tooltip_child`) already
reap correctly via `try_wait()`.

## Desired state

Replace fire-and-forget `.spawn()` calls with
`moonbeauty::libraries::process::spawn_and_reap()`, which spawns and
detaches a wait thread:

    pub fn spawn_and_reap(cmd: &mut std::process::Command) -> io::Result<u32>

Already exists in `libraries/process.rs` and is used for `notify-send` /
`xdg-open` elsewhere.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` (67 passing)
- Manual: restart the bar, open+close 20 tooltips, then
  `ps -eo state,cmd | grep -c '^Z.*moonbeauty'` — should be 0.

## Out of scope

- Converting tracked children to use `spawn_and_reap` — they need the
  `Child` handle for `kill()` and `try_wait()`.
