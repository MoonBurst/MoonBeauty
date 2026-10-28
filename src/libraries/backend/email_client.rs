use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use async_imap::types::Fetch as AsyncFetch;
use futures_util::TryStreamExt;
use mail_parser::MimeHeaders;
use tokio::net::TcpStream;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

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
    base.join("moon-beauty-shell").join("email.json")
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
        let dir = path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("/tmp"));
        let tmp = dir.join("mbs-email-tmp.json");

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        f.sync_all().ok();

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
        f.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Finding 28: reject folder names that could break out of the SELECT
/// command sent to the IMAP server. `async_imap::Session::select()` and
/// `uid_fetch()` format the folder as a quoted string; CR/LF/control
/// chars can terminate the command and inject arbitrary IMAP commands
/// (e.g. a hostile LIST response like "INBOX\r\nAPPEND INBOX ...").
///
/// Reject: empty, over 512 bytes, any C0 control char (0x00-0x1f)
/// including CR/LF, plus DEL (0x7f).
/// Accept: everything else (paths, "[Gmail]/Sent Mail", unicode names).
pub fn is_safe_imap_folder(folder: &str) -> bool {
    if folder.is_empty() || folder.len() > 512 {
        return false;
    }
    !folder.chars().any(|c| c.is_control())
}

/// Finding 29: reject any message id that isn't a plain ASCII integer.
///
/// `async-imap::Session::uid_fetch`, `uid_store`, and `uid_copy` embed
/// the uid in the wire command. A crafted `id` from a poisoned
/// `index.json` (or a future IPC caller) could inject IMAP commands:
/// `uid_fetch("1; APPEND INBOX ...")` breaks out of the fetch. Numeric
/// validation is the only safe contract — server-assigned uids are
/// always u32 integers, serialized to 1-10 ASCII digits.
///
/// Accept: non-empty, ≤20 bytes (u64::MAX is 20 digits), all ASCII digits.
pub fn is_safe_imap_uid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.chars().all(|c| c.is_ascii_digit())
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
            let _ = f.write_all(json.as_bytes());
        }
    }
    def
}

#[derive(Debug, Clone)]
pub struct ParsedEmail {
    pub text: String,
    pub images: Vec<std::path::PathBuf>,
    pub attachments: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeEnvelope {
    pub id: String,
    pub subject: String,
    pub sender: String,
    pub sender_email: String,
    pub is_starred: bool,
    pub is_read: bool,
}

// ── Secure filesystem helpers (finding 03) ────────────────────────
//
// Every write to the email cache, drafts file, and folders list must
// land at mode 0600 (owner RW, no group/other access). Prior code used
// std::fs::write / tokio::fs::write / bare OpenOptions, which inherit
// umask (default 022 → 0644). On a multi-user host that made every
// cached email body, attachment, and draft readable by other local
// users. Sink-level fix: all writes go through these helpers.
//
// Note: OpenOptions.mode() only applies to NEW files. If the file
// already exists at 0644, mode() is a no-op — so we also chmod
// explicitly after opening. That's what chmod_private exists for.

fn chmod_private(path: &std::path::Path, is_dir: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = if is_dir { 0o700 } else { 0o600 };
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)
}

/// Overwrite `path` with `bytes` at mode 0600, creating parent dirs at
/// 0700. Safe to call on an existing file with looser permissions: the
/// explicit chmod after open handles that case.
fn secure_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        let _ = chmod_private(parent, true);
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    let _ = chmod_private(path, false);
    f.write_all(bytes)
}

async fn secure_write_async(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt as _;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
        let _ = chmod_private(parent, true);
    }
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .await?;
    let _ = chmod_private(path, false);
    f.write_all(bytes).await
}

/// Append-only variant for logs (`drafts.txt`). Same permission model
/// as secure_write.
fn secure_append(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        let _ = chmod_private(parent, true);
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    let _ = chmod_private(path, false);
    f.write_all(bytes)
}

/// Recursively chmod every directory under `root` to 0700 and every
/// file to 0600. Idempotent; called once per process by the real
/// entry point (`ensure_secure_cache_tree`), and directly by tests.
fn ensure_secure_cache_tree_at(root: &std::path::Path) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let _ = chmod_private(root, true);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                let _ = chmod_private(&p, true);
                stack.push(p);
            } else if ft.is_file() {
                let _ = chmod_private(&p, false);
            }
        }
    }
    Ok(())
}

