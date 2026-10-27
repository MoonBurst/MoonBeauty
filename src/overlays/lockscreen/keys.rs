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
    use iced::keyboard::{Key, key::Named};

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
