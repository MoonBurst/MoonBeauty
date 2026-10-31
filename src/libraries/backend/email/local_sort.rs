//! Local-only email sorting.
//!
//! Runs after INBOX fetch. Counts senders; for every sender with ≥ threshold
//! emails and no existing local folder, creates the folder and assigns all
//! that sender's emails to it. INBOX view in the shell filters those out;
//! Gmail's INBOX is untouched.
//!
//! Storage: ~/.local/share/moonbeauty/email_cache/local_sort.json

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::client::NativeEnvelope;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalSort {
    /// folder_name → sender info
    pub folders: HashMap<String, FolderInfo>,
    /// "uid@folder" → folder_name. Which emails are sorted out of their Gmail folder.
    pub assignments: HashMap<String, String>,
    /// sender_key → display name (for the user to see)
    pub sender_names: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    pub sender_key: String,
    pub sender_display: String,
    pub created_at: String,
    pub email_count: usize,
}

pub fn path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("moonbeauty/email_cache/local_sort.json")
}

pub fn load() -> LocalSort {
    let p = path();
    if let Ok(c) = std::fs::read_to_string(&p) {
        if let Ok(s) = serde_json::from_str(&c) {
            return s;
        }
    }
    LocalSort::default()
}

pub fn save(s: &LocalSort) {
    let p = path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }
}

/// Grouping key. Normalizes case and strips `+tag` from Gmail's plus-addressing
/// so `me+steam@gmail.com` and `me@gmail.com` group together.
pub fn sender_key(email: &str, display: &str) -> String {
    let e = email.trim().to_lowercase();
    if !e.is_empty() {
        if let (Some(plus), Some(at)) = (e.find('+'), e.find('@')) {
            if plus < at {
                return format!("{}{}", &e[..plus], &e[at..]);
            }
        }
        return e;
    }
    display.trim().to_lowercase()
}

/// Derive a human folder name from sender.
pub fn folder_name_from_sender(email: &str, display: &str) -> String {
    let mut d = display.trim().to_string();
    for suffix in [
        " Support",
        " Team",
        " Notifications",
        " Notification",
        " Alerts",
        " Alert",
    ] {
        if let Some(stripped) = d.strip_suffix(suffix) {
            d = stripped.to_string();
            break;
        }
    }
    if !d.is_empty() {
        return crate::libraries::process::sanitize_folder_name(&d);
    }
    // steampowered.com → Steampowered
    if let Some(at) = email.find('@') {
        let domain = &email[at + 1..];
        let root = domain.split('.').next().unwrap_or(domain);
        let mut c = root.chars();
        return crate::libraries::process::sanitize_folder_name(&match c.next() {
            Some(f) => f.to_uppercase().chain(c).collect::<String>(),
            None => "Unknown".to_string(),
        });
    }
    "Unknown".to_string()
}

/// Run auto-sort. Idempotent — running twice creates nothing new.
/// Returns (folders_created, emails_assigned).
pub fn auto_sort(
    envelopes: &[NativeEnvelope],
    gmail_folder: &str,
    threshold: usize,
) -> (usize, usize) {
    if envelopes.is_empty() || threshold == 0 {
        return (0, 0);
    }
    let mut state = load();
    let mut created = 0;
    let mut assigned = 0;

    // Group by sender key
    let mut by_sender: HashMap<String, Vec<&NativeEnvelope>> = HashMap::new();
    for env in envelopes {
        let key = sender_key(&env.sender_email, &env.sender);
        state.sender_names.insert(key.clone(), env.sender.clone());
        by_sender.entry(key).or_default().push(env);
    }

    for (key, envs) in &by_sender {
        if envs.len() < threshold {
            continue;
        }
        // Already have a folder for this sender?
        if state.folders.values().any(|f| &f.sender_key == key) {
            // Ensure all matching emails are assigned (in case new ones arrived)
            let folder_name = state
                .folders
                .iter()
                .find(|(_, f)| &f.sender_key == key)
                .map(|(n, _)| n.clone());
            if let Some(name) = folder_name {
                for env in envs {
                    let k = format!("{}@{}", env.id, gmail_folder);
                    if !state.assignments.contains_key(&k) {
                        state.assignments.insert(k, name.clone());
                        assigned += 1;
                    }
                }
            }
            continue;
        }

        // Create new folder. If a Gmail label shares the name, suffix it
        // to avoid a confusing duplicate in the sidebar.
        let base = folder_name_from_sender(&envs[0].sender_email, &envs[0].sender);
        let gmail_collision = super::client::load_folders_cache()
            .iter()
            .any(|f| f == &base);
        let base_with_suffix = if gmail_collision {
            format!("{} (local)", base)
        } else {
            base.clone()
        };
        let mut final_name = base_with_suffix.clone();
        let mut n = 2;
        while state.folders.contains_key(&final_name) {
            final_name = format!("{} ({})", base_with_suffix, n);
            n += 1;
        }
        state.folders.insert(
            final_name.clone(),
            FolderInfo {
                sender_key: key.clone(),
                sender_display: envs[0].sender.clone(),
                created_at: chrono::Local::now().to_rfc3339(),
                email_count: envs.len(),
            },
        );
        created += 1;

        for env in envs {
            let k = format!("{}@{}", env.id, gmail_folder);
            state.assignments.insert(k, final_name.clone());
            assigned += 1;
        }
    }

    // Refresh counts on existing folders
    for info in state.folders.values_mut() {
        info.email_count = by_sender
            .get(&info.sender_key)
            .map(|v| v.len())
            .unwrap_or(0);
    }

    if created > 0 || assigned > 0 {
        save(&state);
        eprintln!(
            "[auto-sort] {} folders created, {} emails assigned (threshold {})",
            created, assigned, threshold
        );
    }
    (created, assigned)
}

