use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input, Stack};
use iced::{Element, Length};
use std::collections::HashMap;

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{AngledHexGeometry, ShapeBox, ShapeType};
use crate::libraries::style::TextExt;

pub const COMPOSE_TO_ID: &str = "compose_to";
pub const COMPOSE_SUBJ_ID: &str = "compose_subj";
pub const COMPOSE_BODY_ID: &str = "compose_body";
pub const COMPOSE_MANUAL_ATTACH_ID: &str = "compose_manual_attach";
pub const EMAIL_SEARCH_ID: &str = "email_search";

/// Finding 25: reject auto-opening attachments whose extension could
/// yield code execution when double-clicked on a default config.
/// A hostile sender delivering `invoice.pdf.desktop` looks like a PDF to
/// the user but routes xdg-open → desktop file Exec= → RCE.
fn is_safe_to_auto_open(path: &std::path::Path) -> bool {
    // Finding 25: default-deny allowlist. Only known-safe media/doc
    // formats auto-open. Replaces a denylist that could not anticipate
    // new macro formats (.docm/.xlsm) or multi-step archives (.tar.gz).
    const ALLOW_EXT: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "svg",
        "pdf", "txt", "md", "csv",
        "mp3", "mp4", "ogg", "webm", "flac",
    ];
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => ALLOW_EXT.iter().any(|a| ext.eq_ignore_ascii_case(a)),
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
    MessageLoadedFor(String, String, crate::libraries::backend::email::client::ParsedEmail),
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
    ReplySelected,
    EditDraft(String),
    OpenSearch,
    SearchChanged(String),
    CloseSearch,
    OpenLink(String),
    ToggleStar,
    DeleteSelected,
    SaveContact,
    ContactSaved(String),
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
    pub editing_draft_id: Option<String>,
    pub is_searching: bool,
    pub search_query: String,
    pub status_notice: Option<String>,
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
            editing_draft_id: None,
            is_searching: false,
            search_query: String::new(),
            status_notice: None,
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
            editing_draft_id: None,
            is_searching: false,
            search_query: String::new(),
            status_notice: None,
            parsed_cache: HashMap::new(),
        }
    }

    pub fn visible_envelopes(&self) -> Vec<(usize, &crate::libraries::backend::email::client::NativeEnvelope)> {
        let q = self.search_query.trim().to_lowercase();
        self.envelopes
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                if q.is_empty() {
                    return true;
                }
                if m.subject.to_lowercase().contains(&q)
                    || m.sender.to_lowercase().contains(&q)
                    || m.sender_email.to_lowercase().contains(&q)
                {
                    return true;
                }
                let cache_key = format!("{}|{}", self.active_folder, m.id);
                self.parsed_cache
                    .get(&cache_key)
                    .is_some_and(|p| p.text.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn select_after_removal(&mut self, vis_pos: Option<usize>) -> Option<String> {
        self.parsed_email = None;
        self.active_mail_id = None;
        self.fullscreen_read = false;
        let vis_after: Vec<usize> = self
            .visible_envelopes()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        if vis_after.is_empty() {
            self.selected_index = 0;
            None
        } else {
            let target_pos = vis_pos.map(|p| p.min(vis_after.len() - 1)).unwrap_or(0);
            self.selected_index = vis_after[target_pos];
            self.envelopes
                .get(self.selected_index)
                .map(|m| m.id.clone())
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
                self.editing_draft_id = None;
                self.status_notice = None;
            }
            EmailMessage::SelectMail(id) => {
                // Switching mails exits fullscreen.
                self.fullscreen_read = false;
                self.status_notice = None;
                
                // Sync the selected_index so keyboard navigation follows mouse clicks
                if let Some(idx) = self.envelopes.iter().position(|m| m.id == id) {
                    self.selected_index = idx;
                }

                let same_and_loaded = self.active_mail_id.as_deref() == Some(id.as_str())
                    && self.parsed_email.is_some();
                if !same_and_loaded {
                    self.active_mail_id = Some(id.clone());
                    self.parsed_email = None; // Force clear the old preview
                    // Cache key must include folder because UIDs are not globally unique
                    let cache_key = format!("{}|{}", self.active_folder, id);
                    if let Some(cached) = self.parsed_cache.get(&cache_key).cloned() {
                        self.parsed_email = Some(cached);
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
            EmailMessage::MessageLoadedFor(folder, id, b) => {
                let cache_key = format!("{}|{}", folder, id);
                if self.parsed_cache.len() >= 5 && !self.parsed_cache.contains_key(&cache_key) {
                    if let Some(oldest) = self.parsed_cache.keys().next().cloned() {
                        self.parsed_cache.remove(&oldest);
                    }
                }
                self.parsed_cache.insert(cache_key, b.clone());
                if self.active_folder == folder
                    && self.active_mail_id.as_deref() == Some(id.as_str())
                {
                    self.parsed_email = Some(b);
                }
            }
            EmailMessage::MessageLoaded(b) => {
                let trimmed = b.text.trim_start();
                let is_synthetic = trimmed.starts_with('✓') || trimmed.starts_with('✗');
                if !is_synthetic {
                    if let Some(id) = self.active_mail_id.clone() {
                        let cache_key = format!("{}|{}", self.active_folder, id);
                        if self.parsed_cache.len() >= 5 && !self.parsed_cache.contains_key(&cache_key) {
                            if let Some(oldest) = self.parsed_cache.keys().next().cloned() {
                                self.parsed_cache.remove(&oldest);
                            }
                        }
                        self.parsed_cache.insert(cache_key, b.clone());
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
                self.editing_draft_id = None;
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
                if let Some(draft_id) = self.editing_draft_id.take() {
                    let _ = crate::libraries::backend::email::client::delete_draft(&draft_id);
                    if self.active_folder == "Drafts" {
                        self.envelopes = crate::libraries::backend::email::client::load_drafts();
                        if self.selected_index >= self.envelopes.len() && !self.envelopes.is_empty() {
                            self.selected_index = self.envelopes.len() - 1;
                        }
                        self.active_mail_id = None;
                        self.parsed_email = None;
                        self.parsed_cache.clear();
                    }
                }
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
                if let Some(draft_id) = self.editing_draft_id.take() {
                    let _ = crate::libraries::backend::email::client::delete_draft(&draft_id);
                }
                self.save_current_draft();
                let was_reply = self.reply_from_fullview;
                self.is_composing = false;
                self.compose_body = iced::widget::text_editor::Content::new();
                self.active_compose_field = 0;
                self.compose_to.clear();
                self.compose_subject.clear();
                self.compose_attachments.clear();
                self.reply_from_fullview = false;
                if self.active_folder == "Drafts" {
                    self.envelopes = crate::libraries::backend::email::client::load_drafts();
                    self.selected_index = 0;
                    self.active_mail_id = None;
                    self.parsed_email = None;
                    self.parsed_cache.clear();
                } else if was_reply && self.parsed_email.is_some() {
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
                if self.is_composing {
                    self.save_current_draft();
                }
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
                        let out_path = unique_download_path(&download_dir, &safe_name);
                        crate::dbg_log!("[attach] saving to {:?}", out_path);
                        if crate::libraries::process::private_write_new(&out_path, data).is_ok() {
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
            EmailMessage::OpenSearch => {
                if !self.is_composing && !self.is_logging_in && !self.is_adding_folder {
                    self.is_searching = true;
                }
            }
            EmailMessage::SearchChanged(q) => {
                self.search_query = q;
            }
            EmailMessage::CloseSearch => {
                self.is_searching = false;
                self.search_query.clear();
            }
            EmailMessage::OpenLink(_) => {}
            EmailMessage::SelectMailNext => {
                let vis: Vec<usize> = self.visible_envelopes().into_iter().map(|(i, _)| i).collect();
                if let Some(first) = vis.first().copied() {
                    self.selected_index = match vis.iter().position(|&i| i == self.selected_index) {
                        Some(pos) => vis[(pos + 1).min(vis.len() - 1)],
                        None => first,
                    };
                }
            }
            EmailMessage::SelectMailPrev => {
                let vis: Vec<usize> = self.visible_envelopes().into_iter().map(|(i, _)| i).collect();
                if let Some(first) = vis.first().copied() {
                    self.selected_index = match vis.iter().position(|&i| i == self.selected_index) {
                        Some(pos) => vis[pos.saturating_sub(1)],
                        None => first,
                    };
                }
            }
            EmailMessage::SelectFolderNext => {
                if !self.folders.is_empty() {
                    self.active_folder_idx = (self.active_folder_idx + 1) % self.folders.len();
                    self.active_folder = self.folders[self.active_folder_idx].clone();
                    self.selected_index = 0;
                    self.active_mail_id = None;
                    self.parsed_email = None;
                    self.parsed_cache.clear();
                }
            }
            EmailMessage::SelectFolderPrev => {
                if !self.folders.is_empty() {
                    self.active_folder_idx = if self.active_folder_idx == 0 {
                        self.folders.len() - 1
                    } else {
                        self.active_folder_idx - 1
                    };
                    self.active_folder = self.folders[self.active_folder_idx].clone();
                    self.selected_index = 0;
                    self.active_mail_id = None;
                    self.parsed_email = None;
                    self.parsed_cache.clear();
                }
            }
            EmailMessage::OpenSelected => {
                if let Some(mail) = self.envelopes.get(self.selected_index) {
                    self.active_mail_id = Some(mail.id.clone());
                    self.parsed_email = None;
                }
            }
            EmailMessage::EditDraft(id) => {
                if let Some(mail) = self
                    .envelopes
                    .iter()
                    .find(|e| e.id == id)
                    .cloned()
                {
                    let body = if self.active_mail_id.as_deref() == Some(id.as_str()) {
                        self.parsed_email.as_ref().map(|p| p.text.clone())
                    } else {
                        None
                    }
                    .or_else(|| {
                        crate::libraries::backend::email::client::read_draft(&id).map(|p| p.text)
                    })
                    .unwrap_or_default();

                    let to = if !mail.sender_email.trim().is_empty() {
                        mail.sender_email
                    } else {
                        mail.sender
                    };
                    self.fullscreen_read = false;
                    self.is_composing = true;
                    self.reply_from_fullview = false;
                    self.editing_draft_id = Some(id);
                    self.compose_to = to;
                    self.compose_subject = mail.subject;
                    self.compose_body = iced::widget::text_editor::Content::with_text(&body);
                    self.compose_attachments.clear();
                    self.selected_autocomplete_idx = None;
                }
            }
            EmailMessage::ReplySelected => {
                if let Some(mail) = self
                    .envelopes
                    .iter()
                    .find(|e| Some(&e.id) == self.active_mail_id.as_ref())
                    .or_else(|| self.envelopes.get(self.selected_index))
                    .cloned()
                {
                    let to = if !mail.sender_email.trim().is_empty() {
                        mail.sender_email
                    } else {
                        mail.sender
                    };
                    let subj = if mail
                        .subject
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("re:")
                    {
                        mail.subject
                    } else {
                        format!("Re: {}", mail.subject)
                    };
                    self.fullscreen_read = false;
                    self.is_composing = true;
                    self.reply_from_fullview = true;
                    self.compose_to = to;
                    self.compose_subject = subj;
                    self.compose_body = iced::widget::text_editor::Content::new();
                    self.compose_attachments.clear();
                    self.selected_autocomplete_idx = None;
                }
            }
            EmailMessage::ToggleStar => {
                if let Some(mail) = self.envelopes.get_mut(self.selected_index) {
                    mail.is_starred = !mail.is_starred;
                }
            }
            EmailMessage::DeleteSelected | EmailMessage::SaveContact => {}
            EmailMessage::ContactSaved(email) => {
                if !email.is_empty()
                    && !self
                        .contacts
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(&email))
                {
                    self.contacts.push(email.clone());
                }
                self.status_notice = Some(format!("✓ Saved contact: {}", email));
            }
            EmailMessage::CopyGmailFilter => { /* handled in app/update.rs for clipboard access */ }
            EmailMessage::MarkSpam | EmailMessage::MarkNotSpam => { /* handled in app/update.rs */ }
            EmailMessage::DraftSaved => {
                self.status_notice =
                    Some("✓ Draft saved to ~/.local/share/moonbeauty/drafts.txt".into());
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
        let pad_top = 20.0_f32;
        let pad_bottom = 20.0_f32;
        let (header_inset, body_inset, footer_inset) =
            AngledHexGeometry::new(win_w, win_h, cut_f, angle_deg).standard_insets();

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
            let visible = self.visible_envelopes();
            if visible.is_empty() {
                mail_col = mail_col.push(
                    container(text("No matching emails.").color(theme.muted)).padding(10),
                );
            }
            for (i, mail) in visible {
                let id = mail.id.clone();
                // ONLY highlight the currently focused index.
                let is_sel = i == self.selected_index;
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
                let border_color = if is_sel { theme.text } else { theme.border };
                let border_width = if is_sel { 2.0 } else { 1.0 };

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
                        color: border_color,
                        width: border_width,
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
            } else if self.editing_draft_id.is_some() {
                "📝 EDIT DRAFT"
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
                        .on_press(EmailMessage::Dismiss)
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
            let is_draft = self.active_folder == "Drafts"
                || self
                    .active_mail_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("draft_"));
            let action_bar = if is_draft {
                let draft_id = self.active_mail_id.clone().unwrap_or_default();
                row![
                    button(
                        text("✎ Edit Draft")
                            .size((theme.font_size_normal) as f32)
                            .color(theme.background)
                    )
                    .on_press(EmailMessage::EditDraft(draft_id))
                    .padding([6, 14])
                    .style(move |_t, _s| button::Style {
                        background: Some(iced::Background::Color(theme.success)),
                        border: iced::Border {
                            color: theme.success,
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    }),
                    button(
                        text("Delete Draft")
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
                .spacing(8)
            } else {
                let in_spam = self.active_folder == "Spam" || self.active_folder == "[Gmail]/Spam";
                let spam_label = if in_spam { "Not Spam" } else { "Mark Spam" };
                let spam_msg = if in_spam {
                    EmailMessage::MarkNotSpam
                } else {
                    EmailMessage::MarkSpam
                };
                row![
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
                    if email.links.len() == 1 {
                        let first_url = email.links[0].1.clone();
                        Element::from(
                            button(
                                text("🔗 Open Link")
                                    .size((theme.font_size_normal) as f32)
                                    .color(theme.background),
                            )
                            .on_press(EmailMessage::OpenLink(first_url))
                            .padding([6, 14])
                            .style(move |_t, _s| button::Style {
                                background: Some(iced::Background::Color(theme.success)),
                                border: iced::Border {
                                    color: theme.success,
                                    width: 1.0,
                                    radius: 4.0.into(),
                                },
                                ..Default::default()
                            }),
                        )
                    } else if !email.links.is_empty() {
                        Element::from(
                            text(format!("🔗 {} links below", email.links.len()))
                                .size(fs_sub)
                                .color(theme.success),
                        )
                    } else {
                        Element::from(text(""))
                    },
                ]
                .align_y(iced::alignment::Vertical::Center)
                .spacing(8)
            };
            body_col = body_col.push(action_bar);

            // Render Images
            for img_path in &email.images {
                body_col = body_col.push(
                    iced::widget::image(iced::widget::image::Handle::from_path(img_path))
                        .width(Length::Fill)
                        .content_fit(iced::ContentFit::Contain),
                );
            }

            // In-body search matches when Ctrl+F search is active
            let q = self.search_query.trim().to_lowercase();
            if !q.is_empty() {
                let matching_lines: Vec<&str> = email
                    .text
                    .lines()
                    .filter(|l| l.to_lowercase().contains(&q))
                    .take(8)
                    .collect();
                if !matching_lines.is_empty() {
                    let mut m_col = column![
                        text(format!("Matches for '{}':", self.search_query.trim()))
                            .size(fs_sub)
                            .bold()
                            .color(theme.success)
                    ]
                    .spacing(4);
                    for line in matching_lines {
                        m_col = m_col.push(
                            text(line)
                                .size(fs_sub)
                                .font(iced::Font::MONOSPACE)
                                .color(theme.warning)
                                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                        );
                    }
                    body_col = body_col.push(
                        container(m_col)
                            .padding(10)
                            .width(Length::Fill)
                            .style(move |_t| container::Style {
                                background: Some(iced::Background::Color(theme.background)),
                                border: iced::Border {
                                    color: theme.success,
                                    width: 1.0,
                                    radius: 4.0.into(),
                                },
                                ..Default::default()
                            }),
                    );
                }
            }

            // Render Text
            body_col = body_col.push(
                text(&email.text)
                    .size(fs_main)
                    .color(theme.text)
                    .font(iced::Font::MONOSPACE)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            );

            // Render Clickable Links (extracted from HTML <a href> & plain text)
            if !email.links.is_empty() {
                body_col = body_col.push(
                    text("Links (Click to Open in Browser):")
                        .size(fs_main)
                        .bold()
                        .color(theme.success),
                );
                let mut links_col = column![].spacing(6);
                for (label, url) in &email.links {
                    let display_url = if url.chars().count() > 72 {
                        let s: String = url.chars().take(69).collect();
                        format!("{}...", s)
                    } else {
                        url.clone()
                    };
                    let btn_label = if label.is_empty() {
                        format!("🔗 {}", display_url)
                    } else {
                        format!("🔗 {} — {}", label, display_url)
                    };
                    links_col = links_col.push(
                        button(
                            text(btn_label)
                                .size(fs_sub)
                                .color(theme.text)
                                .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                        )
                        .on_press(EmailMessage::OpenLink(url.clone()))
                        .padding([6, 10])
                        .style(move |_t, _s| button::Style {
                            background: Some(iced::Background::Color(theme.background)),
                            border: iced::Border {
                                color: theme.border,
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                    );
                }
                body_col = body_col.push(links_col);
            }

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
                        .on_press(EmailMessage::ReplySelected)
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

        let notice_el: Element<'a, EmailMessage> = if let Some(ref notice) = self.status_notice {
            text(notice).size(fs_sub).color(theme.success).into()
        } else {
            text("").into()
        };
        let header_row = row![
            text("✉ EMAIL CLIENT")
                .size(fs_head)
                .bold()
                .color(theme.text),
            notice_el,
            iced::widget::space::horizontal(),
            browser_btn,
            contact_btn,
            button(text("🔍 Search").size(fs_main).bold().color(
                if self.is_searching { theme.success } else { theme.text }
            ))
            .on_press(if self.is_searching {
                EmailMessage::CloseSearch
            } else {
                EmailMessage::OpenSearch
            })
            .padding([6, 12])
            .style(move |_t, _s| button::Style {
                background: Some(iced::Background::Color(theme.surface_alt)),
                border: iced::Border {
                    color: if self.is_searching { theme.success } else { theme.text },
                    width: 1.0,
                    radius: 4.0.into()
                },
                ..Default::default()
            }),
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
            text("Ctrl+↑/↓: Folders  •  ↑/↓: Navigate  •  Enter: Open  •  Ctrl+F: Search  •  n: New  •  s: Star  •  d: Delete  •  c: Save Contact")
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

        let mut root_col = column![header_container].spacing(12);
        if self.is_searching && !self.is_composing && !self.is_logging_in {
            let search_bar = container(
                row![
                    text("🔍").size(fs_main).color(theme.success),
                    text_input("Search sender, subject, or message...", &self.search_query)
                        .id(EMAIL_SEARCH_ID)
                        .on_input(EmailMessage::SearchChanged)
                        .padding(8)
                        .size(fs_main)
                        .width(Length::Fill),
                    button(text("✕").size(fs_main).color(theme.muted))
                        .on_press(EmailMessage::CloseSearch)
                        .padding([4, 8])
                        .style(|_t, _s| button::Style {
                            background: None,
                            ..Default::default()
                        }),
                ]
                .spacing(8)
                .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 0.0,
                right: body_inset,
                bottom: 0.0,
                left: body_inset,
            });
            root_col = root_col.push(search_bar);
        }
        root_col = root_col.push(main_row_container).push(footer_container);

        let card_content = container(
            root_col
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



impl EmailOverlay {
    fn save_current_draft(&self) {
        let to = &self.compose_to;
        let sub = &self.compose_subject;
        let body = self.compose_body.text();
        if to.is_empty() && sub.is_empty() && body.trim().is_empty() {
            println!("[draft] compose is empty, skipping save");
            return;
        }
        println!("[draft] saving from overlay: to={:?}, subj={:?}, body_len={}", to, sub, body.len());
        let _ = crate::libraries::backend::email::client::save_draft_sync(to, sub, &body);
    }
}

#[cfg(test)]
mod search_delete_tests {
    use super::EmailOverlay;
    use crate::libraries::backend::email::client::NativeEnvelope;

    fn env(id: &str, subject: &str) -> NativeEnvelope {
        NativeEnvelope {
            id: id.into(),
            subject: subject.into(),
            sender: "Sender".into(),
            sender_email: "s@example.com".into(),
            is_starred: false,
            is_read: true,
        }
    }

    #[test]
    fn delete_while_searching_stays_on_matching_results() {
        let mut ov = EmailOverlay::empty();
        ov.envelopes = vec![
            env("1", "Noise A"),
            env("2", "Steam receipt 1"),
            env("3", "Noise B"),
            env("4", "Steam receipt 2"),
            env("5", "Noise C"),
            env("6", "Steam receipt 3"),
        ];
        ov.is_searching = true;
        ov.search_query = "steam".into();
        ov.selected_index = 3; // "Steam receipt 2" (visible pos 1)

        let vis_pos = ov
            .visible_envelopes()
            .iter()
            .position(|(i, _)| *i == ov.selected_index);
        assert_eq!(vis_pos, Some(1));

        ov.envelopes.remove(ov.selected_index);
        let next = ov.select_after_removal(vis_pos);
        // Should land on "Steam receipt 3" (now at index 4), NOT index 3 ("Noise C") or index 1 ("top")
        assert_eq!(next.as_deref(), Some("6"));
        assert_eq!(ov.selected_index, 4);

        // Delete last visible item -> moves up to "Steam receipt 1"
        let vis_pos = ov
            .visible_envelopes()
            .iter()
            .position(|(i, _)| *i == ov.selected_index);
        ov.envelopes.remove(ov.selected_index);
        let next = ov.select_after_removal(vis_pos);
        assert_eq!(next.as_deref(), Some("2"));
        assert_eq!(ov.selected_index, 1);
    }
}

fn unique_download_path(dir: &std::path::Path, safe_name: &str) -> std::path::PathBuf {
    let direct = dir.join(safe_name);
    if !direct.exists() {
        return direct;
    }
    let path = std::path::Path::new(safe_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("attachment");
    let ext = path.extension().and_then(|s| s.to_str());
    for n in 1..1000u32 {
        let candidate_name = match ext {
            Some(e) if !e.is_empty() => format!("{} ({}).{}", stem, n, e),
            _ => format!("{} ({})", stem, n),
        };
        let candidate = dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!(
        "{}-{}",
        crate::libraries::process::random_token(),
        safe_name
    ))
}

#[cfg(test)]
mod contact_attach_tests {
    use super::{unique_download_path, EmailMessage, EmailOverlay};

    #[test]
    fn unique_download_path_avoids_overwriting_existing_files() {
        let dir = std::env::temp_dir().join(format!(
            "mbs-attach-test-{}",
            crate::libraries::process::random_token()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let p0 = unique_download_path(&dir, "report.pdf");
        assert_eq!(p0.file_name().unwrap(), "report.pdf");
        std::fs::write(&p0, b"one").unwrap();

        let p1 = unique_download_path(&dir, "report.pdf");
        assert_eq!(p1.file_name().unwrap(), "report (1).pdf");
        std::fs::write(&p1, b"two").unwrap();

        let p2 = unique_download_path(&dir, "report.pdf");
        assert_eq!(p2.file_name().unwrap(), "report (2).pdf");

        let r0 = unique_download_path(&dir, "README");
        assert_eq!(r0.file_name().unwrap(), "README");
        std::fs::write(&r0, b"readme").unwrap();
        let r1 = unique_download_path(&dir, "README");
        assert_eq!(r1.file_name().unwrap(), "README (1)");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn contact_saved_updates_list_and_status_without_imap_error() {
        let mut ov = EmailOverlay::empty();
        ov.update(EmailMessage::ContactSaved("alice@example.com".into()));
        assert_eq!(ov.imap_error, None);
        assert!(ov.status_notice.as_deref().unwrap().contains("alice@example.com"));
        assert_eq!(
            ov.contacts
                .iter()
                .filter(|c| c.as_str() == "alice@example.com")
                .count(),
            1
        );

        ov.update(EmailMessage::ContactSaved("ALICE@example.com".into()));
        assert_eq!(
            ov.contacts
                .iter()
                .filter(|c| c.eq_ignore_ascii_case("alice@example.com"))
                .count(),
            1
        );
    }
}