/// One-shot migration on first call per process. Fixes any pre-existing
/// cache entries that were written at 0644 by earlier builds. Cheap
/// (guarded by OnceLock); subsequent calls are no-ops.
fn ensure_secure_cache_tree() {
    use std::sync::OnceLock;
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let root = base.join("moon-beauty-shell/email_cache");
        if !root.exists() {
            return;
        }
        match ensure_secure_cache_tree_at(&root) {
            Ok(()) => eprintln!("[email] secure-cache migration: applied to {:?}", root),
            Err(e) => eprintln!("[email] secure-cache migration failed: {e}"),
        }
    });
}

fn get_cache_dir(folder: &str) -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let root = base.join("moon-beauty-shell/email_cache");
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
    // Defense-in-depth: sanitize is unit-tested; if a future refactor
    // weakens it, refuse rather than traverse.
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
            // Finding 29: reject poisoned envelopes from a hand-edited
            // or old-version index.json before their ids can reach
            // uid_fetch/uid_store/uid_copy.
            let before = envs.len();
            envs.retain(|e| is_safe_imap_uid(&e.id));
            if envs.len() < before {
                eprintln!(
                    "[email] dropped {} unsafe envelope id(s) from {}",
                    before - envs.len(), folder
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
    let smtp_ok = match lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&cfg.smtp_host) {
        Ok(builder) => {
            let _mailer: lettre::AsyncSmtpTransport<lettre::Tokio1Executor> = builder
                .port(cfg.smtp_port)
                .credentials(lettre::transport::smtp::authentication::Credentials::new(
                    cfg.email.clone(), pass))
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
    // Finding 28: defense-in-depth at the network sink. Even a folder
    // from a stale cache (or from auto_sort's sender-derived names) must
    // pass this check before reaching SELECT.
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
        .uid_fetch("1:*", "(FLAGS BODY.PEEK[HEADER.FIELDS (SUBJECT FROM DATE)])")
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
    // Finding 29: numeric-uid gate before uid_fetch.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    // Finding 28: defense-in-depth at the network sink.
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
    let lock_path = crate::libraries::process::mbs_runtime_dir()
        .join("mbs_email_sync.lock");
    let _lock_file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(_) => {
            if std::env::var_os("MBS_DEBUG").is_some() {
                eprintln!("[sync] another process is already syncing, skipping duplicate spawn.");
            }
            return;
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
    let mut session = match connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[sync] connect failed: {e}");
            return;
        }
    };
    let imap_folder = map_gmail_folder(&folder);
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
            match session.uid_fetch(&env.id, "(BODY.PEEK[])").await {
                Ok(stream) => match stream.try_collect::<Vec<_>>().await {
                    Ok(fetches) => {
                        if let Some(fetch) = fetches.first() {
                            if let Some(body) = fetch.body() {
                                // Finding 06: cap body cache at 100 MB. Larger bodies are
                                // almost always attachments mail-parser couldn't split.
                                const MAX_BODY: usize = 100 * 1024 * 1024;
                                if body.len() > MAX_BODY {
                                    eprintln!(
                                        "[sync] skipping oversize body {} ({} bytes)",
                                        env.id, body.len()
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
    // Finding 26: cap raw MIME size BEFORE parse. mail_parser decodes the
    // full MIME tree in memory; base64/QUOTED-PRINTABLE can expand up to
    // 3×, so a 500 MB body from a hostile server/peer is a multi-GB
    // allocation before our post-decode caps (06c/06e) can fire. Matches
    // the 100 MB cap used in sync_bodies_background (06e) for consistency.
    const MAX_RAW: u64 = 100 * 1024 * 1024;
    let cache_file = get_cache_dir(folder).join(format!("{}.eml", id));
    let bytes =
        if cache_file.exists() && cache_file.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            // Pre-check cached size via metadata (no allocation). Prevents
            // std::fs::read from loading a hostile 500 MB cache file that
            // a previous build wrote before batch-2 caps existed.
            if cache_file.metadata().map(|m| m.len()).unwrap_or(0) > MAX_RAW {
                return ParsedEmail {
                    text: format!(
                        "Message too large to display (cached size exceeds {} MB)",
                        MAX_RAW / (1024 * 1024)
                    ),
                    images: Vec::new(),
                    attachments: Vec::new(),
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
                    }
                }
            }
        };

    let mut parsed_email = ParsedEmail {
        text: String::new(),
        images: Vec::new(),
        attachments: Vec::new(),
    };

    if let Some(parsed) = mail_parser::MessageParser::default().parse(&bytes) {
        // 1. Extract Text
        if let Some(t) = parsed.body_text(0) {
            parsed_email.text = t.to_string();
        }

        if parsed_email.text.trim().is_empty() {
            if let Some(h) = parsed.body_html(0) {
                let h_str = h.to_string();
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
                        && (line[1..].starts_with("http://")
                            || line[1..].starts_with("https://"))
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
            // Finding 06: reject oversize attachments before storing.
            const MAX_ATTACH: usize = 50 * 1024 * 1024;
            if data.len() > MAX_ATTACH {
                eprintln!(
                    "[email] skipping oversize attachment: {} ({} bytes)",
                    name, data.len()
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
                // Finding 06: reject oversize inline images before write.
                const MAX_IMG: usize = 10 * 1024 * 1024;
                if part.contents().len() > MAX_IMG {
                    eprintln!(
                        "[email] skipping oversize inline image: {} ({} bytes)",
                        safe_name, part.contents().len()
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
            // Log first 1500 bytes for inspection
            eprintln!("[send] ─── MIME START ───");
            for line in raw.lines().take(40) {
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
    // Finding 29: numeric-uid gate before uid_store.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
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
    let op = if star { "+FLAGS (\\Flagged)" } else { "-FLAGS (\\Flagged)" };
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

pub async fn move_to_trash(id: &str, folder: &str) -> Result<(), String> {
    // Finding 29: numeric-uid gate before uid_copy/uid_store.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    let orig_folder = map_gmail_folder(folder);
    let trash_folder = map_gmail_folder("Trash");
    session
        .select(&orig_folder)
        .await
        .map_err(|e| e.to_string())?;
    session
        .uid_copy(id, &trash_folder)
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

pub async fn move_to_spam(id: &str, folder: &str) -> Result<(), String> {
    // Finding 29: numeric-uid gate before uid_copy/uid_store.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    let orig_folder = map_gmail_folder(folder);
    let spam_folder = map_gmail_folder("Spam");
    session.select(&orig_folder).await.map_err(|e| e.to_string())?;
    session
        .uid_copy(id, &spam_folder)
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

pub async fn mark_not_spam(id: &str) -> Result<(), String> {
    // Finding 29: numeric-uid gate before uid_copy/uid_store.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
    }
    let cfg = load_config();
    let pass = resolve_password(&cfg);
    let mut session = connect_and_login(&cfg.imap_host, cfg.imap_port, &cfg.email, &pass).await?;
    session
        .select("[Gmail]/Spam")
        .await
        .map_err(|e| e.to_string())?;
    session
        .uid_copy(id, "INBOX")
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

pub async fn delete_message(id: &str, folder: &str) -> Result<(), String> {
    // Finding 29: numeric-uid gate before uid_store.
    if !is_safe_imap_uid(id) {
        return Err(format!("unsafe message id: {:?}", id));
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
    let stream = session.list(Some(""), Some("*")).await.map_err(|e| e.to_string())?;
    let names: Vec<_> = stream.try_collect().await.map_err(|e| e.to_string())?;
    let mut folders = Vec::new();
    for name in names {
        let n = name.name();
        if n.is_empty() || n == "INBOX" || n.starts_with("[Gmail]") {
            continue;
        }
        // Finding 28: reject hostile server LIST responses. A folder
        // named "FOO\r\nAPPEND ..." would break out of SELECT.
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
    base.join("moon-beauty-shell/email_cache/folders.json")
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
        .join("moon-beauty-shell")
        .join("contacts.txt")
}

pub fn load_contacts() -> Vec<String> {
    let path = contacts_path();
    // Finding 20: one-shot migration. Only accept if every non-empty
    // line looks like an email — rejects a stray ./resources/contacts.txt.
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
                    let _ = crate::libraries::process::private_write_file(
                        &path, content.as_bytes());
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

pub async fn save_draft(to: &str, subject: &str, body: &str) -> Result<(), String> {
    let path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("moon-beauty-shell/drafts.txt");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    secure_append(
        &path,
        format!("To: {}\nSubject: {}\n{}\n---\n", to, subject, body).as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod secure_fs_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn mode_of(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mbs-sec-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn secure_write_creates_0600() {
        let dir = unique_dir("write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sub").join("file.txt");
        secure_write(&path, b"secret").unwrap();
        assert_eq!(mode_of(&path), 0o600, "file mode");
        assert_eq!(mode_of(&path.parent().unwrap()), 0o700, "parent mode");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secure_write_downgrades_existing_loose_file() {
        let dir = unique_dir("downgrade");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("file.txt");
        std::fs::write(&path, b"x").unwrap();
        let mut p = std::fs::metadata(&path).unwrap().permissions();
        p.set_mode(0o644);
        std::fs::set_permissions(&path, p).unwrap();
        assert_eq!(mode_of(&path), 0o644, "precondition");
        secure_write(&path, b"y").unwrap();
        assert_eq!(mode_of(&path), 0o600, "post-fix");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secure_append_preserves_content_and_tightens_mode() {
        let dir = unique_dir("append");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("drafts.txt");
        secure_append(&path, b"one\n").unwrap();
        secure_append(&path, b"two\n").unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body, "one\ntwo\n");
        assert_eq!(mode_of(&path), 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_tree_chmods_recursively() {
        let dir = unique_dir("tree");
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("INBOX");
        std::fs::create_dir_all(&sub).unwrap();
        let f1 = dir.join("index.json");
        let f2 = sub.join("mail.eml");
        std::fs::write(&f1, b"[]").unwrap();
        std::fs::write(&f2, b"body").unwrap();
        for (p, m) in [(&dir, 0o755_u32), (&sub, 0o755), (&f1, 0o644), (&f2, 0o644)] {
            let mut pm = std::fs::metadata(p).unwrap().permissions();
            pm.set_mode(m);
            std::fs::set_permissions(p, pm).unwrap();
        }
        ensure_secure_cache_tree_at(&dir).unwrap();
        assert_eq!(mode_of(&dir), 0o700, "root");
        assert_eq!(mode_of(&sub), 0o700, "subdir");
        assert_eq!(mode_of(&f1), 0o600, "file1");
        assert_eq!(mode_of(&f2), 0o600, "file2");
        let _ = std::fs::remove_dir_all(&dir);
    }
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
mod imap_uid_tests {
    use super::is_safe_imap_uid;

    #[test]
    fn numeric_uids_accepted() {
        assert!(is_safe_imap_uid("1"));
        assert!(is_safe_imap_uid("12345"));
        assert!(is_safe_imap_uid("18446744073709551615")); // u64::MAX, 20 digits
        assert!(is_safe_imap_uid("00000000000000000001"));
    }

    #[test]
    fn injection_attempts_rejected() {
        assert!(!is_safe_imap_uid(""));
        assert!(!is_safe_imap_uid("1; DELETE INBOX"));
        assert!(!is_safe_imap_uid("1\r\nDELETE INBOX"));
        assert!(!is_safe_imap_uid("1 2"));
        assert!(!is_safe_imap_uid("-1"));
        assert!(!is_safe_imap_uid("1.0"));
        assert!(!is_safe_imap_uid("1\tAPPEND"));
        assert!(!is_safe_imap_uid("123456789012345678901")); // 21 digits
    }

    #[test]
    fn non_ascii_digits_rejected() {
        // Unicode digits that is_ascii_digit rejects but are numeric —
        // async-imap would send them verbatim, breaking the command.
        assert!(!is_safe_imap_uid("١٢٣")); // Arabic-Indic
        assert!(!is_safe_imap_uid("1é"));
        assert!(!is_safe_imap_uid("１")); // fullwidth digit one
    }
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
                f, p, root
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
        assert_eq!(
            get_cache_dir_with_root(root, "INBOX"),
            root.join("INBOX")
        );
    }
}
