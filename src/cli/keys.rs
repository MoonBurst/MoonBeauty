//! Keyboard event handlers + app-focus helpers.
//!
//! Extracted from `main.rs`. All `pub` fns reach their call sites in
//! `app/subscription.rs` and `app/update/notifications.rs` via the
//! chain `super::*` -> `crate::app::*` -> `use crate::cli::keys::{...}`
//! in `app/mod.rs`.


use crate::app::Message;
use moonbeauty::overlays::email::EmailMessage;
use moonbeauty::overlays::lockscreen::LockMessage;
use moonbeauty::overlays::screenshot::annotator::AnnotatorMessage;

pub fn focused_app_id() -> Option<String> {
    let out = std::process::Command::new("swaymsg")
        .args(["-t", "get_tree", "-r"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    fn walk(node: &serde_json::Value) -> Option<String> {
        if node.get("focused").and_then(|f| f.as_bool()) == Some(true) {
            if let Some(app_id) = node.get("app_id").and_then(|a| a.as_str()) {
                if !app_id.is_empty() {
                    return Some(app_id.to_string());
                }
            }
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(nodes) = node.get(key).and_then(|n| n.as_array()) {
                for child in nodes {
                    if let Some(r) = walk(child) {
                        return Some(r);
                    }
                }
            }
        }
        None
    }
    walk(&v)
}

pub fn notification_candidates(app_name: &str, desktop_entry: Option<&str>) -> Vec<String> {
    use std::collections::HashSet;
    let lower = app_name.to_lowercase();
    let mut cand_set: HashSet<String> = HashSet::new();
    cand_set.insert(lower.clone());
    cand_set.insert(lower.replace(' ', "-"));
    cand_set.insert(lower.replace(' ', "_"));
    cand_set.insert(lower.replace(' ', ""));
    if let Some(de) = desktop_entry {
        let de_clean = de.trim_end_matches(".desktop").to_lowercase();
        cand_set.insert(de_clean);
    }
    cand_set
        .into_iter()
        .filter(|s| !s.is_empty() && moonbeauty::libraries::process::is_safe_app_id(s))
        .collect()
}

pub fn is_app_focused(candidates: &[String]) -> bool {
    let Some(focused) = focused_app_id() else {
        return false;
    };
    candidates.iter().any(|c| focused.eq_ignore_ascii_case(c))
}

pub fn focus_notification_app(app_name: &str, desktop_entry: Option<&str>) -> bool {
    use std::collections::HashSet;

    let lower = app_name.to_lowercase();
    let mut cand_set: HashSet<String> = HashSet::new();
    cand_set.insert(lower.clone());
    cand_set.insert(lower.replace(' ', "-"));
    cand_set.insert(lower.replace(' ', "_"));
    cand_set.insert(lower.replace(' ', ""));
    if let Some(de) = desktop_entry {
        let de_clean = de.trim_end_matches(".desktop").to_string();
        cand_set.insert(de_clean.clone());
        cand_set.insert(de_clean.to_lowercase());
    }
    let candidates: Vec<String> = cand_set
        .into_iter()
        .filter(|s| !s.is_empty() && moonbeauty::libraries::process::is_safe_app_id(s))
        .collect();
    moonbeauty::dbg_log!(
        "[focus] app={:?} entry={:?} candidates={:?}",
        app_name,
        desktop_entry,
        candidates
    );

    if std::env::var("SWAYSOCK").is_ok() {
        for cand in &candidates {
            let _ = std::process::Command::new("swaymsg")
                .args(["-t", "command", &format!("[app_id=\"{}\"] focus", cand)])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        if let Some(focused) = focused_app_id() {
            for cand in &candidates {
                if focused.eq_ignore_ascii_case(cand) {
                    return true;
                }
            }
        }
    }

    if let Some(de) = desktop_entry {
        let de_clean = de.trim_end_matches(".desktop");
        if moonbeauty::libraries::process::is_safe_app_id(de_clean) {
            let ok = std::process::Command::new("gtk-launch")
                .arg(de_clean)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
    }

    if let Some(first) = candidates.first() {
        let _ = std::process::Command::new("gtk-launch").arg(first).status();
    }
    false
}

pub fn handle_lock_keypress(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    if status == iced::event::Status::Captured {
        return None;
    }
    let debug = std::env::var_os("MBS_LOCK_DEBUG").is_some();
    moonbeauty::overlays::lockscreen::resolve_lock_key(event).map(|k| {
        if debug {
            eprintln!("[lock-key] resolved: {:?}", k);
        }
        Message::Lock(LockMessage::KeyPress(k))
    })
}

pub fn handle_email_keypress(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event
    else {
        return None;
    };
    use iced::keyboard::{key::Named, Key};

    // ---- SEND: three shortcuts to maximize compatibility ----
    if key == Key::Named(Named::Enter) && (modifiers.control() || modifiers.alt()) {
        eprintln!("[key] Send triggered via Enter+mod");
        return Some(Message::Email(EmailMessage::SendMail));
    }
    if let Key::Character(c) = &key {
        if modifiers.control() && c.to_lowercase() == "s" {
            eprintln!("[key] Send triggered via Ctrl+S");
            return Some(Message::Email(EmailMessage::SendMail));
        }
        if modifiers.control() && c.to_lowercase() == "o" {
            eprintln!("[key] Attach triggered via Ctrl+O");
            return Some(Message::Email(EmailMessage::AddAttachment));
        }
    }

    if key == Key::Named(Named::Tab) {
        return Some(Message::ComposeTabPressed(modifiers.shift()));
    }

    if modifiers.alt() {
        match key {
            Key::Named(Named::ArrowUp) => {
                return Some(Message::Email(EmailMessage::SelectFolderPrev))
            }
            Key::Named(Named::ArrowDown) => {
                return Some(Message::Email(EmailMessage::SelectFolderNext))
            }
            _ => {}
        }
    }

    if modifiers.control() {
        if let Key::Character(c) = &key {
            match c.to_lowercase().as_str() {
                "n" | "j" => return Some(Message::Email(EmailMessage::SelectMailNext)),
                "p" | "k" => return Some(Message::Email(EmailMessage::SelectMailPrev)),
                _ => {}
            }
        }
    }

    if matches!(key, Key::Character(_))
        && status == iced::event::Status::Captured
        && !modifiers.control()
    {
        return None;
    }

    match key {
        Key::Named(Named::ArrowUp) => Some(Message::Email(EmailMessage::SelectMailPrev)),
        Key::Named(Named::ArrowDown) => Some(Message::Email(EmailMessage::SelectMailNext)),
        Key::Named(Named::Enter) => Some(Message::Email(EmailMessage::OpenSelected)),
        Key::Named(Named::Delete) => Some(Message::Email(EmailMessage::DeleteSelected)),
        Key::Character(c) if c == "n" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::OpenComposer))
        }
        Key::Character(c) if c == "s" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::ToggleStar))
        }
        Key::Character(c) if c == "d" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::DeleteSelected))
        }
        Key::Character(c) if c == "c" && !modifiers.control() => {
            Some(Message::Email(EmailMessage::SaveContact))
        }
        _ => None,
    }
}

