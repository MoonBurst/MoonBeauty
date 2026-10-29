09 ToggleEditModule (RESOLVED, batch 14)

   SettingsMessage::ToggleEditModule set self.editing_module, but
   view() never read it and no view path emitted the message.
   Removed: variant, field, two inits, handler arm.
