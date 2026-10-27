use mail_parser::MimeHeaders;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use lettre::message::Mailbox;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use lettre::transport::smtp::authentication::Credentials;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(rename = "moonbeauty_email_address_1", alias = "email", alias = "address", alias = "user", alias = "username", alias = "gmail_address_himalaya")]
    pub email: String,
    #[serde(rename = "moonbeauty_email_apppass_1", alias = "password", alias = "pass", alias = "code", alias = "app_password", alias = "gmail_code_himalaya")]
    pub password: Option<String>,
    #[serde(rename = "moonbeauty_password_cmd_1", alias = "password_cmd", default)]
    pub password_cmd: Option<String>,
    #[serde(rename = "moonbeauty_imap_host_1", alias = "imap_host", alias = "imap", default = "default_imap")]
    pub imap_host: String,
    #[serde(rename = "moonbeauty_imap_port_1", alias = "imap_port", default = "default_imap_port")]
    pub imap_port: u16,
    #[serde(rename = "moonbeauty_smtp_host_1", alias = "smtp_host", alias = "smtp", default = "default_smtp")]
    pub smtp_host: String,
    #[serde(rename = "moonbeauty_smtp_port_1", alias = "smtp_port", default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(rename = "moonbeauty_sops_key_1", alias = "sops_key", default)]
    pub sops_key: Option<String>,
}

fn default_imap() -> String { "imap.gmail.com".into() }
fn default_imap_port() -> u16 { 993 }
fn default_smtp() -> String { "smtp.gmail.com".into() }
fn default_smtp_port() -> u16 { 465 }

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

fn find_sops() -> String {
    use std::path::Path;
    for p in ["/run/current-system/sw/bin/sops", "/usr/bin/sops", "/usr/local/bin/sops"] {
        if Path::new(p).exists() { return p.into(); }
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".nix-profile/bin/sops");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }
    "sops".into()
}

pub fn save_config(cfg: &EmailConfig, use_sops: bool) -> Result<(), String> {
    let path = config_path();
    let enc_path = path.with_extension("enc.json");
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    
    if use_sops {
        let dir = path.parent().unwrap_or_else(|| std::path::Path::new("/tmp"));
        let tmp = dir.join("mbs-email-tmp.json");
        
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp).map_err(|e| e.to_string())?;
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
                            key_to_use = Some(line.trim().replace("# public key:", "").trim().to_string());
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
                                    cmd.arg("--age").arg(line.trim().replace("# public key:", "").trim());
                                    found_key = true;
                                    break;
                                }
                            }
                            if !found_key && content.contains("AGE-SECRET-KEY-1") {
                                if let Ok(out) = std::process::Command::new("age-keygen").args(["-y", p.to_str().unwrap()]).output() {
                                    if out.status.success() {
                                        let pk = String::from_utf8_lossy(&out.stdout).trim().to_string();
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
                    if found_key { break; }
                }
                if !found_key { cmd.arg("--pgp").arg(k); }
            } else {
                cmd.arg("--pgp").arg(k);
            }
        }
        
        cmd.arg(tmp.to_str().unwrap());
        let out = cmd.output().map_err(|e| format!("Failed to run sops: {}", e))?;
        
        if out.status.success() {
            let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&enc_path).map_err(|e| e.to_string())?;
            f.write_all(&out.stdout).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(&tmp);
            if let Some(key) = &cfg.sops_key {
                if key.ends_with(".txt") || key.contains("/") {
                    if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(enc_path.with_extension("keypath")) {
                        let _ = f.write_all(key.as_bytes());
                    }
                }
            }
            return Ok(());
        } else {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("SOPS error: {}", String::from_utf8_lossy(&out.stderr)));
        }
    } else {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).map_err(|e| e.to_string())?;
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
                        final_path = home.join(".config/sops/age").join(k).to_string_lossy().to_string();
                    }
                }
            }
            cmd.env("SOPS_AGE_KEY_FILE", final_path);
        }
        cmd.args(["--decrypt", enc_path.to_str().unwrap()]);
        let out = cmd.output().ok();
        out.and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).to_string()) } else { None })
    } else {
        None
    };

    if let Some(json) = content {
        if let Ok(cfg) = serde_json::from_str::<EmailConfig>(&json) {
            return cfg;
        }
    }
    
    let def = EmailConfig::default();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let Ok(json) = serde_json::to_string_pretty(&def) {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path) {
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

fn get_cache_dir(folder: &str) -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("moon-beauty-shell/email_cache").join(map_gmail_folder(folder))
}