/// Keyboard handler for annotator text-annotation mode. Forwards printable
/// characters, Enter, and Backspace to the annotator's text buffer. Modifier
/// combos fall through (they're reserved for tool shortcuts / global binds);
/// events already captured by a focused widget (e.g. the watermark
/// text_input) are skipped so we don't double-handle them.
pub fn handle_annotator_text_input(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        modified_key,
        text,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    use iced::keyboard::{key::Named, Key};

    if modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    if status == iced::event::Status::Captured {
        return None;
    }

    // Named keys first (Enter's `text` is Some("\r"), a control char —
    // must be intercepted here before the printable filter below).
    if let Key::Named(named) = &modified_key {
        match named {
            Named::Enter => {
                return Some(Message::Annotator(AnnotatorMessage::NewLinePressed));
            }
            Named::Backspace => {
                return Some(Message::Annotator(AnnotatorMessage::BackspacePressed));
            }
            _ => {}
        }
    }

    // Printable: prefer `text` (respects IME, layout, shift/caps). Convert
    // SmolStr → String because AnnotatorMessage::CharacterInput takes String.
    if let Some(t) = text {
        if !t.is_empty() && !t.chars().any(|c| c.is_control()) {
            return Some(Message::Annotator(AnnotatorMessage::CharacterInput(
                t.to_string(),
            )));
        }
    }

    // Fallback: modified_key.
    if let Key::Character(c) = modified_key {
        if !c.is_empty() {
            return Some(Message::Annotator(AnnotatorMessage::CharacterInput(
                c.to_string(),
            )));
        }
    }

    None
}
