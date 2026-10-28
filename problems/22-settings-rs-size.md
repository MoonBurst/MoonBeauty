`settings.rs` is 2,000+ lines.

The `view()` match is huge.

**Fix:** Break each tab into its own `fn view_<tab>()`.