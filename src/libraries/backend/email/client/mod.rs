//! Email client: config, IMAP/SMTP, MIME parsing, secure filesystem.
//!
//! Extracted submodules:
//!   types.rs       — NativeEnvelope, ParsedEmail
//!   validation.rs  — is_safe_imap_uid, is_safe_imap_folder

mod secure_fs;
mod types;
mod validation;

use secure_fs::*;
pub use types::*;
pub use validation::*;

use async_imap::types::Fetch as AsyncFetch;
use futures_util::TryStreamExt;
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use mail_parser::MimeHeaders;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use tokio::net::TcpStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(
        rename = "moonbeauty_email_address_1",
        alias = "email",
        alias = "address",
        alias = "user",
        alias = "username",
        alias = "gmail_address_himalaya"
    )]
    pub email: String,
    #[serde(
        rename = "moonbeauty_email_apppass_1",
        alias = "password",
        alias = "pass",
        alias = "code",
        alias = "app_password",
        alias = "gmail_code_himalaya"
    )]
    pub password: Option<String>,
    #[serde(rename = "moonbeauty_password_cmd_1", alias = "password_cmd", default)]
    pub password_cmd: Option<String>,
    #[serde(
        rename = "moonbeauty_imap_host_1",
        alias = "imap_host",
        alias = "imap",
        default = "default_imap"
    )]
    pub imap_host: String,
    #[serde(
        rename = "moonbeauty_imap_port_1",
        alias = "imap_port",
        default = "default_imap_port"
    )]
    pub imap_port: u16,
    #[serde(
        rename = "moonbeauty_smtp_host_1",
        alias = "smtp_host",
        alias = "smtp",
        default = "default_smtp"
    )]
    pub smtp_host: String,
    #[serde(
        rename = "moonbeauty_smtp_port_1",
        alias = "smtp_port",
        default = "default_smtp_port"
    )]
    pub smtp_port: u16,
    #[serde(rename = "moonbeauty_sops_key_1", alias = "sops_key", default)]
    pub sops_key: Option<String>,
}

fn default_imap() -> String {
    "imap.gmail.com".into()
}
fn default_imap_port() -> u16 {
    993
}
fn default_smtp() -> String {
    "smtp.gmail.com".into()
}
fn default_smtp_port() -> u16 {
    465
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            email: "user@gmail.com".into(),
            password: None,
            password_cmd: Some("echo 'app-password-here'".into()),
            imap_host: default_imap(),
            imap_port: default_imap_port(),
            smtp_host: default_smtp(),
            smtp_port: default_smtp_port(),
            sops_key: None,
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("moonbeauty").join("email.json")
}

pub fn resolve_password(cfg: &EmailConfig) -> String {
    let mut pass = String::new();
    if let Some(cmd) = &cfg.password_cmd {
        if let Ok(out) = std::process::Command::new("sh").args(["-c", cmd]).output() {
            pass = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    if pass.is_empty() {
        pass = cfg.password.clone().unwrap_or_default();
    }
    pass.replace(" ", "").trim().to_string()
}

async fn connect_and_login(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
) -> Result<async_imap::Session<async_native_tls::TlsStream<TcpStream>>, String> {
    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| format!("TCP connect to {host}:{port}: {e}"))?;
    let tls = async_native_tls::TlsConnector::new();
    let tls_stream = tls
        .connect(host, tcp)
        .await
        .map_err(|e| format!("TLS handshake with {host}: {e}"))?;
    let mut client = async_imap::Client::new(tls_stream);
    match client.read_response().await {
        Some(Ok(_)) => {}
        Some(Err(e)) => return Err(format!("IMAP greeting: {e}")),
        None => return Err("IMAP server closed before greeting".into()),
    }
    client
        .login(user, pass)
        .await
        .map_err(|(e, _client)| e.to_string())
}

fn find_sops() -> String {
    use std::path::Path;
    for p in [
        "/run/current-system/sw/bin/sops",
        "/usr/bin/sops",
        "/usr/local/bin/sops",
    ] {
        if Path::new(p).exists() {
            return p.into();
        }
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".nix-profile/bin/sops");
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }
    "sops".into()
}

