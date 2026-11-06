# Reap self-terminating tooltip children

**Effort:** 5m
**Subsystem:** tooltips
**Depends on:** —
**Good first issue:** yes

## Current state

Tooltip children spawned by the bar (`weather_tooltip_child` field in
`MoonBeautyShellApp`) self-terminate via `std::process::exit(0)` on:

- `AUTO_MS` timeout (25s for tray popup, 20s for weather)
- User dismissal
- Auth success (lockscreen does not use this path — separate)

The bar never calls `try_wait()` on those children. When they exit on
their own, `Child`'s `Drop` does NOT reap them (Rust documents this:
"there is no Drop implementation that closes stdin/stdout/stderr
pipes or waits for the child to exit"). They become zombies
(`[moonbeauty] <defunct>` in `ps`).

One observed during a session:
    pid=68988  ppid=3948487  etime=01:14  [moonbeauty] <defunct>

Zero memory cost per zombie, but they accumulate over hours of use.
Not a leak, but a hygiene issue — and one that obscures real problems
(a real zombie is a symptom of bad child management).

## Desired state

One-line addition to `Message::TickHardware` (or `Message::IdleTick`,
which fires every 1.5s — plenty for reaping):

    if let Some(c) = self.weather_tooltip_child.as_mut() {
        let _ = c.try_wait();
    }

The `try_wait()` returns `Ok(Some(status))` if the child exited; we
ignore the status (nothing we can do with it). The `Child` struct
internally marks the process as reaped so `Drop` won't attempt again.

## Verification

- `cargo check --locked`
- `cargo test --locked --lib` (76 passing)
- Manual: open and dismiss 10 tooltips, then:
    `ps -eo pid,ppid,args | grep -c '<defunct>'` — expect 0
  for the bar's ppid specifically.

## Out of scope

Applying the same to `standalone_child` (overlay windows) — those
already use `try_wait()` in `standalone_running()`.
