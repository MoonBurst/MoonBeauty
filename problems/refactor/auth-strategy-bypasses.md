# Fix auth strategy bypasses in `LockCoordinator::authenticate()`

**Effort:** 20-30m
**Subsystem:** lockscreen
**Depends on:** —
**Good first issue:** no

## Current state

`src/overlays/lockscreen/coordinator.rs::authenticate()` tries four
strategies in order:

1. `sudo -S -k -v`
2. `pamtester login <user> authenticate`
3. `script -e -c "sudo -k -v" /dev/null`
4. `su -c true <user>` (via `script -e`)

Two of these are bypassable on any misconfigured host:

- **Strategy 1** returns 0 with ANY input when sudoers has a `NOPASSWD:`
  rule for the invoking user (very common: `%wheel ALL=(ALL) NOPASSWD:
  ALL`, docker/podman rules, many single-user NixOS configs). `sudo -v`
  never prompts; stdin password is ignored.

- **Strategy 4** returns 0 WITHOUT prompting when `<user>` equals the
  invoking user. `su -c true $USER` performs no user transition, so
  PAM auth is skipped. This bypass is unconditional on hosts where
  util-linux su is present (i.e. all Linux hosts).

- **Strategy 3** has identical bypass semantics to Strategy 1 (it's
  `sudo -k -v` wrapped in `script`) and is redundant with the guarded
  Strategy 1.

Not exploitable on the test host (sudo requires password here,
su-to-self actually prompts), but real on other deployments.

## Desired state

Four changes in `authenticate()`:

1. **Reorder**: pamtester becomes Strategy 0 — the only strategy that
   is guaranteed to require the actual password.
2. **Guard sudo** with `sudo_is_nopasswd()`: run `sudo -k -n -v` first;
   if exit 0, sudo is NOPASSWD and cannot be used as an auth oracle,
   skip Strategy 1.
3. **Delete Strategy 3** — identical bypass semantics to Strategy 1,
   redundant.
4. **Delete Strategy 4** — always bypassable.

Additionally, a startup diagnostic in `spawn()`:

    [lock-coord] auth: pamtester=Some(...) sudo=Some(...) sudo_nopasswd=false

If `pamtester=None && sudo_nopasswd=true`, log:

    WARNING: no working auth strategy. Lockscreen will FAIL CLOSED —
    even the correct password will not unlock. Fix one of:
    (a) install pamtester AND enable security.pam.services.login,
    (b) remove NOPASSWD from the invoking user's sudoers rule.

## Trade-off

Fails closed. On hosts where pamtester is missing AND sudo is NOPASSWD,
the lockscreen will refuse all passwords. A lock that stays locked is
infinitely better than a lock that opens with any input; the startup
warning tells the user what to configure.

## Verification

- `cargo check --locked`
- `cargo test --locked --lib` (76 passing + new auth tests if added)
- Manual on NOPASSWD host: wrong password does NOT unlock; log shows
  `sudo is NOPASSWD — refusing to use it as auth oracle`
- Manual on standard host: wrong password rejected by pamtester or
  sudo; correct password unlocks
- `--test-lock-auth` still reports `RESULT: ✓ auth succeeded` with
  correct password

## Out of scope

Compiling a replacement for pamtester into the shell (PAM is
dynamically configured; invoking the host's pamtester respects
security.pam.services.*).