pub fn save_config(cfg: &EmailConfig, use_sops: bool) -> Result<(), String> {
    let path = config_path();
    let enc_path = path.with_extension("enc.json");
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;

    if use_sops {
        // Write plaintext temp config in tmpfs with an unpredictable name before sops encrypts it.
        let tmp = crate::libraries::process::mbs_unique_tmp("mbs-email-plain", "json");
        crate::libraries::process::private_write_new(&tmp, json.as_bytes())
            .map_err(|e| e.to_string())?;

        let mut cmd = std::process::Command::new(find_sops());
        cmd.arg("--encrypt");

        let mut key_to_use = cfg.sops_key.clone();
        if key_to_use.is_none() || key_to_use.as_deref() == Some("") {
            if let Some(home) = dirs::home_dir() {
                let age_file = home.join(".config/sops/age/keys.txt");
                if let Ok(content) = std::fs::read_to_string(age_file) {
                    for line in content.lines() {
                        if line.trim().starts_with("# public key:") {
                            key_to_use =
                                Some(line.trim().replace("# public key:", "").trim().to_string());
                            break;
                        }
                    }
                }
            }
        }

        if let Some(key) = key_to_use {
            let k = key.trim();
            if k.starts_with("age1") {
                cmd.arg("--age").arg(k);
            } else if k.contains("BEGIN PGP") {
                cmd.arg("--pgp").arg(k);
            } else if k.ends_with(".txt") || k.contains("/") {
                let mut paths_to_try = vec![std::path::PathBuf::from(k)];
                if let Some(home) = dirs::home_dir() {
                    paths_to_try.push(home.join(k));
                    paths_to_try.push(home.join(".config/sops/age").join(k));
                }
                let mut found_key = false;
                for p in paths_to_try {
                    if p.exists() {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            for line in content.lines() {
                                if line.trim().starts_with("# public key:") {
                                    cmd.arg("--age")
                                        .arg(line.trim().replace("# public key:", "").trim());
                                    found_key = true;
                                    break;
                                }
                            }
                            if !found_key && content.contains("AGE-SECRET-KEY-1") {
                                if let Ok(out) = std::process::Command::new("age-keygen")
                                    .args(["-y", p.to_str().unwrap()])
                                    .output()
                                {
                                    if out.status.success() {
                                        let pk =
                                            String::from_utf8_lossy(&out.stdout).trim().to_string();
                                        if pk.starts_with("age1") {
                                            cmd.arg("--age").arg(&pk);
                                            found_key = true;
                                        }
                                    }
                                }
                            }
                            if !found_key {
                                for word in content.split_whitespace() {
                                    if word.starts_with("age1") {
                                        cmd.arg("--age").arg(word);
                                        found_key = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if found_key {
                        break;
                    }
                }
                if !found_key {
                    cmd.arg("--pgp").arg(k);
                }
            } else {
                cmd.arg("--pgp").arg(k);
            }
        }

        cmd.arg(tmp.to_str().unwrap());
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to run sops: {}", e))?;

        if out.status.success() {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&enc_path)
                .map_err(|e| e.to_string())?;
            // Force 0600 on the final encrypted file.
            let _ = chmod_private(&enc_path, false);
            f.write_all(&out.stdout).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&tmp);
            if let Some(key) = &cfg.sops_key {
                if key.ends_with(".txt") || key.contains("/") {
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(enc_path.with_extension("keypath"))
                    {
                        let _ = f.write_all(key.as_bytes());
                    }
                }
            }
            Ok(())
        } else {
            let _ = std::fs::remove_file(&tmp);
            Err(format!(
                "SOPS error: {}",
                String::from_utf8_lossy(&out.stderr)
            ))
        }
    } else {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| e.to_string())?;
        // Force 0600 on existing files (OpenOptions::mode() only applies on creation).
        let _ = chmod_private(&path, false);
        f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn config_exists() -> bool {
    let path = config_path();
    let enc_path = path.with_extension("enc.json");
    path.exists() || enc_path.exists()
}

pub fn load_config() -> EmailConfig {
    ensure_secure_cache_tree();
    let path = config_path();
    let enc_path = path.with_extension("enc.json");

    let content = if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else if enc_path.exists() {
        let mut cmd = std::process::Command::new(find_sops());
        let keypath_file = enc_path.with_extension("keypath");
        if let Ok(keypath) = std::fs::read_to_string(&keypath_file) {
            let k = keypath.trim();
            let mut final_path = k.to_string();
            if !std::path::Path::new(k).is_absolute() {
                if let Some(home) = dirs::home_dir() {
                    if home.join(k).exists() {
                        final_path = home.join(k).to_string_lossy().to_string();
                    } else if home.join(".config/sops/age").join(k).exists() {
                        final_path = home
                            .join(".config/sops/age")
                            .join(k)
                            .to_string_lossy()
                            .to_string();
                    }
                }
            }
            cmd.env("SOPS_AGE_KEY_FILE", final_path);
        }
        cmd.args(["--decrypt", enc_path.to_str().unwrap()]);
        let out = cmd.output().ok();
        out.and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                None
            }
        })
    } else {
        None
    };

    if let Some(json) = content {
        if let Ok(cfg) = serde_json::from_str::<EmailConfig>(&json) {
            return cfg;
        }
    }

    let def = EmailConfig::default();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&def) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
        {
            // Force 0600 on existing files.
            let _ = chmod_private(&path, false);
            let _ = f.write_all(json.as_bytes());
        }
    }
    def
}

// ── Secure filesystem helpers ────────────────────────────────────

fn get_cache_dir(folder: &str) -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let root = base.join("moonbeauty/email_cache");
    get_cache_dir_with_root(&root, folder)
}

/// Testable variant: takes the cache root explicitly so unit tests can
/// assert the escape-prevention property without mutating XDG_DATA_HOME.
fn get_cache_dir_with_root(root: &std::path::Path, folder: &str) -> PathBuf {
    let mapped = map_gmail_folder(folder);
    let components: Vec<String> = mapped
        .split('/')
        .filter(|c| !c.is_empty() && !c.starts_with('.'))
        .map(crate::libraries::process::sanitize_folder_name)
        .collect();
    if components.is_empty() {
        return root.join("__invalid__");
    }
    let path: PathBuf = components.iter().map(String::as_str).collect();
    let path = root.join(&path);
    // Defense-in-depth: refuse any path that escapes root.
    if !path.starts_with(root) || path == root {
        eprintln!(
            "[email] SECURITY: refusing unsafe cache path {:?} (root {:?})",
            path, root
        );
        return root.join("__invalid__");
    }
    path
}

pub fn load_index(folder: &str) -> Vec<NativeEnvelope> {
    let path = get_cache_dir(folder).join("index.json");
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(mut envs) = serde_json::from_str::<Vec<NativeEnvelope>>(&content) {
            // Reject non-numeric envelope ids before they reach IMAP uid_* commands.
            let before = envs.len();
            envs.retain(|e| is_safe_imap_uid(&e.id));
            if envs.len() < before {
                eprintln!(
                    "[email] dropped {} unsafe envelope id(s) from {}",
                    before - envs.len(),
                    folder
                );
            }
            return envs;
        }
    }
    Vec::new()
}

pub fn save_index(folder: &str, envs: &[NativeEnvelope]) {
    let dir = get_cache_dir(folder);
    let path = dir.join("index.json");
    if let Ok(content) = serde_json::to_string_pretty(envs) {
        let _ = secure_write(&path, content.as_bytes());
    }
}

pub async fn save_draft(to: &str, subject: &str, body: &str) -> Result<(), String> {
    save_draft_sync(to, subject, body)
}

pub fn drafts_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("moonbeauty/drafts.txt")
}

