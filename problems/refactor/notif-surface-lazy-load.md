# Lazy-load the notification surface (Save 20 MB RAM)

**Effort:** 30–45m
**Subsystem:** notifications / lifecycle
**Depends on:** —
**Good first issue:** yes

## Current state

`notif-surface` (`moonbeauty --notif-surface`) is spawned at bar startup and runs forever, consuming ~20 MB of physical RAM (`Private_Dirty`) even when there are zero notifications.

## Desired state

1. Remove the eager `spawn_and_reap` block for `notif-surface` in `src/main.rs`.
2. Add an idle timer to `notif-surface`'s `Tick` handler to exit after 60 seconds of zero active/pending toasts.
3. Rely on the existing D-Bus service file (`org.freedesktop.Notifications.service`) to spawn it on demand when a notification arrives.

## Trade-off

The first notification after a quiet period will take ~300ms longer to appear due to D-Bus activation latency. Subsequent notifications are instant while the surface is awake.

## Verification

- `ps -eo rss,args | grep -- '--notif-surface'` should show no output after 60s of idle time.
- Send a notification (`notify-send "test"`) and verify it appears.
- Wait 60s and verify the notif-surface process exits again.
