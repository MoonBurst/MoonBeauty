# Split `backend/email/client/mod.rs` into a folder

**Effort:** 4–6h
**Subsystem:** email backend
**Depends on:** —
**Good first issue:** no

## Current state

`src/libraries/backend/email/client/mod.rs` is ~1,415 lines.

Already extracted to sibling files:
- `client/types.rs` — `NativeEnvelope`, `ParsedEmail`
- `client/validation.rs` — `is_safe_imap_uid`, `is_safe_imap_folder` + tests
- `client/secure_fs.rs` — `secure_write*`, `chmod_private`, `ensure_secure_cache_tree*` + tests

Still inline in `mod.rs`: config + sops integration, `resolve_password`,
`find_sops`, IMAP connect/fetch/sync/move/delete, `read_message`,
`send_message` (SMTP), contacts CRUD, folder cache, `save_draft`.

## Desired state

    src/libraries/backend/email/client/
    ├── mod.rs           re-exports + shared types only
    ├── types.rs         (already exists)
    ├── validation.rs    (already exists)
    ├── secure_fs.rs     (already exists)
    ├── config.rs        EmailConfig, load/save, sops
    ├── imap.rs          connect, fetch, sync, move/delete/star
    ├── smtp.rs          send_message
    ├── parse.rs         read_message, MIME helpers
    ├── contacts.rs      contacts CRUD
    └── folders.rs       map_gmail_folder, cache paths

## Caution

`is_safe_imap_uid`, `is_safe_imap_folder`, and `secure_write*` are
security-critical. Tests exist (`imap_uid_tests`, `secure_fs_tests`,
`cache_dir_tests`). All must still pass — they're the coverage.

## Verification

- `cargo check --locked` (zero warnings)
- `cargo test --locked --lib` — 67 passing (security tests unchanged)
- Manual: `moonbeauty --toggle email`, 3-pane layout loads, arrow keys
  navigate, Enter opens body, compose works.