pub fn save_draft_sync(to: &str, subject: &str, body: &str) -> Result<(), String> {
    let path = drafts_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    secure_append(
        &path,
        format!("To: {}\nSubject: {}\n{}\n---\n", to, subject, body).as_bytes(),
    ).map_err(|e| e.to_string())?;
    println!("[draft] saved successfully");
    Ok(())
}

fn split_draft_chunks(content: &str) -> Vec<String> {
    content
        .split("\n---\n")
        .map(|s| s.strip_suffix("\n---").unwrap_or(s).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_draft_chunk(chunk: &str) -> (String, String, String) {
    let mut to = String::new();
    let mut subject = String::new();
    let mut body_lines = Vec::new();
    let mut in_body = false;
    for line in chunk.lines() {
        if !in_body && line.starts_with("To: ") {
            to = line[4..].to_string();
        } else if !in_body && line.starts_with("Subject: ") {
            subject = line[9..].to_string();
            in_body = true;
        } else if in_body {
            body_lines.push(line);
        }
    }
    (to, subject, body_lines.join("\n").trim().to_string())
}

fn load_drafts_from_str(content: &str) -> Vec<NativeEnvelope> {
    let mut envs: Vec<NativeEnvelope> = split_draft_chunks(content)
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let (to, subject, _) = parse_draft_chunk(&chunk);
            NativeEnvelope {
                id: format!("draft_{}", idx),
                subject,
                sender: to.clone(),
                sender_email: to,
                is_starred: false,
                is_read: true,
            }
        })
        .collect();
    envs.reverse();
    envs
}

fn decode_basic_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_basic_html_entities(&out)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_safe_http_url(url: &str) -> bool {
    let u = url.trim();
    if u.len() > 2048 || u.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lower = u.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

pub fn extract_links(html: Option<&str>, text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    const MAX_LINKS: usize = 50;

    let mut push_link = |label: String, raw_url: String| {
        if out.len() >= MAX_LINKS {
            return;
        }
        let url = decode_basic_html_entities(raw_url.trim());
        if !is_safe_http_url(&url) {
            return;
        }
        if seen.insert(url.clone()) {
            let clean_label = if label.is_empty() || label == url {
                String::new()
            } else {
                label.chars().take(80).collect()
            };
            out.push((clean_label, url));
        }
    };

    if let Some(h) = html {
        let lower = h.to_ascii_lowercase();
        let mut pos = 0usize;
        while let Some(rel_start) = lower[pos..].find("<a") {
            let tag_start = pos + rel_start;
            let after_a = tag_start + 2;
            let next_ch = lower[after_a..].chars().next();
            if !matches!(next_ch, Some(c) if c.is_whitespace() || c == '>') {
                pos = after_a;
                continue;
            }
            let Some(rel_end) = lower[after_a..].find('>') else {
                break;
            };
            let tag_end = after_a + rel_end;
            let attrs = &h[after_a..tag_end];
            let attrs_lower = &lower[after_a..tag_end];

            let mut href_val: Option<String> = None;
            let mut search_at = 0usize;
            while let Some(h_rel) = attrs_lower[search_at..].find("href") {
                let h_idx = search_at + h_rel;
                let prev_ok = h_idx == 0
                    || attrs_lower[..h_idx]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_whitespace());
                let rest = &attrs[h_idx + 4..];
                let rest_trimmed = rest.trim_start();
                if prev_ok && rest_trimmed.starts_with('=') {
                    let after_eq = rest_trimmed[1..].trim_start();
                    if let Some(quoted) = after_eq.strip_prefix('"') {
                        if let Some(q_end) = quoted.find('"') {
                            href_val = Some(quoted[..q_end].to_string());
                        }
                    } else if let Some(quoted) = after_eq.strip_prefix('\'') {
                        if let Some(q_end) = quoted.find('\'') {
                            href_val = Some(quoted[..q_end].to_string());
                        }
                    } else {
                        let end = after_eq
                            .find(|c: char| c.is_whitespace() || c == '>')
                            .unwrap_or(after_eq.len());
                        href_val = Some(after_eq[..end].to_string());
                    }
                    break;
                }
                search_at = h_idx + 4;
            }

            let close_start = lower[tag_end + 1..]
                .find("</a>")
                .map(|r| tag_end + 1 + r);
            if let Some(url) = href_val {
                let label = if let Some(cs) = close_start {
                    strip_html_tags(&h[tag_end + 1..cs])
                } else {
                    String::new()
                };
                push_link(label, url);
            }
            pos = close_start.map(|cs| cs + 4).unwrap_or(tag_end + 1);
        }
    }

    for token in text.split_whitespace() {
        let trimmed = token
            .trim_start_matches(['<', '(', '[', '"', '\''])
            .trim_end_matches(['>', ')', ']', '"', '\'', '.', ',', ';']);
        let low = trimmed.to_ascii_lowercase();
        if low.starts_with("https://") || low.starts_with("http://") {
            push_link(String::new(), trimmed.to_string());
        }
    }

    out
}

fn read_draft_from_str(content: &str, id: &str) -> Option<ParsedEmail> {
    let num = id.strip_prefix("draft_")?.parse::<usize>().ok()?;
    let chunks = split_draft_chunks(content);
    let chunk = chunks.get(num)?;
    let (_, _, text) = parse_draft_chunk(chunk);
    let links = extract_links(None, &text);
    Some(ParsedEmail {
        text,
        images: Vec::new(),
        attachments: Vec::new(),
        links,
    })
}

fn delete_draft_from_str(content: &str, id: &str) -> Result<String, String> {
    let num = id
        .strip_prefix("draft_")
        .ok_or_else(|| "invalid draft id".to_string())?
        .parse::<usize>()
        .map_err(|_| "invalid draft number".to_string())?;
    let mut chunks = split_draft_chunks(content);
    if num >= chunks.len() {
        return Err(format!(
            "draft index {} out of bounds (len: {})",
            num,
            chunks.len()
        ));
    }
    chunks.remove(num);
    let mut new_content = String::new();
    for chunk in chunks {
        new_content.push_str(&chunk);
        new_content.push_str("\n---\n");
    }
    Ok(new_content)
}

