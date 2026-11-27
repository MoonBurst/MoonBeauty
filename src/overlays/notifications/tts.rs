//! Notification text-to-speech readout formatting and dispatch.

pub fn speak_tts(
    app_name: &str,
    summary: &str,
    body: &str,
    custom_readout: Option<&str>,
    tts_command: Option<String>,
    volume: u32,
    read_body: bool,
) {
    let a = app_name.to_string();
    let s = summary.to_string();
    let b = body.to_string();
    let mut readout_template = custom_readout.map(|s| s.to_string()).unwrap_or_else(|| {
        let lower = format!("{} {}", s, b).to_lowercase();
        if lower.contains("logged in") || lower.contains("signed in") {
            "{sender} has logged in".to_string()
        } else {
            "Message from {sender}".to_string()
        }
    });
    if !read_body {
        for pat in [
            ": {body}", ": $body", ": {message}", ": $message",
            " - {body}", " - $body", " - {message}", " - $message",
            " {body}", " $body", " {message}", " $message",
            "{body}", "$body", "{message}", "$message",
        ] {
            readout_template = readout_template.replace(pat, "");
        }
        readout_template = readout_template
            .trim()
            .trim_end_matches(':')
            .trim()
            .to_string();
    }

    std::thread::spawn(move || {
        // Smart sender extraction
        let mut sender = String::new();
        let mut msg = b.clone();

        // 1. Try to parse "Alice: Hello" from the body
        if let Some(idx) = b.find(':') {
            let prefix = b[..idx].trim();
            if !prefix.is_empty() && prefix.len() < 30 && !prefix.contains("http") {
                sender = prefix.to_string();
                msg = b[idx + 1..].trim().to_string();
            }
        }

        // 2. If no sender found, check if the summary is a real name
        if sender.is_empty() {
            let lower = s.to_lowercase();
            let is_generic = lower.contains("new message")
                || lower.contains("new voice")
                || lower.contains("voice activity")
                || lower.contains("notification")
                || lower == "test"
                || looks_like_action(&s);

            if !is_generic {
                sender = s.clone();
                // Strip common login suffixes so "{sender} has logged in"
                // doesn't become "Player has logged in has logged in".
                let lower_sender = sender.to_lowercase();
                for suffix in [
                    " has logged in",
                    " has signed in",
                    " is now online",
                    " logged in",
                ] {
                    if lower_sender.ends_with(suffix) {
                        sender = sender[..sender.len() - suffix.len()].trim().to_string();
                        break;
                    }
                }
            } else {
                // Summary is generic. Try to find the real sender:
                //   "New Voice from Alice" -> "Alice"
                //   "Alice spoke in room"  -> "Alice"
                //   "Alice" (short body)   -> "Alice"
                //   else                   -> app name
                if let Some(name) = extract_from_phrase(&s) {
                    sender = name;
                } else if let Some(name) = extract_leading_name(&b) {
                    let remaining = b[name.len()..].trim().to_string();
                    sender = name;
                    if !remaining.is_empty() {
                        msg = remaining;
                    }
                } else {
                    sender = a.clone();
                }
            }
        }

        // Belt-and-suspenders: reject any sender that still looks
        // like an action phrase, using the app name instead.
        if !sender.is_empty() && looks_like_action(&sender) {
            crate::dbg_log!(
                "[tts-sanitize] rejecting action-phrase sender={:?}, using app={:?}",
                sender,
                a
            );
            sender = a.clone();
        }

        // Post-process: try to rescue a bad sender by looking in
        // the summary. Runs after the primary extraction so we can
        // also improve cases where the original path fell back to
        // the app name or used an action phrase as the "name".
        let needs_tts_rescue = sender.is_empty() || sender == a || looks_like_action(&sender);
        if needs_tts_rescue {
            let sender_before = sender.clone();
            let rescued = extract_name_from_summary(&s)
                .or_else(|| extract_from_phrase(&s))
                .or_else(|| looks_like_clean_name(&s).then(|| s.trim().to_string()));
            match rescued {
                Some(n) => {
                    crate::dbg_log!(
                        "[tts-rescue] summary={:?} sender {:?} -> {:?}",
                        s,
                        sender_before,
                        n
                    );
                    sender = n;
                }
                None => {
                    if sender.is_empty() || looks_like_action(&sender) || sender == a {
                        crate::dbg_log!(
                            "[tts-rescue] no name in summary={:?}, using app={:?}",
                            s,
                            a
                        );
                        sender = a.clone();
                    }
                }
            }
        }

        let template_has_body = ["{body}", "$body", "{message}", "$message"]
            .iter()
            .any(|k| readout_template.contains(k));
        let is_login_rule = {
            let low = readout_template.to_lowercase();
            low.contains("logged in") || low.contains("signed in")
        };
        let mut spoken = readout_template.clone();
        for (key, val) in [
            ("{sender}", &sender),
            ("$sender", &sender),
            ("{user}", &sender),
            ("$user", &sender),
            ("{name}", &sender),
            ("$name", &sender),
            ("{message}", &msg),
            ("$message", &msg),
            ("{body}", &msg),
            ("$body", &msg),
            ("{summary}", &s),
            ("$summary", &s),
            ("{app}", &a),
            ("$app", &a),
            ("{app_name}", &a),
            ("$app_name", &a),
            ("{appname}", &a),
            ("$appname", &a),
        ] {
            spoken = spoken.replace(key, val);
        }

        if read_body && !template_has_body && !is_login_rule {
            let clean_msg = msg.trim();
            if !clean_msg.is_empty() && !clean_msg.eq_ignore_ascii_case(&sender) {
                spoken = format!("{}: {}", spoken.trim_end(), clean_msg);
            }
        }

        crate::dbg_log!("[tts-trace] app={:?} summary={:?} body={:?}", a, s, b);
        crate::dbg_log!(
            "[tts-trace] extracted sender={:?} message={:?}",
            sender,
            msg
        );
        crate::dbg_log!(
            "[tts-trace] template={:?} -> spoken={:?}",
            readout_template,
            spoken
        );

        if let Some(cmd) = tts_command {
            let vol_str = format!("{:.2}", (volume as f32) / 100.0);
            let script = if cmd.contains("{text}") {
                cmd.replace("{text}", "\"$1\"")
                    .replace("{volume}", &vol_str)
            } else {
                format!("{} \"$1\"", cmd)
            };
            match std::process::Command::new("sh")
                .args(["-c", &script, "--", &spoken])
                .output()
            {
                Ok(o) => eprintln!(
                    "[tts] custom exit={:?} stderr={}",
                    o.status.code(),
                    String::from_utf8_lossy(&o.stderr)
                        .trim()
                        .chars()
                        .take(200)
                        .collect::<String>(),
                ),
                Err(e) => eprintln!("[tts] custom spawn failed: {e}"),
            }
            return;
        }

        let candidates: Vec<String> = vec![
            "/run/current-system/sw/bin/sage-tts".into(),
            "sage-tts".into(),
            "spd-say".into(),
            "espeak-ng".into(),
            "espeak".into(),
        ];
        let mut tried: Vec<String> = Vec::new();
        for bin in &candidates {
            tried.push(bin.clone());
            let mut cmd = std::process::Command::new(bin);
            if bin.contains("espeak") {
                let amp = (volume * 2).min(200).to_string();
                cmd.args(["-a", &amp]);
            }
            match cmd.arg(&spoken).output() {
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    let really_ok =
                        o.status.success() && !stderr.contains("daemon not running");
                    if really_ok {
                        return;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    eprintln!("[tts] spawn {} failed: {e}", bin);
                    return;
                }
            }
        }
        eprintln!("[tts] NO TTS BINARY FOUND; tried {:?}", tried);
    });
}