pub fn load_index(folder: &str) -> Vec<NativeEnvelope> {
    let path = get_cache_dir(folder).join("index.json");
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(envs) = serde_json::from_str(&content) {
            return envs;
        }
    }
    Vec::new()
}

pub fn save_index(folder: &str, envs: &[NativeEnvelope]) {
    let dir = get_cache_dir(folder);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("index.json");
    if let Ok(content) = serde_json::to_string_pretty(envs) {
        let _ = std::fs::write(path, content);
    }
}

pub async fn fetch_envelopes(folder: &str) -> Result<Vec<NativeEnvelope>, String> {
    if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] fetch_envelopes called for {}", folder); }
    let cached = load_index(folder);
    if !cached.is_empty() {
        if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] loaded {} emails from cache", cached.len()); }
        return Ok(cached);
    }
    let envs = fetch_from_network(folder).await?;
    save_index(folder, &envs);
    Ok(envs)
}

pub async fn fetch_from_network(folder: &str) -> Result<Vec<NativeEnvelope>, String> {
    if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] fetch_from_network called for {}", folder); }
    let cfg = load_config();
    let folder = folder.to_string();
    let res = tokio::task::spawn_blocking(move || {
        let pass = resolve_password(&cfg);
        let tls = native_tls::TlsConnector::builder().build().map_err(|e| e.to_string())?;
        let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), cfg.imap_host.as_str(), &tls).map_err(|e| e.to_string())?;
        let mut session = client.login(&cfg.email, &pass).map_err(|e| e.0.to_string())?;
        session.select(&map_gmail_folder(&folder)).map_err(|e| e.to_string())?;
        let fetches = session.uid_fetch("1:*", "(FLAGS BODY.PEEK[HEADER.FIELDS (SUBJECT FROM DATE)])").map_err(|e| e.to_string())?;
            
        let mut envs = Vec::new();
        for fetch in fetches.iter().rev() {
            let is_starred = fetch.flags().iter().any(|f| matches!(f, imap::types::Flag::Flagged));
            if let Some(header) = fetch.header() {
                if let Some(parsed) = mail_parser::MessageParser::default().parse(header) {
                    let subj = parsed.subject().unwrap_or("(No Subject)").to_string();
                    let from_obj = parsed.from().and_then(|f| match f {
                        mail_parser::Address::List(list) => list.iter().next(),
                        mail_parser::Address::Group(groups) => groups.first().and_then(|g| g.addresses.first()),
                    });
                    let from_name = from_obj.and_then(|a| a.name()).unwrap_or_default().to_string();
                    let from_email = from_obj.and_then(|a| a.address()).unwrap_or("").to_string();
                    let uid_str = fetch.uid.map(|u| u.to_string()).unwrap_or_else(|| fetch.message.to_string());
                    let is_read = fetch.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));
                    let sender_display = if !from_name.trim().is_empty() { from_name } else if !from_email.trim().is_empty() { from_email.clone() } else { "Unknown".to_string() };
                    envs.push(NativeEnvelope { id: uid_str, subject: subj, sender: sender_display, sender_email: from_email, is_starred, is_read });
                }
            }
        }
        let _ = session.logout();
        let mut final_envs = envs;
        if folder == "Unread" {
            final_envs.retain(|e| !e.is_read);
        }
        Ok(final_envs)
    }).await.map_err(|e| e.to_string())?;
    res
}

async fn fetch_raw_message(id: &str, folder: &str) -> Result<Vec<u8>, String> {
    let cfg = load_config();
    let id = id.to_string();
    let folder = folder.to_string();
    tokio::task::spawn_blocking(move || {
        let pass = resolve_password(&cfg);
        let tls = native_tls::TlsConnector::builder().build().unwrap();
        let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), cfg.imap_host.as_str(), &tls).map_err(|e| e.to_string())?;
        let mut session = client.login(&cfg.email, &pass).map_err(|e| e.0.to_string())?;
        session.select(&map_gmail_folder(&folder)).map_err(|e| e.to_string())?;
        let fetches = session.uid_fetch(&id, "(BODY.PEEK[])").map_err(|e| e.to_string())?;
        if let Some(fetch) = fetches.first() {
            if let Some(body) = fetch.body() {
                return Ok(body.to_vec());
            }
        }
        Err("No body".into())
    }).await.map_err(|e| e.to_string())?
}