pub fn delete_draft(id: &str) -> Result<(), String> {
    let path = drafts_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Err("could not read drafts file".into());
    };
    let new_content = delete_draft_from_str(&content, id)?;
    secure_write(&path, new_content.as_bytes()).map_err(|e| e.to_string())?;
    println!("[draft] successfully deleted {}", id);
    Ok(())
}

pub fn load_drafts() -> Vec<NativeEnvelope> {
    let path = drafts_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    load_drafts_from_str(&content)
}

pub fn read_draft(id: &str) -> Option<ParsedEmail> {
    let path = drafts_path();
    let content = std::fs::read_to_string(&path).ok()?;
    read_draft_from_str(&content, id)
}

pub async fn fetch_envelopes(folder: &str) -> Result<Vec<NativeEnvelope>, String> {
    if std::env::var_os("MBS_DEBUG").is_some() {
        eprintln!("[sync] fetch_envelopes called for {}", folder);
    }
    let cached = load_index(folder);
    if !cached.is_empty() {
        if std::env::var_os("MBS_DEBUG").is_some() {
            eprintln!("[sync] loaded {} emails from cache", cached.len());
        }
        return Ok(cached);
    }
    let envs = fetch_from_network(folder).await?;
    save_index(folder, &envs);
    Ok(envs)
}

/// One-shot IMAP + SMTP diagnostic. Called by `--test-email`.
/// Returns (imap_ok, smtp_ok, human_report).
pub async fn test_connection() -> Result<(bool, bool, String), String> {
    let cfg = load_config();
    let mut r = String::new();
    r.push_str(&format!("config:  {}\n", config_path().display()));
    r.push_str(&format!("  user:  {}\n", cfg.email));
    r.push_str(&format!("  imap:  {}:{}\n", cfg.imap_host, cfg.imap_port));
    r.push_str(&format!("  smtp:  {}:{}\n", cfg.smtp_host, cfg.smtp_port));
    r.push_str(&format!("  sops key set: {}\n", cfg.sops_key.is_some()));

    let pass = resolve_password(&cfg);
    r.push_str(&format!("  password length: {}\n", pass.len()));
    if pass.is_empty() {
        r.push_str("\n⚠️  password empty — set app-password in email.json or password_cmd\n");
        return Ok((false, false, r));
    }

    // IMAP: connect + login + select
    r.push_str("\nimap:\n");
    let imap_ok = match connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await {
        Ok(mut session) => {
            r.push_str("  ✅ TCP+TLS+greeting+login\n");
            match session.select("INBOX").await {
                Ok(_) => {
                    r.push_str("  ✅ INBOX select\n");
                    let _ = session.logout().await;
                    true
                }
                Err(e) => {
                    r.push_str(&format!("  ❌ INBOX select: {}\n", e));
                    false
                }
            }
        }
        Err(e) => {
            r.push_str(&format!("  ❌ {}\n", e));
            false
        }
    };

    // SMTP: build only (no send — we don't want to spam)
    r.push_str("\nsmtp:\n");
    let smtp_ok = match lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&cfg.smtp_host)
    {
        Ok(builder) => {
            let _mailer: lettre::AsyncSmtpTransport<lettre::Tokio1Executor> = builder
                .port(cfg.smtp_port)
                .credentials(lettre::transport::smtp::authentication::Credentials::new(
                    cfg.email.clone(),
                    pass,
                ))
                .build();
            r.push_str("  ✅ builder configured (not sent)\n");
            true
        }
        Err(e) => {
            r.push_str(&format!("  ❌ builder: {}\n", e));
            false
        }
    };

    Ok((imap_ok, smtp_ok, r))
}

pub async fn fetch_from_network(folder: &str) -> Result<Vec<NativeEnvelope>, String> {
    if std::env::var_os("MBS_DEBUG").is_some() {
        eprintln!("[sync] fetch_from_network called for {}", folder);
    }
    // Validate folder name before reaching IMAP SELECT.
    if !is_safe_imap_folder(folder) {
        return Err(format!("unsafe folder name: {:?}", folder));
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    session
        .select(map_gmail_folder(folder))
        .await
        .map_err(|e| e.to_string())?;
    let fetches: Vec<AsyncFetch> = session
        .uid_fetch(
            "1:*",
            "(FLAGS BODY.PEEK[HEADER.FIELDS (SUBJECT FROM DATE)])",
        )
        .await
        .map_err(|e| e.to_string())?
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;

    let mut envs = Vec::new();
    for fetch in fetches.iter().rev() {
        let is_starred = fetch
            .flags()
            .any(|f| matches!(f, async_imap::types::Flag::Flagged));
        if let Some(header) = fetch.header() {
            if let Some(parsed) = mail_parser::MessageParser::default().parse(header) {
                let subj = parsed.subject().unwrap_or("(No Subject)").to_string();
                let from_obj = parsed.from().and_then(|f| match f {
                    mail_parser::Address::List(list) => list.iter().next(),
                    mail_parser::Address::Group(groups) => {
                        groups.first().and_then(|g| g.addresses.first())
                    }
                });
                let from_name = from_obj
                    .and_then(|a| a.name())
                    .unwrap_or_default()
                    .to_string();
                let from_email = from_obj.and_then(|a| a.address()).unwrap_or("").to_string();
                let uid_str = fetch
                    .uid
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| fetch.message.to_string());
                let is_read = fetch
                    .flags()
                    .any(|f| matches!(f, async_imap::types::Flag::Seen));
                let sender_display = if !from_name.trim().is_empty() {
                    from_name
                } else if !from_email.trim().is_empty() {
                    from_email.clone()
                } else {
                    "Unknown".to_string()
                };
                envs.push(NativeEnvelope {
                    id: uid_str,
                    subject: subj,
                    sender: sender_display,
                    sender_email: from_email,
                    is_starred,
                    is_read,
                });
            }
        }
    }
    let _ = session.logout().await;
    if folder == "Unread" {
        envs.retain(|e| !e.is_read);
    }
    Ok(envs)
}

