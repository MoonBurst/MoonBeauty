use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input, Stack};
use iced::{Element, Length};
use std::collections::HashMap;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

pub const COMPOSE_TO_ID: &str = "compose_to";
pub const COMPOSE_SUBJ_ID: &str = "compose_subj";
pub const COMPOSE_BODY_ID: &str = "compose_body";
pub const COMPOSE_MANUAL_ATTACH_ID: &str = "compose_manual_attach";

/// Finding 25: reject auto-opening attachments whose extension could
/// yield code execution when double-clicked on a default config.
/// A hostile sender delivering `invoice.pdf.desktop` looks like a PDF to
/// the user but routes xdg-open → desktop file Exec= → RCE.
fn is_safe_to_auto_open(path: &std::path::Path) -> bool {
    const DENY_EXT: &[&str] = &[
        "desktop", "service", "sh", "bash", "zsh", "ksh", "csh", "fish", "py", "pl", "rb", "php",
        "lua", "exe", "bat", "cmd", "ps1", "vbs", "app", "command", "run", "bin", "msi", "dmg",
        "pkg", "deb", "rpm", "appimage", "lnk", "url", "scf", "reg", "hta",
    ];
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => !DENY_EXT.iter().any(|d| ext.eq_ignore_ascii_case(d)),
        None => false,
    }
}

#[derive(Debug, Clone)]
pub enum EmailMessage {
    SelectFolder(String),
    SelectMail(String),
    EnvelopesLoaded(
        String,
        Vec<crate::libraries::backend::email::client::NativeEnvelope>,
    ),
    NetworkEnvelopesLoaded(
        String,
        Vec<crate::libraries::backend::email::client::NativeEnvelope>,
    ),
    FoldersLoaded(Vec<String>),
    SyncDone,
    MessageLoaded(crate::libraries::backend::email::client::ParsedEmail),
    Error(String),
    SetLoading,
    OpenComposer,
    ToChanged(String),
    SubjectChanged(String),
    BodyChanged(iced::widget::text_editor::Action),
    AttachmentsPicked(Vec<std::path::PathBuf>),
    SendMail,
    AddAttachment,
    ManualAttachChanged(String),
    SubmitManualAttach,
    SelectContact(String),
    SelectAutocompleteNext,
    SelectAutocompletePrev,
    ConfirmAutocomplete,
    AttachmentPicked(std::path::PathBuf),
    RemoveAttachment(usize),
    CancelCompose,
    OpenLogin,
    DismissLogin,
    LoginEmailChanged(String),
    LoginPasswordChanged(String),
    LoginImapChanged(String),
    LoginSmtpChanged(String),
    ToggleSops(bool),
    LoginSopsKeyChanged(String),
    SaveLogin,
    Dismiss,
    GetAppPassword,
    SelectMailNext,
    SelectMailPrev,
    SelectFolderNext,
    SelectFolderPrev,
    OpenSelected,
    ToggleStar,
    DeleteSelected,
    SaveContact,
    DraftSaved,
    FocusNextPane,
    FocusPrevPane,
    OpenInBrowser,
    ToggleAdvancedServers,
    StartAddFolder,
    AddFolderChanged(String),
    SubmitAddFolder,
    RemoveFolder(String),
    SaveAttachment(String),
    CopyGmailFilter,
    MarkSpam,
    MarkNotSpam,
    /// Enter on a loaded email, or the ⛶ button, enters fullscreen
    /// read. Enter again or Escape exits.
    ToggleFullscreen,
    /// Back button in fullscreen header. Same effect as Escape.
    ExitFullscreen,
}

pub struct EmailOverlay {
    pub folders: Vec<String>,
    pub active_folder: String,
    pub active_folder_idx: usize,
    pub envelopes: Vec<crate::libraries::backend::email::client::NativeEnvelope>,
    pub active_mail_id: Option<String>,
    pub selected_index: usize,
    pub focus_pane: usize,
    pub parsed_email: Option<crate::libraries::backend::email::client::ParsedEmail>,
    pub is_composing: bool,
    pub compose_to: String,
    pub compose_subject: String,
    pub compose_body: iced::widget::text_editor::Content,
    pub active_compose_field: usize,
    pub compose_attachments: Vec<std::path::PathBuf>,
    pub compose_manual_attach: String,
    pub contacts: Vec<String>,
    pub selected_autocomplete_idx: Option<usize>,
    pub is_loading: bool,
    pub imap_error: Option<String>,
    pub is_logging_in: bool,
    pub show_advanced_servers: bool,
    pub login_email: String,
    pub login_password: String,
    pub login_imap: String,
    pub login_smtp: String,
    pub use_sops: bool,
    pub login_sops_key: String,
    pub login_error: Option<String>,
    pub is_adding_folder: bool,
    pub new_folder_name: String,
    /// When true and a message is loaded, render it fullscreen with a
    /// back button instead of the 3-pane layout. Enter toggles.
    pub fullscreen_read: bool,
    /// When true, compose was entered from fullview's Enter-reply
    /// action. Cancel restores fullview; send stays 3-pane.
    pub reply_from_fullview: bool,
    /// In-memory parsed-email cache keyed by message id. Populated on
    /// every successful load; cleared on folder switch. Eliminates the
    /// Some->None->Some flicker when re-viewing an already-seen email.
    pub parsed_cache: HashMap<String, crate::libraries::backend::email::client::ParsedEmail>,
}

