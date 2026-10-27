Dead `Message::WindowResized`.

Fires from `resize_events`, stores in `self.window_w`, then `view_toasts(_window_w)` ignores it.

**Fix:** Remove the message, field, and argument.