async fn fetch_raw_message(id: &str, folder: &str) -> Result<Vec<u8>, String> {
    // Numeric-uid and folder gates before IMAP commands.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    if !is_safe_imap_folder(folder) {
        return Err(format!("unsafe folder name: {:?}", folder));
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    session
        .select(map_gmail_folder(folder))
        .await
        .map_err(|e| e.to_string())?;
    let fetches: Vec<AsyncFetch> = session
        .uid_fetch(id, "(BODY.PEEK[])")
        .await
        .map_err(|e| e.to_string())?
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(fetch) = fetches.first() {
        if let Some(body) = fetch.body() {
            return Ok(body.to_vec());
        }
    }
    Err("No body".into())
}

pub async fn sync_bodies_background(folder: String) {
    let lock_path = crate::libraries::process::mbs_runtime_dir().join("mbs_email_sync.lock");
    let _lock_file = {
        use std::os::unix::fs::OpenOptionsExt as _;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&lock_path)
        {
            Ok(f) => f,
            Err(_) => {
                if std::env::var_os("MBS_DEBUG").is_some() {
                    eprintln!(
                        "[sync] another process is already syncing, skipping duplicate spawn."
                    );
                }
                return;
            }
        }
    };

    struct LockGuard(std::path::PathBuf);
    impl Drop for LockGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _guard = LockGuard(lock_path);

    let envs = load_index(&folder);
    if envs.is_empty() {
        return;
    }
    if std::env::var_os("MBS_DEBUG").is_some() {
        eprintln!(
            "[sync] starting background sync for {} emails in {}",
            envs.len(),
            folder
        );
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session =
        match connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[sync] connect failed: {e}");
                return;
            }
        };
    let imap_folder = map_gmail_folder(&folder);
    // Validate folder name before SELECT.
    if !is_safe_imap_folder(&imap_folder) {
        eprintln!("[sync] refusing unsafe folder: {:?}", imap_folder);
        return;
    }
    if let Err(e) = session.select(&imap_folder).await {
        eprintln!("[sync] select {} failed: {e}", imap_folder);
        return;
    }

    let mut count = 0;
    let total = envs.len();
    for env in envs {
        let cache_file = get_cache_dir(&folder).join(format!("{}.eml", env.id));
        if !cache_file.exists() {
            tokio::time::sleep(std::time::Duration::from_secs(12)).await;
            // Numeric-uid gate before uid_fetch.
            if !is_safe_imap_uid(&env.id) {
                eprintln!("[sync] refusing unsafe uid: {:?}", env.id);
                continue;
            }
            match session.uid_fetch(&env.id, "(BODY.PEEK[])").await {
                Ok(stream) => match stream.try_collect::<Vec<_>>().await {
                    Ok(fetches) => {
                        if let Some(fetch) = fetches.first() {
                            if let Some(body) = fetch.body() {
                                // Cap cached raw email body at 100 MB.
                                const MAX_BODY: usize = 100 * 1024 * 1024;
                                if body.len() > MAX_BODY {
                                    eprintln!(
                                        "[sync] skipping oversize body {} ({} bytes)",
                                        env.id,
                                        body.len()
                                    );
                                    continue;
                                }
                                let _ = secure_write_async(&cache_file, body).await;
                                count += 1;
                                if std::env::var_os("MBS_DEBUG").is_some() {
                                    eprintln!("[sync] downloaded {}/{}: {}", count, total, env.id);
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("[sync] fetch {} failed: {e}", env.id),
                },
                Err(e) => eprintln!("[sync] uid_fetch {} failed: {e}", env.id),
            }
        }
    }
    let _ = session.logout().await;
    if std::env::var_os("MBS_DEBUG").is_some() {
        eprintln!("[sync] background sync finished for {}", folder);
    }
}

pub async fn read_message(id: &str, folder: &str) -> ParsedEmail {
    if id.starts_with("draft_") {
        if let Some(draft) = read_draft(id) {
            return draft;
        }
    }

    // Cap raw MIME size at 100 MB before parsing to bound mail_parser allocations.
    const MAX_RAW: u64 = 100 * 1024 * 1024;
    let cache_file = get_cache_dir(folder).join(format!("{}.eml", id));
    let bytes =
        if cache_file.exists() && cache_file.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            // Pre-check cached size via metadata before allocating.
            if cache_file.metadata().map(|m| m.len()).unwrap_or(0) > MAX_RAW {
                return ParsedEmail {
                    text: format!(
                        "Message too large to display (cached size exceeds {} MB)",
                        MAX_RAW / (1024 * 1024)
                    ),
                    images: Vec::new(),
                    attachments: Vec::new(),
                    links: Vec::new(),
                };
            }
            std::fs::read(&cache_file).unwrap_or_default()
        } else {
            match fetch_raw_message(id, folder).await {
                Ok(b) => {
                    if b.len() as u64 > MAX_RAW {
                        return ParsedEmail {
                            text: format!(
                                "Message too large to display ({} MB exceeds {} MB cap)",
                                b.len() / (1024 * 1024),
                                MAX_RAW / (1024 * 1024)
                            ),
                            images: Vec::new(),
                            attachments: Vec::new(),
                            links: Vec::new(),
                        };
                    }
                    let _ = secure_write(&cache_file, &b);
                    b
                }
                Err(e) => {
                    return ParsedEmail {
                        text: format!("Failed to fetch message: {}", e),
                        images: Vec::new(),
                        attachments: Vec::new(),
                        links: Vec::new(),
                    }
                }
            }
        };

    let mut parsed_email = ParsedEmail {
        text: String::new(),
        images: Vec::new(),
        attachments: Vec::new(),
        links: Vec::new(),
    };

    if let Some(parsed) = mail_parser::MessageParser::default().parse(&bytes) {
        // 1. Extract Text & HTML (always scan HTML for links even when plain text exists)
        let html_body = parsed.body_html(0).map(|h| h.to_string());
        if let Some(t) = parsed.body_text(0) {
            parsed_email.text = t.to_string();
        }

        if parsed_email.text.trim().is_empty() {
            if let Some(ref h_str) = html_body {
                // Use a very wide wrap so html2text does not hard-wrap at 80.
                // The `text` widget handles actual pixel-width wrapping at
                // render time; doing both produces mid-word breaks in URLs.
                let unwrapped = html2text::from_read(h_str.as_bytes(), 1_000_000);
                // Clean up: strip angle brackets around URLs, collapse excess
                // blank lines, trim trailing whitespace per line.
                let mut cleaned = String::with_capacity(unwrapped.len());
                let mut prev_blank = false;
                for raw_line in unwrapped.lines() {
                    let line = raw_line.trim_end();
                    let stripped = if line.starts_with('<')
                        && line.ends_with('>')
                        && line.len() > 4
                        && (line[1..].starts_with("http://") || line[1..].starts_with("https://"))
                    {
                        &line[1..line.len() - 1]
                    } else {
                        line
                    };
                    if stripped.is_empty() {
                        if !prev_blank {
                            cleaned.push('\n');
                            prev_blank = true;
                        }
                    } else {
                        cleaned.push_str(stripped);
                        cleaned.push('\n');
                        prev_blank = false;
                    }
                }
                parsed_email.text = cleaned.trim_end().to_string();
            }
        }

        if parsed_email.text.trim().is_empty() {
            parsed_email.text = "This email has no readable text.".into();
        }

        let img_dir = crate::libraries::process::mbs_subdir("mbs-email-images");

        // 2. Attachments (all non-text parts with a filename are attachments)
        for attachment in parsed.attachments() {
            let name = attachment
                .attachment_name()
                .unwrap_or("unknown")
                .to_string();
            let data = attachment.contents().to_vec();
            eprintln!("[email-debug] attachment: {} ({} bytes)", name, data.len());
            // Reject attachments over 50 MB.
            const MAX_ATTACH: usize = 50 * 1024 * 1024;
            if data.len() > MAX_ATTACH {
                eprintln!(
                    "[email] skipping oversize attachment: {} ({} bytes)",
                    name,
                    data.len()
                );
                continue;
            }
            parsed_email.attachments.push((name, data));
        }

        // 3. Inline Images
        for (i, part) in parsed.parts.iter().enumerate() {
            if part.is_text_html() || part.is_text() {
                continue;
            }
            let ctype = part.content_type().map(|c| c.ctype()).unwrap_or("");
            if ctype == "image" {
                let subtype = part
                    .content_type()
                    .and_then(|c| c.subtype())
                    .unwrap_or("png");
                let default_name = format!("image_{}.{}", i, subtype);
                let raw_name = part.attachment_name().unwrap_or(&default_name);
                // Attacker-controlled from Content-Disposition. Keep only the
                // basename to block `../../foo` path traversal.
                let safe_name: String = std::path::Path::new(raw_name)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.trim_start_matches('.').to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| default_name.clone());
                // Reject inline images over 10 MB.
                const MAX_IMG: usize = 10 * 1024 * 1024;
                if part.contents().len() > MAX_IMG {
                    eprintln!(
                        "[email] skipping oversize inline image: {} ({} bytes)",
                        safe_name,
                        part.contents().len()
                    );
                    continue;
                }
                let img_path = img_dir.join(format!("{}_{}", id, safe_name));
                let _ = std::fs::write(&img_path, part.contents());
                parsed_email.images.push(img_path);
                eprintln!(
                    "[email-debug] saved image: {} ({} bytes)",
                    safe_name,
                    part.contents().len()
                );
            }
        }

        // Cap rendered body text at 1 MB to keep Iced text layout responsive.
        const MAX_TEXT_BYTES: usize = 1_000_000;
        if parsed_email.text.len() > MAX_TEXT_BYTES {
            let mut cut = MAX_TEXT_BYTES;
            while cut > 0 && !parsed_email.text.is_char_boundary(cut) {
                cut -= 1;
            }
            parsed_email.text.truncate(cut);
            parsed_email.text.push_str("\n\n[... body truncated at 1 MB ...]");
        }
        parsed_email.links = extract_links(html_body.as_deref(), &parsed_email.text);
        return parsed_email;
    }
    parsed_email.text = "Failed to parse message.".into();
    parsed_email
}

pub async fn send_message(
    to: &str,
    subject: &str,
    body: &str,
    attachments: Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    use lettre::message::{header::ContentDisposition, MultiPart, SinglePart};

    fn mime_for(name: &str) -> lettre::message::header::ContentType {
        let lower = name.to_lowercase();
        let mime = if lower.ends_with(".png") {
            "image/png"
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            "image/jpeg"
        } else if lower.ends_with(".gif") {
            "image/gif"
        } else if lower.ends_with(".webp") {
            "image/webp"
        } else if lower.ends_with(".pdf") {
            "application/pdf"
        } else if lower.ends_with(".txt") {
            "text/plain; charset=utf-8"
        } else if lower.ends_with(".zip") {
            "application/zip"
        } else if lower.ends_with(".mp3") {
            "audio/mpeg"
        } else if lower.ends_with(".mp4") {
            "video/mp4"
        } else {
            "application/octet-stream"
        };
        lettre::message::header::ContentType::parse(mime).unwrap_or_else(|_| {
            lettre::message::header::ContentType::parse("application/octet-stream").unwrap()
        })
    }

    let cfg = load_config();
    let from_addr: Mailbox = cfg
        .email
        .parse()
        .map_err(|_| "Invalid from address".to_string())?;
    let to_addr: Mailbox = to.parse().map_err(|_| "Invalid to address".to_string())?;

    eprintln!("[send] building message: {} attachments", attachments.len());
    for (name, data) in &attachments {
        eprintln!("[send]   attachment: {} ({} bytes)", name, data.len());
    }

    let email = if attachments.is_empty() {
        Message::builder()
            .from(from_addr)
            .to(to_addr)
            .subject(subject)
            .header(lettre::message::header::ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .map_err(|e| e.to_string())?
    } else {
        // First .singlepart() on MultiPartBuilder consumes it and returns MultiPart.
        // Subsequent .singlepart() calls on MultiPart return MultiPart (mut self).
        let mut multipart: MultiPart = MultiPart::mixed().singlepart(
            SinglePart::builder()
                .header(lettre::message::header::ContentType::TEXT_PLAIN)
                .body(body.to_string()),
        );

        for (name, data) in attachments {
            let content_type = mime_for(&name);
            eprintln!("[send] attaching {} as {:?}", name, content_type);
            let part = SinglePart::builder()
                .header(content_type)
                .header(ContentDisposition::attachment(&name))
                .body(data);
            multipart = multipart.singlepart(part);
        }

        let built = Message::builder()
            .from(from_addr)
            .to(to_addr)
            .subject(subject)
            .multipart(multipart)
            .map_err(|e| format!("Build failed: {}", e))?;

        // Log the raw MIME structure so we can verify attachments are inside
        if let Ok(raw) = String::from_utf8(built.formatted()) {
            let has_attachment = raw.contains("Content-Disposition: attachment");
            let content_type_count = raw.matches("Content-Type:").count();
            eprintln!(
                "[send] MIME check: attachment_header={} content_types={} total_bytes={}",
                has_attachment,
                content_type_count,
                raw.len()
            );
            // Print header section only — the body starts after the first blank line
            // so body content isn't leaked into mbs-standalone-*.log.
            eprintln!("[send] ─── MIME HEADERS ───");
            for line in raw.lines().take(40) {
                if line.is_empty() {
                    eprintln!("[send] ... (body omitted)");
                    break;
                }
                eprintln!("[send] {}", line);
            }
            eprintln!("[send] ─── MIME END ───");
        }

        built
    };

    let pass = resolve_password(&cfg);
    let creds = Credentials::new(cfg.email.clone(), pass);
    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.smtp_host)
            .map_err(|e| e.to_string())?
            .port(cfg.smtp_port)
            .credentials(creds)
            .build();
    mailer
        .send(email)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn toggle_star(id: &str, folder: &str, star: bool) -> Result<(), String> {
    // Numeric-uid and folder gates before IMAP commands.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    if !is_safe_imap_folder(folder) {
        return Err(format!("unsafe folder name: {:?}", folder));
    }
    let mut envs = load_index(folder);
    if let Some(env) = envs.iter_mut().find(|e| e.id == id) {
        env.is_starred = star;
        save_index(folder, &envs);
    }

    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    session
        .select(map_gmail_folder(folder))
        .await
        .map_err(|e| e.to_string())?;
    let op = if star {
        "+FLAGS (\\Flagged)"
    } else {
        "-FLAGS (\\Flagged)"
    };
    let _: Vec<_> = session
        .uid_store(id, op)
        .await
        .map_err(|e| e.to_string())?
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;
    let _ = session.logout().await;
    Ok(())
}

async fn imap_move_message(
    id: &str,
    src_imap_folder: &str,
    dest_imap_folder: &str,
) -> Result<(), String> {
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    if !is_safe_imap_folder(src_imap_folder) {
        return Err(format!("unsafe folder name: {:?}", src_imap_folder));
    }
    if !is_safe_imap_folder(dest_imap_folder) {
        return Err(format!("unsafe folder name: {:?}", dest_imap_folder));
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    session.select(src_imap_folder).await.map_err(|e| e.to_string())?;
    session.uid_copy(id, dest_imap_folder).await.map_err(|e| e.to_string())?;
    let _: Vec<_> = session
        .uid_store(id, r"+FLAGS (\Deleted)")
        .await.map_err(|e| e.to_string())?
        .try_collect().await.map_err(|e| e.to_string())?;
    let _: Vec<_> = session
        .expunge()
        .await.map_err(|e| e.to_string())?
        .try_collect().await.map_err(|e| e.to_string())?;
    let _ = session.logout().await;
    Ok(())
}

pub async fn move_to_trash(id: &str, folder: &str) -> Result<(), String> {
    if !is_safe_imap_folder(folder) {
        return Err(format!("unsafe folder name: {:?}", folder));
    }
    imap_move_message(id, &map_gmail_folder(folder), &map_gmail_folder("Trash")).await
}

pub async fn move_to_spam(id: &str, folder: &str) -> Result<(), String> {
    if !is_safe_imap_folder(folder) {
        return Err(format!("unsafe folder name: {:?}", folder));
    }
    imap_move_message(id, &map_gmail_folder(folder), &map_gmail_folder("Spam")).await
}

pub async fn mark_not_spam(id: &str) -> Result<(), String> {
    imap_move_message(id, "[Gmail]/Spam", "INBOX").await
}

pub async fn delete_message(id: &str, folder: &str) -> Result<(), String> {
    // Numeric-uid and folder gates before IMAP commands.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    if !is_safe_imap_folder(folder) {
        return Err(format!("unsafe folder name: {:?}", folder));
    }
    let mut envs = load_index(folder);
    envs.retain(|e| e.id != id);
    save_index(folder, &envs);

    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    session
        .select(map_gmail_folder(folder))
        .await
        .map_err(|e| e.to_string())?;
    let _: Vec<_> = session
        .uid_store(id, "+FLAGS (\\Deleted)")
        .await
        .map_err(|e| e.to_string())?
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;
    let _: Vec<_> = session
        .expunge()
        .await
        .map_err(|e| e.to_string())?
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;
    let _ = session.logout().await;
    Ok(())
}

pub async fn fetch_folder_list() -> Result<Vec<String>, String> {
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    let stream = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|e| e.to_string())?;
    let names: Vec<_> = stream.try_collect().await.map_err(|e| e.to_string())?;
    let mut folders = Vec::new();
    for name in names {
        let n = name.name();
        if n.is_empty() || n == "INBOX" || n.starts_with("[Gmail]") {
            continue;
        }
        // Reject unsafe folder names returned by server LIST.
        if !is_safe_imap_folder(n) {
            eprintln!("[imap] rejecting unsafe folder name from LIST: {:?}", n);
            continue;
        }
        folders.push(n.to_string());
    }
    let _ = session.logout().await;
    folders.sort();
    folders.dedup();
    Ok(folders)
}