pub async fn sync_bodies_background(folder: String) {
    let lock_path = dirs::runtime_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join("mbs_email_sync.lock");
    
    // Try to create the lock file. If it already exists, another process is syncing.
    let _lock_file = match std::fs::OpenOptions::new().write(true).create_new(true).open(&lock_path) {
        Ok(f) => f,
        Err(_) => {
            if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] another process is already syncing, skipping duplicate spawn."); }
            return;
        }
    };
    
    // Ensure the lock is removed when we finish or panic
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
    if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] starting background sync for {} emails in {}", envs.len(), folder); }
    let cfg = load_config();
    
    tokio::task::spawn_blocking(move || {
        if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] connecting to IMAP..."); }
        let pass = resolve_password(&cfg);
        let tls = native_tls::TlsConnector::builder().build().unwrap();
        let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), cfg.imap_host.as_str(), &tls).unwrap();
        let mut session = client.login(&cfg.email, &pass).unwrap();
        
        let imap_folder = map_gmail_folder(&folder);
        session.select(&imap_folder).unwrap();
        
        let mut count = 0;
        let total = envs.len();
        
        for env in envs {
            let cache_file = get_cache_dir(&folder).join(format!("{}.eml", env.id));
            if !cache_file.exists() {
                std::thread::sleep(std::time::Duration::from_secs(12));
                if let Ok(fetches) = session.uid_fetch(&env.id, "(BODY.PEEK[])") {
                    if let Some(fetch) = fetches.first() {
                        if let Some(body) = fetch.body() {
                            let _ = std::fs::write(&cache_file, body);
                            count += 1;
                            if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] downloaded {}/{}: {}", count, total, env.id); }
                        }
                    }
                }
            }
        }
        let _ = session.logout();
        if std::env::var_os("MBS_DEBUG").is_some() { eprintln!("[sync] background sync finished for {}", folder); }
    }).await.unwrap_or_else(|e| eprintln!("[sync] task failed: {}", e));
}



pub async fn read_message(id: &str, folder: &str) -> ParsedEmail {
    let cache_file = get_cache_dir(folder).join(format!("{}.eml", id));
    let bytes = if cache_file.exists() && cache_file.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        std::fs::read(&cache_file).unwrap_or_default()
    } else {
        match fetch_raw_message(id, folder).await {
            Ok(b) => {
                let _ = std::fs::write(&cache_file, &b);
                b
            }
            Err(e) => return ParsedEmail {
                text: format!("Failed to fetch message: {}", e),
                images: Vec::new(),
                attachments: Vec::new(),
            },
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
                parsed_email.text = html2text::from_read(h_str.as_bytes(), 80);
            }
        }

        if parsed_email.text.trim().is_empty() {
            parsed_email.text = "This email has no readable text.".into();
        }

        let img_dir = dirs::runtime_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join("mbs-email-images");
        let _ = std::fs::create_dir_all(&img_dir);

        // 2. Attachments (all non-text parts with a filename are attachments)
        for attachment in parsed.attachments() {
            let name = attachment.attachment_name().unwrap_or("unknown").to_string();
            let data = attachment.contents().to_vec();
            eprintln!("[email-debug] attachment: {} ({} bytes)", name, data.len());
            parsed_email.attachments.push((name, data));
        }

        // 3. Inline Images
        for (i, part) in parsed.parts.iter().enumerate() {
            if part.is_text_html() || part.is_text() {
                continue;
            }
            let ctype = part.content_type().map(|c| c.ctype()).unwrap_or("");
            if ctype == "image" {
                let subtype = part.content_type()
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
                let img_path = img_dir.join(format!("{}_{}", id, safe_name));
                let _ = std::fs::write(&img_path, part.contents());
                parsed_email.images.push(img_path);
                eprintln!("[email-debug] saved image: {} ({} bytes)", safe_name, part.contents().len());
            }
        }

        return parsed_email;
    }
    parsed_email.text = "Failed to parse message.".into();
    parsed_email
}

pub async fn send_message(to: &str, subject: &str, body: &str, attachments: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    use lettre::message::{header::ContentDisposition, MultiPart, SinglePart};

    fn mime_for(name: &str) -> lettre::message::header::ContentType {
        let lower = name.to_lowercase();
        let mime = if lower.ends_with(".png") { "image/png" }
            else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg" }
            else if lower.ends_with(".gif") { "image/gif" }
            else if lower.ends_with(".webp") { "image/webp" }
            else if lower.ends_with(".pdf") { "application/pdf" }
            else if lower.ends_with(".txt") { "text/plain; charset=utf-8" }
            else if lower.ends_with(".zip") { "application/zip" }
            else if lower.ends_with(".mp3") { "audio/mpeg" }
            else if lower.ends_with(".mp4") { "video/mp4" }
            else { "application/octet-stream" };
        lettre::message::header::ContentType::parse(mime)
            .unwrap_or_else(|_| lettre::message::header::ContentType::parse("application/octet-stream").unwrap())
    }

    let cfg = load_config();
    let from_addr: Mailbox = cfg.email.parse().map_err(|_| "Invalid from address".to_string())?;
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
                .body(body.to_string())
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
            eprintln!("[send] MIME check: attachment_header={} content_types={} total_bytes={}", 
                has_attachment, content_type_count, raw.len());
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
    let mailer: AsyncSmtpTransport<Tokio1Executor> = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.smtp_host)
        .map_err(|e| e.to_string())?
        .port(cfg.smtp_port)
        .credentials(creds)
        .build();
    mailer.send(email).await.map(|_| ()).map_err(|e| e.to_string())
}

