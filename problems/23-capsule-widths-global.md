Fragile `CAPSULE_WIDTHS` global.

Widths are recorded in a global mutex during render, then read for position math. If a width changes mid-frame, clicks are off for one frame.

**Fix:** Return widths from render, store on app struct.