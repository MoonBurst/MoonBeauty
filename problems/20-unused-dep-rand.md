Possibly unused `rand` / `rand_chacha`.

All randomness uses `/dev/urandom` directly.

**Fix:** Remove if `cargo machete` confirms.