/// Split envelope list into (assigned-to-local-folder, stays-visible).
pub fn partition(
    envelopes: &[NativeEnvelope],
    gmail_folder: &str,
) -> (Vec<NativeEnvelope>, Vec<NativeEnvelope>) {
    let state = load();
    let mut moved = Vec::new();
    let mut stayed = Vec::new();
    for env in envelopes {
        let k = format!("{}@{}", env.id, gmail_folder);
        if state.assignments.contains_key(&k) {
            moved.push(env.clone());
        } else {
            stayed.push(env.clone());
        }
    }
    (moved, stayed)
}

/// All envelopes belonging to a local folder, using cached INBOX data.
pub fn envelopes_in_local_folder(
    inbox_envelopes: &[NativeEnvelope],
    folder_name: &str,
) -> Vec<NativeEnvelope> {
    let state = load();
    let Some(info) = state.folders.get(folder_name) else {
        return Vec::new();
    };
    inbox_envelopes
        .iter()
        .filter(|e| sender_key(&e.sender_email, &e.sender) == info.sender_key)
        .cloned()
        .collect()
}

/// Produce Gmail filter text for a local folder. User pastes this into
/// Gmail → Settings → Filters → Create new filter → "Matches: ...".
pub fn gmail_filter_text_for(folder_name: &str) -> Option<String> {
    let state = load();
    let info = state.folders.get(folder_name)?;
    Some(format!(
        "Matches: from:({})\nDo this: Skip Inbox, Apply label \"{}\"",
        info.sender_key, folder_name
    ))
}

/// If `current_folder` is a local folder and `uid` belongs to it,
/// return the Gmail folder the email actually lives in. Used by
/// `SelectMail` so body-fetch goes to the right IMAP path.
pub fn source_folder_for(uid: &str, current_folder: &str) -> Option<String> {
    let state = load();
    if !state.folders.contains_key(current_folder) {
        return None;
    }
    for (key, local_name) in &state.assignments {
        if local_name != current_folder {
            continue;
        }
        if let Some(at) = key.find('@') {
            if &key[..at] == uid {
                return Some(key[at + 1..].to_string());
            }
        }
    }
    None
}

/// Remove a local folder. Assignments are cleared so emails reappear in INBOX.
pub fn delete_folder(folder_name: &str) -> bool {
    let mut state = load();
    if state.folders.remove(folder_name).is_none() {
        return false;
    }
    state.assignments.retain(|_, v| v != folder_name);
    save(&state);
    true
}

#[cfg(test)]
mod folder_name_tests {
    use super::folder_name_from_sender;

    #[test]
    fn malicious_display_names_sanitized() {
        for display in &[
            "../../.config/autostart/evil",
            "..",
            ".",
            "/etc/passwd",
            "..\\..\\evil",
            "a/b",
        ] {
            let out = folder_name_from_sender("x@evil.com", display);
            assert!(!out.contains('/'), "{:?} → {:?}", display, out);
            assert!(!out.contains('\\'), "{:?} → {:?}", display, out);
            assert!(!out.contains(".."), "{:?} → {:?}", display, out);
            assert!(!out.starts_with('.'), "{:?} → {:?}", display, out);
            assert!(!out.is_empty(), "{:?} → empty", display);
        }
    }

    #[test]
    fn benign_display_names_unchanged() {
        for name in &["GitHub", "Acme Corp.", "Foo (local)", "version 1.2"] {
            assert_eq!(
                folder_name_from_sender("x@example.com", name),
                *name,
                "{:?} changed",
                name
            );
        }
    }

    #[test]
    fn suffix_strip_still_works() {
        assert_eq!(
            folder_name_from_sender("x@steampowered.com", "Steam Support"),
            "Steam"
        );
    }
}