/// True when `text` looks like a clean person-name summary:
/// short, no separators, no action verbs, all words capitalised,
/// and no generic notification vocabulary.
fn looks_like_clean_name(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 40 {
        return false;
    }
    if trimmed.contains(':')
        || trimmed.contains(" - ")
        || trimmed.contains('/')
        || trimmed.contains('(')
        || trimmed.contains(')')
    {
        return false;
    }
    if looks_like_action(trimmed) {
        return false;
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.is_empty() || words.len() > 4 {
        return false;
    }
    const GENERIC: &[&str] = &[
        "new",
        "message",
        "messages",
        "voice",
        "activity",
        "notification",
        "notifications",
        "alert",
        "alerts",
        "system",
        "test",
        "app",
        "you",
        "have",
        "a",
        "an",
        "the",
        "this",
        "that",
        "incoming",
        "audio",
        "video",
        "call",
        "missed",
        "recent",
        "update",
        "updates",
        "reminder",
        "warning",
        "info",
        "success",
        "error",
        "received",
        "unread",
        "from",
        "to",
        "in",
        "on",
        "at",
        "by",
        "here",
        "there",
    ];
    words.iter().all(|w| {
        let starts_upper = w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let is_generic = GENERIC.iter().any(|g| g.eq_ignore_ascii_case(w));
        starts_upper && !is_generic
    })
}

fn looks_like_action(text: &str) -> bool {
    const VERBS: &[&str] = &[
        "spoke",
        "speaks",
        "said",
        "says",
        "sent",
        "sends",
        "started",
        "starts",
        "called",
        "calls",
        "joined",
        "joins",
        "left",
        "leaves",
        "replied",
        "replies",
        "mentioned",
        "mentions",
        "pinged",
        "pings",
        "posted",
        "posts",
        "reacted",
        "reacts",
        "uploaded",
        "uploads",
        "shared",
        "shares",
        "playing",
        "paused",
        "changed",
        "updated",
    ];
    VERBS
        .iter()
        .any(|v| text.split_whitespace().any(|w| w.eq_ignore_ascii_case(v)))
}

/// Extract a leading name from "Alice spoke in room" -> "Alice".
/// Rejects generic sentence prefixes ("New", "Voice", "Audio", …).
fn extract_leading_name(text: &str) -> Option<String> {
    const GENERIC_PREFIX: &[&str] = &[
        "new",
        "a",
        "an",
        "the",
        "this",
        "that",
        "voice",
        "audio",
        "video",
        "incoming",
        "notification",
        "system",
        "app",
        "activity",
        "alert",
    ];
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    if GENERIC_PREFIX
        .iter()
        .any(|p| words[0].eq_ignore_ascii_case(p))
    {
        return None;
    }
    let is_name_word = |w: &str| -> bool {
        !w.is_empty()
            && w.chars()
                .all(|c| c.is_alphabetic() || c == '-' || c == '_' || c == '.')
            && w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
    };
    let mut name_parts = Vec::new();
    for w in words.iter().take(3) {
        if is_name_word(w) {
            name_parts.push(*w);
        } else {
            break;
        }
    }
    if name_parts.is_empty() {
        None
    } else {
        Some(name_parts.join(" "))
    }
}

/// Try to find a person name inside a summary that is NOT of the
/// form "… from Name". Handles common Electron/Horizon shapes:
///   "Genesis Frost: New Message"  -> "Genesis Frost"
///   "Genesis Frost - New Message" -> "Genesis Frost"
///   "Genesis Frost spoke in…"     -> "Genesis Frost"
fn extract_name_from_summary(text: &str) -> Option<String> {
    const MAX_NAME_CHARS: usize = 40;
    // 1) "Name: rest" — split on the first colon
    if let Some(idx) = text.find(':') {
        let candidate = text[..idx].trim();
        if !candidate.is_empty()
            && candidate.chars().count() <= MAX_NAME_CHARS
            && candidate.chars().any(|c| c.is_alphabetic())
        {
            return Some(candidate.to_string());
        }
    }
    // 2) "Name - rest" — split on " - "
    if let Some(idx) = text.find(" - ") {
        let candidate = text[..idx].trim();
        if !candidate.is_empty() && candidate.chars().count() <= MAX_NAME_CHARS {
            return Some(candidate.to_string());
        }
    }
    // 3) "Name <verb> rest" — leading words before the first action verb
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if looks_like_action(w) && i > 0 {
            let candidate = words[..i].join(" ");
            if candidate.chars().count() <= MAX_NAME_CHARS {
                return Some(candidate);
            }
            break;
        }
    }
    None
}

fn extract_from_phrase(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let idx = lower.find(" from ")?;
    let after = text[idx + 6..].trim();
    if after.is_empty() {
        return None;
    }
    let name: String = after
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = name.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
