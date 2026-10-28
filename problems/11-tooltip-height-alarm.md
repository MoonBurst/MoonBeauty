Unreachable `"alarm" => 340`.

`compute_tooltip_height` still has an alarm arm, but alarm routes to the standalone overlay before tooltip spawn.

**Fix:** Remove the arm.