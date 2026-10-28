//! Lockscreen keyboard event to Key mapping.
//!
//! Extracted from handle_lock_keypress in main.rs so the logic is
//! testable without spinning up a lockscreen. The wrapper in main.rs
//! applies the Status::Captured filter and wraps the result in
//! Message::Lock(LockMessage::KeyPress(..)).

pub fn resolve_lock_key(event: iced::Event) -> Option<iced::keyboard::Key> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        modified_key, text, ..
    }) = event
    else {
        return None;
    };
    use iced::keyboard::{key::Named, Key};

    // 1. Named keys first (control chars would be rejected by text filter).
    if let Key::Named(named) = &modified_key {
        if matches!(named, Named::Enter | Named::Backspace | Named::Tab) {
            return Some(modified_key);
        }
    }

    // 2. Printable characters: prefer `text`.
    if let Some(t) = text {
        if !t.is_empty() && !t.chars().any(|c| c.is_control()) {
            return Some(Key::Character(t));
        }
    }

    // 3. Fallback: modified_key.
    if let Key::Character(c) = modified_key {
        if !c.is_empty() {
            return Some(Key::Character(c));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{Key, Location, Modifiers};

    /// Build a KeyPressed event with the given modified_key and text fields.
    /// `key` is set equal to `modified_key` — resolve_lock_key doesn't read
    /// it, so the choice is arbitrary but documented here.
    fn key_pressed(modified_key: Key, text: Option<&'static str>) -> iced::Event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: modified_key.clone(),
            modified_key,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::default(),
            text: text.map(Into::into),
            repeat: false,
        })
    }

    /// Regression: Enter's `text` field is `\r` (a control character). If
    /// the printable filter runs first, Enter is silently dropped and the
    /// coordinator never sees Submit — the lockscreen stays locked.
    #[test]
    fn enter_returns_named_enter_despite_control_char_text() {
        use iced::keyboard::key::Named;
        let ev = key_pressed(Key::Named(Named::Enter), Some("\r"));
        assert!(matches!(
            resolve_lock_key(ev),
            Some(Key::Named(Named::Enter))
        ));
    }

    /// Regression: same trap as Enter — Tab's `text` is `\t`.
    #[test]
    fn tab_returns_named_tab() {
        use iced::keyboard::key::Named;
        let ev = key_pressed(Key::Named(Named::Tab), Some("\t"));
        assert!(matches!(resolve_lock_key(ev), Some(Key::Named(Named::Tab))));
    }

    #[test]
    fn backspace_returns_named_backspace() {
        use iced::keyboard::key::Named;
        let ev = key_pressed(Key::Named(Named::Backspace), None);
        assert!(matches!(
            resolve_lock_key(ev),
            Some(Key::Named(Named::Backspace))
        ));
    }

    /// Regression: `key` field ignores modifiers (Shift+P → "p"). The
    /// resolve function must prefer `text` so the uppercase "P" survives.
    #[test]
    fn shift_p_returns_uppercase() {
        let ev = key_pressed(Key::Character("P".into()), Some("P"));
        assert!(matches!(
            resolve_lock_key(ev),
            Some(Key::Character(c)) if c == "P"
        ));
    }

    #[test]
    fn plain_lowercase_passes_through() {
        let ev = key_pressed(Key::Character("p".into()), Some("p"));
        assert!(matches!(
            resolve_lock_key(ev),
            Some(Key::Character(c)) if c == "p"
        ));
    }

    /// Fallback path: no `text` provided, falls back to `modified_key`.
    #[test]
    fn character_without_text_uses_modified_key() {
        let ev = key_pressed(Key::Character("P".into()), None);
        assert!(matches!(
            resolve_lock_key(ev),
            Some(Key::Character(c)) if c == "P"
        ));
    }

    /// Modifier keys (Shift, Ctrl, etc.) produce no text and shouldn't
    /// be forwarded to the password buffer.
    #[test]
    fn modifier_only_returns_none() {
        use iced::keyboard::key::Named;
        let ev = key_pressed(Key::Named(Named::Shift), None);
        assert!(resolve_lock_key(ev).is_none());
    }
}
