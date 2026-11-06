# Problems

Two categories of work items.

## Bug notes (maintainer-written)

Short plain-text notes on behavioral issues. These are symptoms, not
specs — read them for context, not for a fix plan.

- [email](email) — compose window: pressing Enter should open a larger
  reader/responder; currently stays in the 3-pane view
  instead of all (may be a right-click-to-cycle affordance that isn't
  discoverable, not an actual regression — verify before "fixing")
- [music_player](music_player) — scrollbar doesn't follow arrow keys;
  Enter-to-play doesn't play the right track
- [top_bar_background](top_bar_background) — background shape looks odd,
  maybe should use a slanted box
- [unsure_how](unsure_how) — wants clickable custom system-readout
  pieces (borg mount/unmount, podman open in browser)

These are the maintainer's own notes. They are honest about symptoms,
not about causes. Convert to a structured spec (see `refactor/`) before
asking a contributor to work on one.

## Refactor queue (structured)

Scoped, independently-mergeable structural work. Each item has the same
shape:

    **Effort:** ...     (wall-clock for an experienced Rust/Iced dev)
    **Subsystem:** ...
    **Depends on:** ...  (hard block; do not start otherwise)
    **Good first issue:** yes/no

followed by **Current state**, **Desired state**, **Verification**,
**Out of scope**.

See [refactor/README.md](refactor/README.md) for the full index.

## Verification for every refactor item

- `cargo check --locked` — zero warnings
- `cargo test --locked --lib` — 76 passing
- Manual test of the affected feature after
  `systemctl --user restart moonbeauty`
  (the running process is NOT updated by `cargo build`)

## Recently fixed

Notes here that are now resolved. See `git log` for the commits.

- `lockscreen-systemd` — lockscreen coordinator now runs under
  `moonbeauty-lockscreen.service` with `Restart=on-failure`. Independent
  of bar lifecycle; no longer dies on TTY switch or bar restart.
- `lockscreen-dead-ui` — `--toggle lockscreen` was spawning
  `--overlay lockscreen` (UI without coordinator) rather than
  `--lockscreen` (coordinator entry point). Every lock was a dead UI
  that silently dropped Enter. Fixed in `spawn_standalone`.
- `lockscreen-primary-exclusive` — all three per-output children were
  spawned with `KeyboardInteractivity::Exclusive`, letting sway rotate
  focus mid-typing. 22-char passwords arrived as 21 chars. Only primary
  gets Exclusive now.
- `lockscreen-ttyswitch` — coordinator survives TTY switch via its own
  `/sys/class/tty/tty0/active` watcher (respawns children on
  return-to-desktop).
- `bar-ttyswitch` — bar exits on return-to-desktop TTY; systemd respawns
  with fresh layer surfaces (~3s barless window).
- `websearch-utf8` — `build_url` percent-encodes UTF-8 bytes, not
  Unicode code points. `café` was producing `%E9` instead of `%C3%A9`;
  `Ā` was `%100` (invalid escape). 9 unit tests added.
- `handle-web-race` — `--toggle web` was `tokio::spawn(xdg_open) +
  exit(0)`, so the child could be killed mid-fork. Now synchronous
  fork+exec.
- `perms-user-content` — 8 save sites used `fs::write` (0644);
  switched to `private_write_file` (0600). Startup sweep tightens
  pre-existing 0644/0755 files.
- `net-ui-jank` — `update_net` (TcpStream ping + `ss` subprocess) moved
  off UI thread to `spawn_blocking`.
- `podman-tooltip-hang` — 120s connect timeout → 5s, plus
  `tokio::time::timeout` wrapping every daemon call. Tooltip opens in
  <100ms even with stale socket.
- `gpu_regression` — GPU detection reads `/sys/class/drm` directly when
  `lspci` is absent from PATH. Both AMD GPUs (1002:744C, 1002:743F) are
  now detected. Display names strip only the vendor prefix
  ("RX 7900 XTX", "RX 6400").
- `music_search_queue` — playing a search result no longer clears the query and list; you can arrow down and press Enter to play the next one.
- `music_search_queue` — searching now replaces the MPD queue with the search results rather than appending to the user's playlist. Playing a song and letting it end now advances to the next search result instead of looping back to the top of the playlist.