pub fn folders_cache_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    base.join("moonbeauty/email_cache/folders.json")
}

pub fn load_folders_cache() -> Vec<String> {
    ensure_secure_cache_tree();
    let path = folders_cache_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&content) {
            return list;
        }
    }
    Vec::new()
}

pub fn save_folders_cache(folders: &[String]) {
    let path = folders_cache_path();
    if let Ok(json) = serde_json::to_string_pretty(folders) {
        let _ = secure_write(&path, json.as_bytes());
    }
}

pub fn contacts_path() -> PathBuf {
    dirs::data_dir()
        .expect("XDG_DATA_HOME or HOME must be set; refusing to place contacts.txt in cwd")
        .join("moonbeauty")
        .join("contacts.txt")
}

pub fn load_contacts() -> Vec<String> {
    let path = contacts_path();
    // One-shot migration; only accept if every non-empty line looks like an email.
    if !path.exists() {
        let legacy = PathBuf::from("resources/contacts.txt");
        if legacy.exists() {
            if let Ok(content) = std::fs::read_to_string(&legacy) {
                let valid = content
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .all(|l| l.contains('@') && l.len() < 500);
                if valid {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ =
                        crate::libraries::process::private_write_file(&path, content.as_bytes());
                }
            }
        }
    }
    if let Ok(content) = std::fs::read_to_string(&path) {
        content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

pub async fn save_contact(email: &str) -> Result<(), String> {
    let path = contacts_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let _ = chmod_private(&path, false);
    f.write_all(format!("{}\n", email).as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
}



fn map_gmail_folder(folder: &str) -> String {
    match folder {
        "Unread" => "INBOX",
        "Starred" => "[Gmail]/Starred",
        "Sent" => "[Gmail]/Sent Mail",
        "Drafts" => "[Gmail]/Drafts",
        "Trash" => "[Gmail]/Trash",
        "Spam" => "[Gmail]/Spam",
        _ => folder,
    }
    .to_string()
}

#[cfg(test)]
mod cache_dir_tests {
    use super::get_cache_dir_with_root;

    #[test]
    fn cache_dir_never_escapes_root() {
        let root = std::path::Path::new("/tmp/mbs-test-cache-root-nonexistent");
        for f in &[
            "../../.config/autostart/evil",
            "../../../../tmp/evil",
            "..",
            ".",
            "/etc/passwd",
            "..\\..\\evil",
            "a/../b",
            "",
            "normal",
            "[Gmail]/Sent Mail",
        ] {
            let p = get_cache_dir_with_root(root, f);
            assert!(
                p.starts_with(root),
                "get_cache_dir_with_root({:?}) = {:?} escaped {:?}",
                f,
                p,
                root
            );
        }
    }

    #[test]
    fn known_gmail_folders_preserved() {
        let root = std::path::Path::new("/tmp/mbs-test-cache-root-nonexistent");
        assert_eq!(
            get_cache_dir_with_root(root, "Sent"),
            root.join("[Gmail]/Sent Mail")
        );
        assert_eq!(
            get_cache_dir_with_root(root, "Starred"),
            root.join("[Gmail]/Starred")
        );
        assert_eq!(get_cache_dir_with_root(root, "INBOX"), root.join("INBOX"));
    }
}

#[cfg(test)]
mod draft_tests {
    use super::{delete_draft_from_str, load_drafts_from_str, read_draft_from_str};

    #[test]
    fn drafts_id_matches_body_and_delete_newest_first() {
        let raw = concat!(
            "To: old@example.com\nSubject: Older\nBody old\n---\n",
            "To: new@example.com\nSubject: Newer\n----\nTo: quoted@example.com\nSubject: Quoted\nBody new\n---\n",
        );
        let envs = load_drafts_from_str(raw);
        assert_eq!(envs.len(), 2);
        assert_eq!(envs[0].id, "draft_1");
        assert_eq!(envs[0].subject, "Newer");
        assert_eq!(envs[1].id, "draft_0");
        assert_eq!(envs[1].subject, "Older");

        let newer = read_draft_from_str(raw, &envs[0].id).unwrap();
        assert_eq!(newer.text, "----\nTo: quoted@example.com\nSubject: Quoted\nBody new");

        let older = read_draft_from_str(raw, &envs[1].id).unwrap();
        assert_eq!(older.text, "Body old");

        let after_delete = delete_draft_from_str(raw, &envs[0].id).unwrap();
        let remaining = load_drafts_from_str(&after_delete);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].subject, "Older");
        assert_eq!(
            read_draft_from_str(&after_delete, &remaining[0].id).unwrap().text,
            "Body old"
        );
    }

    #[test]
    fn extract_links_finds_html_and_text_links_and_blocks_unsafe_schemes() {
        let html = r#"
            <p>Confirm below:</p>
            <a href="https://example.com/verify?a=1&amp;b=2">Verify <b>Email</b></a>
            <a href="javascript:alert(1)">Bad</a>
            <a href="file:///etc/passwd">Local</a>
        "#;
        let text = "Confirm below. Also visit https://docs.example.org/help.";
        let links = super::extract_links(Some(html), text);
        assert_eq!(
            links,
            vec![
                ("Verify Email".to_string(), "https://example.com/verify?a=1&b=2".to_string()),
                ("".to_string(), "https://docs.example.org/help".to_string()),
            ]
        );
    }
}