pub async fn toggle_star(id: &str, folder: &str, star: bool) -> Result<(), String> {
    let mut envs = load_index(folder);
    if let Some(env) = envs.iter_mut().find(|e| e.id == id) {
        env.is_starred = star;
        save_index(folder, &envs);
    }

    let cfg = load_config();
    let id = id.to_string();
    let folder = folder.to_string();
    tokio::task::spawn_blocking(move || {
        let pass = resolve_password(&cfg);
        let tls = native_tls::TlsConnector::builder().build().unwrap();
        let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), cfg.imap_host.as_str(), &tls).unwrap();
        let mut session = client.login(&cfg.email, &pass).map_err(|e| e.0.to_string())?;
        session.select(&map_gmail_folder(&folder)).map_err(|e| e.to_string())?;
        let op = if star { "+FLAGS (\\Flagged)" } else { "-FLAGS (\\Flagged)" };
        session.uid_store(&id, op).map_err(|e| e.to_string())?;
        let _ = session.logout();
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())?
}


pub async fn move_to_trash(id: &str, folder: &str) -> Result<(), String> {
    let cfg = load_config();
    let id = id.to_string();
    let folder = folder.to_string();
    tokio::task::spawn_blocking(move || {
        let pass = resolve_password(&cfg);
        let tls = native_tls::TlsConnector::builder().build().unwrap();
        let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), cfg.imap_host.as_str(), &tls).unwrap();
        let mut session = client.login(&cfg.email, &pass).map_err(|e| e.0.to_string())?;
        
        let orig_folder = map_gmail_folder(&folder);
        let trash_folder = map_gmail_folder("Trash");
        
        session.select(&orig_folder).map_err(|e| e.to_string())?;
        
        // Copy to Trash
        session.uid_copy(&id, &trash_folder).map_err(|e| e.to_string())?;
        
        // Delete from Original
        session.uid_store(&id, "+FLAGS (\\Deleted)").map_err(|e| e.to_string())?;
        session.expunge().map_err(|e| e.to_string())?;
        
        let _ = session.logout();
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())?
}

pub async fn delete_message(id: &str, folder: &str) -> Result<(), String> {
    let mut envs = load_index(folder);
    envs.retain(|e| e.id != id);
    save_index(folder, &envs);

    let cfg = load_config();
    let id = id.to_string();
    let folder = folder.to_string();
    tokio::task::spawn_blocking(move || {
        let pass = resolve_password(&cfg);
        let tls = native_tls::TlsConnector::builder().build().unwrap();
        let client = imap::connect((cfg.imap_host.as_str(), cfg.imap_port), cfg.imap_host.as_str(), &tls).unwrap();
        let mut session = client.login(&cfg.email, &pass).map_err(|e| e.0.to_string())?;
        session.select(&map_gmail_folder(&folder)).map_err(|e| e.to_string())?;
        session.uid_store(&id, "+FLAGS (\\Deleted)").map_err(|e| e.to_string())?;
        session.expunge().map_err(|e| e.to_string())?;
        let _ = session.logout();
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())?
}


pub fn load_contacts() -> Vec<String> {
    let path = std::path::PathBuf::from("resources/contacts.txt");
    if let Ok(content) = std::fs::read_to_string(path) {
        content.lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

pub async fn save_contact(email: &str) -> Result<(), String> {
    let path = std::path::PathBuf::from("resources/contacts.txt");
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    f.write_all(format!("{}\n", email).as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn save_draft(to: &str, subject: &str, body: &str) -> Result<(), String> {
    let path = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp")).join("moon-beauty-shell/drafts.txt");
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    f.write_all(format!("To: {}\nSubject: {}\n{}\n---\n", to, subject, body).as_bytes()).map_err(|e| e.to_string())?;
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
    }.to_string()
}
