Dead `__tab__` branch.

`LauncherMessage::ExecuteApp("__tab__")` is guarded in main.rs, but nothing emits it.

**Fix:** Remove the guard.