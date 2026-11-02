# Split `libraries/process.rs` into a folder

**Effort:** 2h
**Subsystem:** filesystem utils
**Depends on:** —
**Good first issue:** no

## Current state

`src/libraries/process.rs` is ~801 lines. Contains seven distinct
concerns: sanitize, XDG runtime paths, private writes, permission
hygiene, app-id/icon safety, JSON reader, spawn helpers. Tests in-file.

## Desired state

    src/libraries/process/
    ├── mod.rs           re-exports + docs
    ├── paths.rs         mbs_runtime_dir, mbs_subdir, mbs_tmp_dir,
    │                    mbs_unique_tmp, home, config_dir, data_dir, cache_dir
    ├── perms.rs         ensure_private_dir, chmod_mode, private_*,
    │                    tighten_existing_runtime_state
    ├── sanitize.rs      sanitize_folder_name, is_safe_app_id,
    │                    safe_icon_candidate
    ├── json.rs          read_json_capped
    ├── spawn.rs         spawn_and_reap, resolve_exe, random_token
    └── imagemagick.rs   ensure_imagemagick_policy

Tests move with their targets. All must still pass.

## Caution

`private_create_new` and `private_write_new` use `O_CREAT | O_EXCL` to
reject symlink squats. The tests
(`private_create_new_rejects_symlink_squat`) are the reason this works —
do NOT change their semantics.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` — 67 passing (all tests move but still run)