impl Default for EmailOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailOverlay {
    /// Cheap constructor — skips config/contacts load. Used when the
    /// email overlay is not the target (spawning for another overlay).
    pub fn empty() -> Self {
        let mut folders: Vec<String> = vec!["INBOX".into()];
        for f in crate::libraries::backend::email::client::load_folders_cache() {
            if !f.is_empty() && !folders.contains(&f) {
                folders.push(f);
            }
        }
        Self {
            folders,
            active_folder: "INBOX".into(),
            active_folder_idx: 0,
            envelopes: Vec::new(),
            active_mail_id: None,
            selected_index: 0,
            focus_pane: 0,
            parsed_email: None,
            is_composing: false,
            compose_to: String::new(),
            compose_subject: String::new(),
            compose_body: iced::widget::text_editor::Content::new(),
            active_compose_field: 0,
            compose_attachments: Vec::new(),
            compose_manual_attach: String::new(),
            contacts: Vec::new(),
            selected_autocomplete_idx: None,
            is_loading: false,
            imap_error: None,
            is_logging_in: false,
            show_advanced_servers: false,
            login_email: String::new(),
            login_password: String::new(),
            login_imap: "imap.gmail.com".into(),
            login_smtp: "smtp.gmail.com".into(),
            use_sops: false,
            login_sops_key: String::new(),
            login_error: None,
            is_adding_folder: false,
            new_folder_name: String::new(),
            fullscreen_read: false,
            reply_from_fullview: false,
            parsed_cache: HashMap::new(),
        }
    }

    pub fn new() -> Self {
        let is_logging_in = !crate::libraries::backend::email::client::config_exists();
        let mut folders: Vec<String> = vec![
            "INBOX".into(),
            "Unread".into(),
            "Starred".into(),
            "Sent".into(),
            "Drafts".into(),
            "Trash".into(),
            "Spam".into(),
        ];
        for f in crate::libraries::backend::email::client::load_folders_cache() {
            if !f.is_empty() && !folders.contains(&f) {
                folders.push(f);
            }
        }
        Self {
            folders,
            active_folder: "INBOX".into(),
            active_folder_idx: 0,
            envelopes: Vec::new(),
            active_mail_id: None,
            selected_index: 0,
            focus_pane: 0,
            parsed_email: None,
            is_composing: false,
            compose_to: String::new(),
            compose_subject: String::new(),
            compose_body: iced::widget::text_editor::Content::new(),
            active_compose_field: 0,
            compose_attachments: Vec::new(),
            compose_manual_attach: String::new(),
            contacts: crate::libraries::backend::email::client::load_contacts(),
            selected_autocomplete_idx: None,
            is_loading: false,
            imap_error: None,
            is_logging_in,
            show_advanced_servers: false,
            login_email: String::new(),
            login_password: String::new(),
            login_imap: "imap.gmail.com".into(),
            login_smtp: "smtp.gmail.com".into(),
            use_sops: false,
            login_sops_key: String::new(),
            login_error: None,
            is_adding_folder: false,
            new_folder_name: String::new(),
            fullscreen_read: false,
            reply_from_fullview: false,
            parsed_cache: HashMap::new(),
        }
    }

    pub fn get_autocomplete_matches(&self) -> Vec<String> {
        if self.compose_to.is_empty() {
            return Vec::new();
        }
        let q = self.compose_to.to_lowercase();
        self.contacts
            .iter()
            .filter(|c| c.to_lowercase().contains(&q) && c.to_lowercase() != q)
            .take(5)
            .cloned()
            .collect()
    }

