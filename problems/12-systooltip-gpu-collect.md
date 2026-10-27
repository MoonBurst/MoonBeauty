Leftover `collect_gpu` in `system_tooltip.rs`.

There is a `let _ = SystemCollector::collect_gpu(...)` in the GPU branch. It does nothing.

**Fix:** Remove it.