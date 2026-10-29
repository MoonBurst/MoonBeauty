Lockscreen input was dead.

`lock_key_sub` in main.rs was wired to `handle_email_keypress`. Fixed by adding `handle_lock_keypress`.

**Status:** Fixed in Option C.