    pub fn update(&mut self, message: EmailMessage) {
        match message {
            EmailMessage::SelectFolder(f) => {
                if let Some(idx) = self.folders.iter().position(|x| x == &f) {
                    self.active_folder_idx = idx;
                }
                self.active_folder = f;
                self.selected_index = 0;
                self.active_mail_id = None;
                self.parsed_email = None;
                self.parsed_cache.clear();
            }
            EmailMessage::SelectMail(id) => {
                // Switching mails exits fullscreen.
                self.fullscreen_read = false;
                let same_and_loaded = self.active_mail_id.as_deref() == Some(id.as_str())
                    && self.parsed_email.is_some();
                if !same_and_loaded {
                    self.active_mail_id = Some(id.clone());
                    // Cache hit: instant display, no Loading frame.
                    // Cache miss: fall back to the async fetch path.
                    if let Some(cached) = self.parsed_cache.get(&id).cloned() {
                        self.parsed_email = Some(cached);
                    } else {
                        self.parsed_email = None;
                    }
                }
            }
            EmailMessage::EnvelopesLoaded(_, envs)
            | EmailMessage::NetworkEnvelopesLoaded(_, envs) => {
                self.envelopes = envs;
                self.is_loading = false;
                self.imap_error = None;
            }
            EmailMessage::SyncDone => {}
            EmailMessage::FoldersLoaded(raw) => {
                let aliases = [
                    "INBOX", "Unread", "Starred", "Sent", "Drafts", "Trash", "Spam",
                ];
                let mut merged: Vec<String> = aliases.iter().map(|s| s.to_string()).collect();
                let mut user_folders: Vec<String> = raw
                    .into_iter()
                    .filter(|f| {
                        !f.is_empty()
                            && f != "INBOX"
                            && !f.starts_with("[Gmail]")
                            && !aliases.contains(&f.as_str())
                    })
                    .collect();
                user_folders.sort();
                user_folders.dedup();
                merged.extend(user_folders.clone());
                if let Some(idx) = merged.iter().position(|f| f == &self.active_folder) {
                    self.active_folder_idx = idx;
                } else {
                    self.active_folder = "INBOX".into();
                    self.active_folder_idx = 0;
                }
                crate::libraries::backend::email::client::save_folders_cache(&user_folders);
                self.folders = merged;
            }
            EmailMessage::MessageLoaded(b) => {
                // Skip caching synthetic responses from SendMail.
                // Real bodies start with whatever the sender wrote;
                // SendMail confirmations start with ✓ or ✗.
                let trimmed = b.text.trim_start();
                let is_synthetic = trimmed.starts_with('✓') || trimmed.starts_with('✗');
                if !is_synthetic {
                    if let Some(id) = self.active_mail_id.clone() {
                        // Cap the cache at 5 entries to prevent memory leaks.
                        // Emails with large attachments (PDFs, images) can be
                        // tens of MB each. If we don't evict, RAM grows forever.
                        if self.parsed_cache.len() >= 5 && !self.parsed_cache.contains_key(&id) {
                            // Remove an arbitrary old entry (HashMap doesn't keep order,
                            // but dropping *any* old entry is fine for an LRU cache).
                            if let Some(oldest) = self.parsed_cache.keys().next().cloned() {
                                self.parsed_cache.remove(&oldest);
                            }
                        }
                        self.parsed_cache.insert(id, b.clone());
                    }
                }
                self.parsed_email = Some(b);
            }
            EmailMessage::Error(e) => {
                self.imap_error = Some(e);
                self.is_loading = false;
            }
            EmailMessage::SetLoading => {
                self.is_loading = true;
                self.imap_error = None;
                self.envelopes.clear();
                self.parsed_email = None;
            }
            EmailMessage::OpenComposer => {
                self.is_composing = true;
                self.active_mail_id = None;
                self.compose_attachments.clear();
            }
            EmailMessage::ToChanged(t) => {
                self.compose_to = t;
                self.selected_autocomplete_idx = Some(0);
            }
            EmailMessage::SubjectChanged(s) => self.compose_subject = s,
            EmailMessage::BodyChanged(b) => {
                self.compose_body.perform(b);
            }
            EmailMessage::AddAttachment => {}
            EmailMessage::ManualAttachChanged(v) => self.compose_manual_attach = v,
            EmailMessage::SubmitManualAttach => {
                let path = std::path::PathBuf::from(self.compose_manual_attach.trim());
                if path.exists() {
                    self.compose_attachments.push(path);
                    self.compose_manual_attach.clear();
                }
            }
            EmailMessage::SelectContact(c) => {
                self.compose_to = c;
                self.selected_autocomplete_idx = None;
            }
            EmailMessage::SelectAutocompleteNext => {
                let matches = self.get_autocomplete_matches();
                if !matches.is_empty() {
                    let idx = self.selected_autocomplete_idx.unwrap_or(0);
                    self.selected_autocomplete_idx = Some((idx + 1) % matches.len());
                }
            }
            EmailMessage::SelectAutocompletePrev => {
                let matches = self.get_autocomplete_matches();
                if !matches.is_empty() {
                    let idx = self.selected_autocomplete_idx.unwrap_or(0);
                    self.selected_autocomplete_idx =
                        Some(if idx == 0 { matches.len() - 1 } else { idx - 1 });
                }
            }
            EmailMessage::ConfirmAutocomplete => {
                let matches = self.get_autocomplete_matches();
                if let Some(idx) = self.selected_autocomplete_idx {
                    if let Some(contact) = matches.get(idx) {
                        self.compose_to = contact.clone();
                        self.selected_autocomplete_idx = None;
                    }
                }
            }
            EmailMessage::AttachmentsPicked(paths) => {
                for p in paths {
                    self.compose_attachments.push(p);
                }
            }
            EmailMessage::AttachmentPicked(p) => self.compose_attachments.push(p),
            EmailMessage::RemoveAttachment(i) => {
                self.compose_attachments.remove(i);
            }
            EmailMessage::SendMail => {
                self.is_composing = false;
                self.compose_body = iced::widget::text_editor::Content::new();
                self.active_compose_field = 0;
                self.compose_to.clear();
                self.compose_subject.clear();
                self.compose_attachments.clear();
                // Send path: stay 3-pane. The ✓ Message sent confirmation
                // (delivered as MessageLoaded after the async send) renders
                // there. Clear reply flag so a later Enter isn't misread.
                self.reply_from_fullview = false;
            }
            EmailMessage::CancelCompose => {
                let was_reply = self.reply_from_fullview;
                self.is_composing = false;
                self.compose_body = iced::widget::text_editor::Content::new();
                self.active_compose_field = 0;
                self.compose_to.clear();
                self.compose_subject.clear();
                self.compose_attachments.clear();
                self.reply_from_fullview = false;
                // Cancel path: restore fullview if we were replying from it.
                if was_reply && self.parsed_email.is_some() {
                    self.fullscreen_read = true;
                }
            }
            EmailMessage::OpenLogin => {
                let cfg = crate::libraries::backend::email::client::load_config();
                self.login_email = cfg.email;
                self.login_password = cfg.password.unwrap_or_default();
                self.login_imap = cfg.imap_host;
                self.login_smtp = cfg.smtp_host;
                self.login_sops_key = cfg.sops_key.unwrap_or_default();
                self.is_logging_in = true;
            }
            EmailMessage::DismissLogin => {
                self.is_logging_in = false;
            }
            EmailMessage::LoginEmailChanged(v) => {
                self.login_email = v;
                let domain = self
                    .login_email
                    .split('@')
                    .nth(1)
                    .unwrap_or("")
                    .to_lowercase();
                if domain == "gmail.com" || domain == "googlemail.com" {
                    self.login_imap = "imap.gmail.com".into();
                    self.login_smtp = "smtp.gmail.com".into();
                }
            }
            EmailMessage::LoginPasswordChanged(v) => self.login_password = v,
            EmailMessage::LoginImapChanged(v) => self.login_imap = v,
            EmailMessage::LoginSmtpChanged(v) => self.login_smtp = v,
            EmailMessage::ToggleSops(v) => self.use_sops = v,
            EmailMessage::LoginSopsKeyChanged(v) => self.login_sops_key = v,
            EmailMessage::SaveLogin => {}
            EmailMessage::Dismiss => {
                self.is_composing = false;
            }
            EmailMessage::ToggleFullscreen => {
                if self.parsed_email.is_some() {
                    self.fullscreen_read = !self.fullscreen_read;
                }
            }
            EmailMessage::ExitFullscreen => {
                self.fullscreen_read = false;
            }
            EmailMessage::GetAppPassword => {}
            EmailMessage::ToggleAdvancedServers => {
                self.show_advanced_servers = !self.show_advanced_servers
            }
            EmailMessage::StartAddFolder => {
                self.is_adding_folder = true;
                self.new_folder_name.clear();
            }
            EmailMessage::AddFolderChanged(v) => self.new_folder_name = v,
            EmailMessage::SubmitAddFolder => {
                let name = self.new_folder_name.trim().to_string();
                if !name.is_empty() && !self.folders.contains(&name) {
                    self.folders.push(name);
                    let aliases = [
                        "INBOX", "Unread", "Starred", "Sent", "Drafts", "Trash", "Spam",
                    ];
                    let user_folders: Vec<String> = self
                        .folders
                        .iter()
                        .filter(|f| !aliases.contains(&f.as_str()))
                        .cloned()
                        .collect();
                    crate::libraries::backend::email::client::save_folders_cache(&user_folders);
                }
                self.is_adding_folder = false;
            }
            EmailMessage::RemoveFolder(f) => {
                if f != "INBOX"
                    && f != "Unread"
                    && f != "Starred"
                    && f != "Sent"
                    && f != "Drafts"
                    && f != "Trash"
                    && f != "Spam"
                {
                    self.folders.retain(|x| x != &f);
                    if self.active_folder == f {
                        self.active_folder = "INBOX".into();
                        self.active_folder_idx = 0;
                        self.selected_index = 0;
                    }
                }
            }
            EmailMessage::FocusNextPane => {
                self.focus_pane = (self.focus_pane + 1).min(2);
            }
            EmailMessage::FocusPrevPane => {
                self.focus_pane = self.focus_pane.saturating_sub(1);
            }
            EmailMessage::OpenInBrowser => {
                if let Some(id) = &self.active_mail_id {
                    let cache_file = dirs::data_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                        .join("moonbeauty/email_cache")
                        .join(&self.active_folder)
                        .join(format!("{}.eml", id));
                    if cache_file.exists() {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(cache_file)
                            .spawn();
                    }
                }
            }
            EmailMessage::SaveAttachment(name) => {
                if let Some(email) = &self.parsed_email {
                    if let Some((_, data)) = email.attachments.iter().find(|(n, _)| n == &name) {
                        // Sanitize: keep only the basename, strip leading dots.
                        // Otherwise `../../.bashrc` from a malicious email would
                        // overwrite the user's shell init.
                        let safe_name: String = std::path::Path::new(&name)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .map(|s| s.trim_start_matches('.').to_string())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "attachment".to_string());
                        let settings = crate::libraries::config::settings::ShellSettings::load();
                        let home =
                            dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
                        let download_dir = if !settings.attachment_download_dir.trim().is_empty() {
                            std::path::PathBuf::from(settings.attachment_download_dir.trim())
                        } else {
                            // Prefer XDG download dir, then ~/Downloads, then ~
                            let xdg = dirs::download_dir();
                            let fallback_dl = home.join("Downloads");
                            match xdg {
                                Some(d) => d,
                                None if fallback_dl.exists() => fallback_dl,
                                None => {
                                    // Create ~/Downloads if it doesn't exist
                                    let _ = std::fs::create_dir_all(&fallback_dl);
                                    fallback_dl
                                }
                            }
                        };
                        let _ = std::fs::create_dir_all(&download_dir);
                        let out_path = download_dir.join(&safe_name);
                        crate::dbg_log!("[attach] saving to {:?}", out_path);
                        if std::fs::write(&out_path, data).is_ok() {
                            // Finding 25: strip exec bits and gate auto-open
                            // on a script/exec extension denylist.
                            {
                                use std::os::unix::fs::PermissionsExt;
                                if let Ok(meta) = std::fs::metadata(&out_path) {
                                    let mut perms = meta.permissions();
                                    perms.set_mode(0o644);
                                    let _ = std::fs::set_permissions(&out_path, perms);
                                }
                            }
                            if is_safe_to_auto_open(&out_path) {
                                let _ = std::process::Command::new("notify-send")
                                    .args([
                                        "-a",
                                        "Email",
                                        "Attachment Saved",
                                        &format!("Saved to {}", out_path.display()),
                                    ])
                                    .spawn();
                                let _ = std::process::Command::new("xdg-open")
                                    .arg(&out_path)
                                    .spawn();
                            } else {
                                let _ = std::process::Command::new("notify-send")
                                    .args([
                                        "-a",
                                        "Email",
                                        "Attachment Saved",
                                        &format!(
                                            "Saved (not auto-opened for safety) → {}",
                                            out_path.display()
                                        ),
                                    ])
                                    .spawn();
                            }
                        } else {
                            let _ = std::process::Command::new("notify-send")
                                .args([
                                    "-a",
                                    "Email",
                                    "Save Failed",
                                    &format!("Could not write to {}", out_path.display()),
                                ])
                                .spawn();
                        }
                    }
                }
            }
            EmailMessage::SelectMailNext => {
                if !self.envelopes.is_empty() {
                    self.selected_index = (self.selected_index + 1).min(self.envelopes.len() - 1);
                }
            }
            EmailMessage::SelectMailPrev => {
                if !self.envelopes.is_empty() {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
            }
            EmailMessage::SelectFolderNext => {
                self.active_folder_idx = (self.active_folder_idx + 1) % self.folders.len();
                self.active_folder = self.folders[self.active_folder_idx].clone();
                self.selected_index = 0;
                self.active_mail_id = None;
                self.parsed_email = None;
            }
            EmailMessage::SelectFolderPrev => {
                self.active_folder_idx = self.active_folder_idx.saturating_sub(1);
                self.active_folder = self.folders[self.active_folder_idx].clone();
                self.selected_index = 0;
                self.active_mail_id = None;
                self.parsed_email = None;
            }
            EmailMessage::OpenSelected => {
                if let Some(mail) = self.envelopes.get(self.selected_index) {
                    self.active_mail_id = Some(mail.id.clone());
                    self.parsed_email = None;
                }
            }
            EmailMessage::ToggleStar => {
                if let Some(mail) = self.envelopes.get_mut(self.selected_index) {
                    mail.is_starred = !mail.is_starred;
                }
            }
            EmailMessage::DeleteSelected | EmailMessage::SaveContact => {}
            EmailMessage::CopyGmailFilter => { /* handled in app/update.rs for clipboard access */ }
            EmailMessage::MarkSpam | EmailMessage::MarkNotSpam => { /* handled in app/update.rs */ }
            EmailMessage::DraftSaved => {
                self.imap_error =
                    Some("Draft saved to ~/.local/share/moonbeauty/drafts.txt".into());
            }
        }
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &'a ShellSettings,
    ) -> Element<'a, EmailMessage> {
        let (win_w, win_h) = settings.get_window_size("email", 1280, 800);
        let fs_main = theme.font_size_overlay as f32;
        let fs_sub = (theme.font_size_overlay as f32) - 2.0;
        let fs_head = (theme.font_size_overlay as f32) + 4.0;

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let header_inset = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let footer_inset = if dy > pad_bottom {
            (dx * (1.0 - (pad_bottom / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let body_inset = header_inset.min(footer_inset) + 4.0;

        let card_bg = ShapeBox::new()
            .shape(ShapeType::AngledHexagon {
                cut: cut_f,
                angle_deg,
            })
            .background(theme.background)
            .border(theme.border, theme.border_width);

        if self.is_logging_in {
            let email_input = text_input("you@gmail.com", &self.login_email)
                .on_input(EmailMessage::LoginEmailChanged)
                .padding(10);
            let pass_input = text_input("App Password", &self.login_password)
                .on_input(EmailMessage::LoginPasswordChanged)
                .secure(true)
                .padding(10);
            let imap_input = text_input("imap.gmail.com", &self.login_imap)
                .on_input(EmailMessage::LoginImapChanged)
                .padding(10);
            let smtp_input = text_input("smtp.gmail.com", &self.login_smtp)
                .on_input(EmailMessage::LoginSmtpChanged)
                .padding(10);

            let sops_check = checkbox(self.use_sops)
                .label("Encrypt with SOPS")
                .on_toggle(EmailMessage::ToggleSops)
                .size(16)
                .text_size(fs_main);

            let sops_key_input = if self.use_sops {
                Some(column![
                    text_input("Age Key (age1... or path/to/keys.txt)", &self.login_sops_key).on_input(EmailMessage::LoginSopsKeyChanged).padding(10),
                    text("Manual CLI equivalent: sops --encrypt --age <key> (or --pgp <fingerprint>) email.json").size(11).color(theme.muted)
                ].spacing(4))
            } else {
                None
            };

            let mut form_col = column![
                text("Connect Email Account")
                    .size(fs_head)
                    .bold()
                    .color(theme.text),
                text("Enter your email and app password.").color(theme.muted),
                email_input,
                pass_input,
                button(
                    container(
                        text("Get Google App Password ↗")
                            .size(12)
                            .color(theme.background)
                    )
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                )
                .on_press(EmailMessage::GetAppPassword)
                .width(Length::Fill)
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.success)),
                    border: iced::Border {
                        color: theme.success,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                })
                .padding(8),
            ]
            .spacing(12);

            if self.show_advanced_servers {
                form_col = form_col.push(imap_input).push(smtp_input);
            }

            form_col = form_col.push(
                button(
                    text(if self.show_advanced_servers {
                        "Hide Manual Setup"
                    } else {
                        "Manual Server Setup"
                    })
                    .size(12)
                    .color(theme.muted),
                )
                .on_press(EmailMessage::ToggleAdvancedServers)
                .style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
                .padding(0),
            );

            form_col = form_col.push(sops_check);
            if let Some(k) = sops_key_input {
                form_col = form_col.push(k);
            }

            form_col = form_col.push(if let Some(err) = &self.login_error {
                text(err).color(theme.danger)
            } else {
                text("").color(theme.danger)
            });

            form_col = form_col.push(
                row![
                    button(text("Save & Login").color(theme.background))
                        .on_press(EmailMessage::SaveLogin)
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.success)),
                            ..Default::default()
                        })
                        .padding(10),
                    button(text("Cancel").color(theme.text))
                        .on_press(EmailMessage::DismissLogin)
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            ..Default::default()
                        })
                        .padding(10),
                ]
                .spacing(10),
            );

            let login_form = form_col.spacing(16).padding(40).width(Length::Fixed(500.0));
            let card_content = container(login_form)
                .width(Length::Fixed(win_w))
                .height(Length::Fixed(win_h))
                .center_x(Length::Fill)
                .center_y(Length::Fill);
            return container(Stack::new().push(card_bg.view()).push(card_content))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        }

        // Main Mail UI
        let mut folder_col = column![].spacing(4);
        for f in &self.folders {
            let is_sel = &self.active_folder == f;
            let col = if is_sel { theme.background } else { theme.text };
            let bg = if is_sel {
                Some(iced::Background::Color(theme.success))
            } else {
                None
            };
            let f_clone = f.clone();

            let f_btn = button(text(f).size(fs_main).bold().color(col))
                .on_press(EmailMessage::SelectFolder(f_clone))
                .padding([8, 12])
                .width(Length::Fill)
                .style(move |_t, _s| button::Style {
                    background: bg,
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });
            folder_col = folder_col.push(f_btn);
        }

        if self.is_adding_folder {
            let input = text_input("Folder name...", &self.new_folder_name)
                .on_input(EmailMessage::AddFolderChanged)
                .on_submit(EmailMessage::SubmitAddFolder)
                .padding(8);
            folder_col = folder_col.push(input);
        } else {
            let add_btn = button(text("+").size(fs_main).bold().color(theme.background))
                .on_press(EmailMessage::StartAddFolder)
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.success)),
                    ..Default::default()
                })
                .padding(8)
                .width(Length::FillPortion(1));

            let rem_btn = button(text("-").size(fs_main).bold().color(theme.background))
                .on_press(EmailMessage::RemoveFolder(self.active_folder.clone()))
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.danger)),
                    ..Default::default()
                })
                .padding(8)
                .width(Length::FillPortion(1));

            let copy_filter_btn = button(text("📋").size(fs_main).color(theme.background))
                .on_press(EmailMessage::CopyGmailFilter)
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.text)),
                    ..Default::default()
                })
                .padding(8)
                .width(Length::FillPortion(1));

            folder_col = folder_col.push(row![add_btn, rem_btn, copy_filter_btn].spacing(4));
        }

        let folder_border = if self.focus_pane == 0 {
            theme.success
        } else {
            theme.border
        };
        let folder_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: 16 })
            .background(theme.surface_alt)
            .border(folder_border, theme.control_border_width)
            .size(Length::Fixed(180.0), Length::Fill);

        let folder_pane = Stack::new().push(folder_bg.view()).push(
            container(folder_col)
                .width(Length::Fixed(180.0))
                .padding(20),
        );

        let mut mail_col = column![].spacing(6);
        if self.is_loading {
            mail_col =
                mail_col.push(container(text("Loading emails...").color(theme.muted)).padding(10));
        } else if let Some(err) = &self.imap_error {
            mail_col = mail_col.push(
                container(
                    column![
                        text("IMAP Error").color(theme.danger),
                        text(err).color(theme.text),
                        button(text("Reconfigure").size(12).color(theme.background))
                            .on_press(EmailMessage::OpenLogin)
                            .style(move |_t, _s| button::Style {
                                background: Some(iced::Background::Color(theme.danger)),
                                border: iced::Border {
                                    color: theme.danger,
                                    width: 1.0,
                                    radius: 4.0.into()
                                },
                                ..Default::default()
                            })
                            .padding(8)
                    ]
                    .spacing(10),
                )
                .padding(10),
            );
        } else if self.envelopes.is_empty() {
            mail_col =
                mail_col.push(container(text("No messages found.").color(theme.muted)).padding(10));
        } else {
            for (i, mail) in self.envelopes.iter().enumerate() {
                let id = mail.id.clone();
                let is_sel =
                    self.active_mail_id.as_deref() == Some(&id) || i == self.selected_index;
                let text_color = if is_sel {
                    theme.text_strong
                } else {
                    theme.text
                };
                let bg_color = if is_sel {
                    Some(iced::Background::Color(theme.surface_alt))
                } else {
                    None
                };

                let star_icon = if mail.is_starred { "★" } else { "☆" };
                let star_col = if mail.is_starred {
                    theme.success
                } else {
                    theme.muted
                };

                let row_item = button(
                    row![
                        text(star_icon).size(fs_main).color(star_col),
                        column![
                            text(&mail.sender)
                                .size(fs_main)
                                .bold()
                                .color(text_color)
                                .wrapping(iced::widget::text::Wrapping::None),
                            text(&mail.subject)
                                .size(fs_sub)
                                .color(theme.muted)
                                .wrapping(iced::widget::text::Wrapping::None),
                        ]
                        .spacing(2)
                        .width(Length::Fill)
                    ]
                    .spacing(8)
                    .align_y(iced::alignment::Vertical::Center)
                    .width(Length::Fill),
                )
                .on_press(EmailMessage::SelectMail(id))
                .padding(10)
                .width(Length::Fill)
                .style(move |_t, _s| button::Style {
                    background: bg_color,
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });
                mail_col = mail_col.push(row_item);
            }
        }

        let mail_border = if self.focus_pane == 1 {
            theme.success
        } else {
            theme.border
        };
        let mail_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: 16 })
            .background(theme.surface_alt)
            .border(mail_border, theme.control_border_width)
            .size(Length::Fixed(320.0), Length::Fill);

        let mail_pane = Stack::new().push(mail_bg.view()).push(
            container(
                scrollable(mail_col)
                    .id(iced::widget::Id::new("email_mail_list"))
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fixed(320.0))
            .height(Length::Fill)
            .padding(iced::Padding {
                top: 20.0,
                right: 36.0,
                bottom: 20.0,
                left: 20.0,
            })
            .clip(true),
        );

        let preview_content: Element<'a, EmailMessage> = if self.is_composing {
            let to_input = text_input("To:", &self.compose_to)
                .id(COMPOSE_TO_ID)
                .on_input(EmailMessage::ToChanged)
                .padding(8);

            let subj_input = text_input("Subject:", &self.compose_subject)
                .id(COMPOSE_SUBJ_ID)
                .on_input(EmailMessage::SubjectChanged)
                .padding(8);

            let body_input = iced::widget::text_editor(&self.compose_body)
                .id(COMPOSE_BODY_ID)
                .on_action(EmailMessage::BodyChanged)
                .padding(8)
                .height(Length::Fill);

            let suggestions = if !self.compose_to.is_empty() {
                let q = self.compose_to.to_lowercase();
                let matches: Vec<String> = self
                    .contacts
                    .iter()
                    .filter(|c| c.to_lowercase().contains(&q) && c.to_lowercase() != q)
                    .take(5)
                    .cloned()
                    .collect();

                if !matches.is_empty() {
                    let mut col = column![].spacing(2).width(Length::Fill);
                    for (i, m) in matches.into_iter().enumerate() {
                        let is_sel = self.selected_autocomplete_idx == Some(i);
                        let bg = if is_sel {
                            theme.success
                        } else {
                            theme.surface_alt
                        };
                        let fg = if is_sel { theme.background } else { theme.text };
                        col = col.push(
                            button(text(m.clone()).size(fs_main).color(fg))
                                .on_press(EmailMessage::SelectContact(m.clone()))
                                .style(move |_t, _s| button::Style {
                                    background: Some(iced::Background::Color(bg)),
                                    border: iced::Border {
                                        color: theme.border,
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                })
                                .padding(4)
                                .width(Length::Fill),
                        );
                    }
                    Some(col)
                } else {
                    None
                }
            } else {
                None
            };

            let mut attach_row = row![].spacing(6);
            for (i, p) in self.compose_attachments.iter().enumerate() {
                if let Some(name) = p.file_name() {
                    attach_row = attach_row.push(
                        button(
                            text(format!("📎 {} ✕", name.to_string_lossy()))
                                .size(11)
                                .color(theme.background),
                        )
                        .on_press(EmailMessage::RemoveAttachment(i))
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.warning)),
                            ..Default::default()
                        })
                        .padding(4),
                    );
                }
            }

            // Fallback: manual file path input for drag-and-drop users
            let manual_attach = row![
                text_input("Or paste file path here...", &self.compose_manual_attach)
                    .on_input(EmailMessage::ManualAttachChanged)
                    .on_submit(EmailMessage::SubmitManualAttach)
                    .padding(6)
                    .size(12)
                    .width(Length::Fill),
                button(text("Add").size(12).color(theme.background))
                    .on_press(EmailMessage::SubmitManualAttach)
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.warning)),
                        ..Default::default()
                    })
                    .padding(6)
            ]
            .spacing(4)
            .width(Length::Fill);

            let compose_heading = if self.reply_from_fullview {
                "↩ REPLY"
            } else {
                "✉ NEW MESSAGE"
            };
            let mut compose_col = column![
                text(compose_heading).size(fs_main).color(theme.success),
                to_input,
            ]
            .spacing(10);

            if let Some(s) = suggestions {
                compose_col =
                    compose_col.push(container(s).padding(4).style(move |_t| container::Style {
                        background: Some(iced::Background::Color(theme.background)),
                        border: iced::Border {
                            color: theme.border,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }));
            }

            compose_col = compose_col.push(subj_input);
            compose_col = compose_col.push(body_input);
            compose_col = compose_col.push(attach_row);
            compose_col = compose_col.push(manual_attach);
            compose_col = compose_col.push(
                row![
                    button(text("Send").color(theme.background))
                        .on_press(EmailMessage::SendMail)
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.success)),
                            ..Default::default()
                        })
                        .padding(8),
                    button(text("📎 Attach").color(theme.background))
                        .on_press(EmailMessage::AddAttachment)
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.warning)),
                            ..Default::default()
                        })
                        .padding(8),
                    button(text("Cancel").color(theme.text))
                        .on_press(EmailMessage::CancelCompose)
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            ..Default::default()
                        })
                        .padding(8),
                    text("Ctrl+S / Ctrl+Enter to send")
                        .size(11)
                        .color(theme.muted),
                ]
                .spacing(10)
                .align_y(iced::alignment::Vertical::Center),
            );

            compose_col
                .padding(20)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else if let Some(email) = &self.parsed_email {
            let mut body_col = column![].spacing(16);

            // Action bar for the currently-viewed message
            let in_spam = self.active_folder == "Spam" || self.active_folder == "[Gmail]/Spam";
            let spam_label = if in_spam { "Not Spam" } else { "Mark Spam" };
            let spam_msg = if in_spam {
                EmailMessage::MarkNotSpam
            } else {
                EmailMessage::MarkSpam
            };
            let action_bar = row![
                button(
                    text(spam_label)
                        .size((theme.font_size_normal) as f32)
                        .color(theme.background)
                )
                .on_press(spam_msg)
                .padding([6, 14])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.warning)),
                    border: iced::Border {
                        color: theme.warning,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }),
                button(
                    text("Move to Trash")
                        .size((theme.font_size_normal) as f32)
                        .color(theme.background)
                )
                .on_press(EmailMessage::DeleteSelected)
                .padding([6, 14])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.danger)),
                    border: iced::Border {
                        color: theme.danger,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }),
            ]
            .spacing(8);
            body_col = body_col.push(action_bar);

            // Render Images
            for img_path in &email.images {
                body_col = body_col.push(
                    iced::widget::image(iced::widget::image::Handle::from_path(img_path))
                        .width(Length::Fill)
                        .content_fit(iced::ContentFit::Contain),
                );
            }

            // Render Text
            body_col = body_col.push(
                text(&email.text)
                    .size(fs_main)
                    .color(theme.text)
                    .font(iced::Font::MONOSPACE)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );

            // Render Attachments
            if !email.attachments.is_empty() {
                body_col = body_col.push(
                    text("Attachments (Click to Preview/Open):")
                        .size(fs_main)
                        .bold()
                        .color(theme.success),
                );
                let mut att_row = row![].spacing(6);
                for (name, _data) in &email.attachments {
                    att_row = att_row.push(
                        button(text(format!("📎 Save {}", name)).color(theme.background))
                            .on_press(EmailMessage::SaveAttachment(name.clone()))
                            .style(move |_t, _s| button::Style {
                                background: Some(iced::Background::Color(theme.warning)),
                                ..Default::default()
                            })
                            .padding(6),
                    );
                }
                body_col = body_col.push(att_row);
            }

            scrollable(container(body_col).padding(20).width(Length::Fill))
                .height(Length::Fill)
                .width(Length::Fill)
                .into()
        } else {
            let display_text = if self.active_mail_id.is_some() {
                "Loading..."
            } else {
                "No message loaded. Select an email with ↑/↓."
            };
            scrollable(
                container(text(display_text).size(fs_main).color(theme.muted))
                    .padding(20)
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
        };

        let preview_border = if self.focus_pane == 2 {
            theme.success
        } else {
            theme.border
        };
        let preview_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut: 16 })
            .background(theme.surface_alt)
            .border(preview_border, theme.control_border_width)
            .size(Length::Fill, Length::Fill);

        let preview_pane = Stack::new().push(preview_bg.view()).push(
            container(preview_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(20),
        );

        // Fullscreen: single-column preview with a back button.
        // 3-pane: folders | list | preview (existing layout).
        let main_row: Element<'a, EmailMessage> = if self.fullscreen_read
            && self.parsed_email.is_some()
            && !self.is_composing
            && !self.is_logging_in
        {
            let (cur_subject, cur_sender) = self
                .envelopes
                .iter()
                .find(|e| Some(&e.id) == self.active_mail_id.as_ref())
                .map(|e| (e.subject.clone(), e.sender.clone()))
                .unwrap_or_default();
            let back_btn = button(text("\u{2190} Back (Esc)").size(fs_sub).color(theme.text))
                .on_press(EmailMessage::ExitFullscreen)
                .padding([6, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.surface_alt)),
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                });
            let fullscreen_header = container(
                row![
                    back_btn,
                    iced::widget::space::horizontal(),
                    text(cur_subject).size(fs_head).bold().color(theme.text),
                    iced::widget::space::horizontal(),
                    text(cur_sender).size(fs_sub).color(theme.muted),
                    text("· Enter to reply ·").size(fs_sub).color(theme.muted),
                    button(text("↩ Reply").size(fs_sub).color(theme.text),)
                        .on_press(EmailMessage::OpenSelected)
                        .padding([4, 8])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.surface_alt)),
                            border: iced::Border {
                                color: theme.border,
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                ]
                .spacing(16)
                .align_y(iced::alignment::Vertical::Center)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: body_inset,
                bottom: 0.0,
                left: body_inset,
            });
            column![fullscreen_header, preview_pane]
                .spacing(12)
                .height(Length::Fill)
                .into()
        } else {
            let layout = row![folder_pane, mail_pane, preview_pane]
                .spacing(16)
                .height(Length::Fill);
            layout.into()
        };

        let contact_btn = if !self.is_composing && self.active_mail_id.is_some() {
            button(text("Save Contact").size(fs_sub).color(theme.text))
                .on_press(EmailMessage::SaveContact)
                .padding([4, 8])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.surface_alt)),
                    border: iced::Border {
                        color: theme.border,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                })
        } else {
            button(text("")).padding(0).style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
        };

        let browser_btn = if !self.is_composing && self.active_mail_id.is_some() {
            button(
                text("🌐 View in Browser")
                    .size(fs_sub)
                    .color(theme.background),
            )
            .on_press(EmailMessage::OpenInBrowser)
            .padding([4, 8])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(theme.warning)),
                border: iced::Border {
                    color: theme.warning,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
        } else {
            button(text("")).padding(0).style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
        };

        let header_row = row![
            text("✉ EMAIL CLIENT")
                .size(fs_head)
                .bold()
                .color(theme.text),
            iced::widget::space::horizontal(),
            browser_btn,
            contact_btn,
            button(text("⚙ Settings").size(fs_main).bold().color(theme.text))
                .on_press(EmailMessage::OpenLogin)
                .padding([6, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.surface_alt)),
                    border: iced::Border {
                        color: theme.text,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                }),
            if self.parsed_email.is_some() && !self.is_composing && !self.fullscreen_read {
                button(text("⛶ Full View (Enter)").size(fs_sub).color(theme.text))
                    .on_press(EmailMessage::ToggleFullscreen)
                    .padding([4, 8])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.surface_alt)),
                        border: iced::Border {
                            color: theme.border,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    })
            } else {
                button(text("")).padding(0).style(|_t, _s| button::Style {
                    background: None,
                    ..Default::default()
                })
            },
            button(text("✏ Compose").size(fs_main).bold().color(theme.success))
                .on_press(EmailMessage::OpenComposer)
                .padding([6, 12])
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(theme.surface_alt)),
                    border: iced::Border {
                        color: theme.success,
                        width: 1.0,
                        radius: 4.0.into()
                    },
                    ..Default::default()
                }),
        ]
        .align_y(iced::alignment::Vertical::Center)
        .spacing(10);

        let footer = container(
            text("Ctrl+↑/↓: Folders  •  ↑/↓: Navigate  •  Enter: Open  •  n: New  •  s: Star  •  d: Delete  •  c: Save Contact")
                .size(11)
                .color(theme.muted)
        ).width(Length::Fill).align_x(iced::alignment::Horizontal::Center);

        let header_container = container(header_row)
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: header_inset,
                bottom: 0.0,
                left: header_inset,
            });

        let main_row_container = container(main_row)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: body_inset,
                bottom: 0.0,
                left: body_inset,
            });

        let footer_container = container(footer)
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: footer_inset,
                bottom: 0.0,
                left: footer_inset,
            });

        let card_content = container(
            column![header_container, main_row_container, footer_container]
                .spacing(16)
                .padding(iced::Padding {
                    top: pad_top,
                    right: 0.0,
                    bottom: pad_bottom,
                    left: 0.0,
                })
                .width(Length::Fixed(win_w))
                .height(Length::Fixed(win_h)),
        );

        let card = Stack::new().push(card_bg.view()).push(card_content);
        container(card)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
