Orphaned right-click on alarm capsule.

`CancelTooltip("alarm")` wipes timers. Unreachable now that left-click goes to `toggle_standalone(Reminders)`.

**Fix:** Remove the `CancelTooltip("alarm")` branch.