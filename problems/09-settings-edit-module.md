Dead `ToggleEditModule`.

`SettingsMessage::ToggleEditModule` sets `editing_module`, but `view()` never reads it.

**Fix:** Remove the field and the message.