//! Email-related update handlers, extracted from `app/update.rs`.
//!
//! Covers `Message::Email(msg)` — the entire email overlay dispatch.

#![allow(clippy::collapsible_match)]

use crate::app::{Message, MoonBeautyShellApp};
use iced::Task as Command;
use moonbeauty::overlays::email::EmailMessage;

impl MoonBeautyShellApp {
    fn email_post_remove_commands(&mut self, next_id: Option<String>) -> Vec<Command<Message>> {
        let mut cmds = Vec::new();
        if let Some(id) = next_id {
            cmds.push(self.handle_email(EmailMessage::SelectMail(id)));
        }
        let vis_pos = self
            .email
            .visible_envelopes()
            .iter()
            .position(|(i, _)| *i == self.email.selected_index)
            .unwrap_or(0);
        let scroll_y = (vis_pos as f32 * 64.0 - 180.0).max(0.0);
        cmds.push(iced::widget::operation::scroll_to(
            iced::widget::Id::new("email_mail_list"),
            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: scroll_y },
        ));
        cmds
    }

    pub(crate) fn handle_email(&mut self, msg: EmailMessage) -> Command<Message> {
        match msg {
            EmailMessage::Dismiss => {
                if self.email.is_searching {
                    return self.handle_email(EmailMessage::CloseSearch);
                }
                if self.email.fullscreen_read {
                    self.email.fullscreen_read = false;
                    return Command::none();
                }
                if self.email.is_composing {
                    self.email.update(EmailMessage::CancelCompose);
                    if self.email.active_folder == "Drafts" {
                        if let Some(id) = self
                            .email
                            .envelopes
                            .get(self.email.selected_index)
                            .map(|m| m.id.clone())
                        {
                            return self.handle_email(EmailMessage::SelectMail(id));
                        }
                    }
                    return Command::none();
                } else {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
            }
            EmailMessage::SelectFolder(f) => {
                println!("[email] SelectFolder: {:?}", f);
                self.email.update(EmailMessage::SelectFolder(f.clone()));
                self.email.envelopes.clear();
                self.email.selected_index = 0;
                self.email.active_mail_id = None;
                self.email.parsed_email = None;

                if f == "Drafts" {
                    self.email.is_loading = false;
                    let envs = moonbeauty::libraries::backend::email::client::load_drafts();
                    self.email.envelopes = envs.clone();
                    return Command::done(Message::Email(EmailMessage::EnvelopesLoaded(f, envs)));
                }

                self.email.is_loading = true;
                let f_fetch = f.clone();
                let f_resp = f;
                return Command::perform(
                    async move { moonbeauty::libraries::backend::email::client::fetch_envelopes(&f_fetch).await },
                    move |res| match res {
                        Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(f_resp, envs)),
                        Err(e) => Message::Email(EmailMessage::Error(e)),
                    },
                );
            }
            EmailMessage::SelectMail(id) => {
                let folder = self.email.active_folder.clone();
                let source = moonbeauty::libraries::backend::email::local_sort::source_folder_for(&id, &folder).unwrap_or_else(|| folder.clone());
                self.email.update(EmailMessage::SelectMail(id.clone()));

                if self.email.active_mail_id.as_deref() == Some(&id) && self.email.parsed_email.is_some() {
                    return Command::none();
                }
                let id_resp = id.clone();
                return Command::perform(
                    async move { moonbeauty::libraries::backend::email::client::read_message(&id, &source).await },
                    move |body| Message::Email(EmailMessage::MessageLoadedFor(folder, id_resp, body)),
                );
            }
            EmailMessage::SelectFolderNext | EmailMessage::SelectFolderPrev => {
                if self.email.is_composing || self.email.is_logging_in || self.email.is_adding_folder {
                    return Command::none();
                }
                if self.email.folders.is_empty() {
                    return Command::none();
                }
                let next_idx = if matches!(msg, EmailMessage::SelectFolderNext) {
                    (self.email.active_folder_idx + 1) % self.email.folders.len()
                } else if self.email.active_folder_idx == 0 {
                    self.email.folders.len() - 1
                } else {
                    self.email.active_folder_idx - 1
                };
                let folder = self.email.folders[next_idx].clone();
                return self.handle_email(EmailMessage::SelectFolder(folder));
            }
            EmailMessage::OpenSearch => {
                if self.email.is_composing || self.email.is_logging_in || self.email.is_adding_folder {
                    return Command::none();
                }
                self.email.update(EmailMessage::OpenSearch);
                return iced::widget::operation::focus(moonbeauty::overlays::email::EMAIL_SEARCH_ID)
                    .map(Message::Email);
            }
            EmailMessage::SearchChanged(_) => {
                self.email.update(msg);
                let vis_indices: Vec<usize> = self
                    .email
                    .visible_envelopes()
                    .into_iter()
                    .map(|(i, _)| i)
                    .collect();
                let mut cmds = Vec::new();
                if let Some(&first_idx) = vis_indices.first() {
                    if !vis_indices.contains(&self.email.selected_index) {
                        self.email.selected_index = first_idx;
                        if let Some(id) = self.email.envelopes.get(first_idx).map(|m| m.id.clone()) {
                            cmds.push(self.handle_email(EmailMessage::SelectMail(id)));
                        }
                    }
                    let vis_pos = vis_indices
                        .iter()
                        .position(|&i| i == self.email.selected_index)
                        .unwrap_or(0);
                    let scroll_y = (vis_pos as f32 * 64.0 - 180.0).max(0.0);
                    cmds.push(iced::widget::operation::scroll_to(
                        iced::widget::Id::new("email_mail_list"),
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: scroll_y },
                    ));
                } else {
                    self.email.active_mail_id = None;
                    self.email.parsed_email = None;
                }
                return Command::batch(cmds);
            }
            EmailMessage::CloseSearch => {
                self.email.update(EmailMessage::CloseSearch);
                let mut cmds = Vec::new();
                if self.email.active_mail_id.is_none() {
                    if let Some(id) = self
                        .email
                        .envelopes
                        .get(self.email.selected_index)
                        .map(|m| m.id.clone())
                    {
                        cmds.push(self.handle_email(EmailMessage::SelectMail(id)));
                    }
                }
                let scroll_y = (self.email.selected_index as f32 * 64.0 - 180.0).max(0.0);
                cmds.push(iced::widget::operation::scroll_to(
                    iced::widget::Id::new("email_mail_list"),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: scroll_y },
                ));
                return Command::batch(cmds);
            }
            EmailMessage::OpenLink(url) => {
                let lower = url.trim().to_ascii_lowercase();
                if lower.starts_with("https://") || lower.starts_with("http://") {
                    let _ = moonbeauty::libraries::process::spawn_and_reap(
                        std::process::Command::new("xdg-open").arg(url.trim()),
                    );
                }
                return Command::none();
            }
            EmailMessage::SelectMailNext | EmailMessage::SelectMailPrev => {
                if self.email.is_composing || self.email.is_logging_in || self.email.is_adding_folder {
                    return Command::none();
                }
                self.email.update(msg);
                let mut cmds = Vec::new();
                if let Some(id) = self
                    .email
                    .envelopes
                    .get(self.email.selected_index)
                    .map(|m| m.id.clone())
                {
                    cmds.push(self.handle_email(EmailMessage::SelectMail(id)));
                }
                let vis_pos = self
                    .email
                    .visible_envelopes()
                    .iter()
                    .position(|(i, _)| *i == self.email.selected_index)
                    .unwrap_or(0);
                let scroll_y = (vis_pos as f32 * 64.0 - 180.0).max(0.0);
                cmds.push(iced::widget::operation::scroll_to(
                    iced::widget::Id::new("email_mail_list"),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: scroll_y },
                ));
                return Command::batch(cmds);
            }
            EmailMessage::OpenSelected => {
                if self.email.is_composing || self.email.is_logging_in || self.email.is_adding_folder {
                    return Command::none();
                }
                if !self
                    .email
                    .visible_envelopes()
                    .iter()
                    .any(|(i, _)| *i == self.email.selected_index)
                {
                    return Command::none();
                }
                if self.email.fullscreen_read {
                    return self.handle_email(EmailMessage::ReplySelected);
                }
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index).cloned() {
                    if mail.id.starts_with("draft_") || self.email.active_folder == "Drafts" {
                        return self.handle_email(EmailMessage::EditDraft(mail.id));
                    }
                    let already_loaded = self.email.active_mail_id.as_deref() == Some(mail.id.as_str())
                        && self.email.parsed_email.is_some();
                    if already_loaded {
                        self.email.fullscreen_read = true;
                    } else {
                        return self.handle_email(EmailMessage::SelectMail(mail.id));
                    }
                }
            }
            EmailMessage::EditDraft(_) => {
                self.email.update(msg);
                let target_id = if self.email.compose_to.is_empty() {
                    moonbeauty::overlays::email::COMPOSE_TO_ID
                } else {
                    moonbeauty::overlays::email::COMPOSE_BODY_ID
                };
                return iced::widget::operation::focus(target_id).map(Message::Email);
            }
            EmailMessage::ReplySelected => {
                self.email.update(EmailMessage::ReplySelected);
                let target_id = if self.email.compose_to.is_empty() {
                    moonbeauty::overlays::email::COMPOSE_TO_ID
                } else {
                    moonbeauty::overlays::email::COMPOSE_BODY_ID
                };
                return iced::widget::operation::focus(target_id).map(Message::Email);
            }
            EmailMessage::SendMail => {
                let to = self.email.compose_to.clone();
                let sub = self.email.compose_subject.clone();
                let body = self.email.compose_body.text().to_string();

                let mut attachments = Vec::new();
                for path in &self.email.compose_attachments {
                    eprintln!("[attach] reading {:?}", path);
                    if let (Some(name), Ok(data)) = (path.file_name(), std::fs::read(path)) {
                        eprintln!(
                            "[attach] attached {} ({} bytes)",
                            name.to_string_lossy(),
                            data.len()
                        );
                        attachments.push((name.to_string_lossy().to_string(), data));
                    } else {
                        eprintln!("[attach] FAILED to read {:?}", path);
                    }
                }

                eprintln!(
                    "[send] sending to={:?} subject={:?} attachments={}",
                    to,
                    sub,
                    attachments.len()
                );
                self.email.update(EmailMessage::SendMail);
                return Command::perform(
                    async move {
                        moonbeauty::libraries::backend::email::client::send_message(
                            &to,
                            &sub,
                            &body,
                            attachments,
                        )
                        .await
                    },
                    |res| match res {
                        Ok(_) => Message::Email(EmailMessage::MessageLoaded(
                            moonbeauty::libraries::backend::email::client::ParsedEmail {
                                text: "✓ Message sent.".into(),
                                images: Vec::new(),
                                attachments: Vec::new(),
                                links: Vec::new(),
                            },
                        )),
                        Err(e) => Message::Email(EmailMessage::MessageLoaded(
                            moonbeauty::libraries::backend::email::client::ParsedEmail {
                                text: format!("✗ Send failed: {}", e),
                                images: Vec::new(),
                                attachments: Vec::new(),
                                links: Vec::new(),
                            },
                        )),
                    },
                );
            }
            EmailMessage::EnvelopesLoaded(folder, envs) => {
                println!("[email] EnvelopesLoaded for {:?} ({} emails)", folder, envs.len());
                if folder == "Drafts" {
                    if self.email.active_folder == "Drafts"
                        && self.email.active_mail_id.is_none()
                        && !self.email.is_composing
                        && !self.email.is_logging_in
                    {
                        if let Some(id) = self.email.envelopes.get(self.email.selected_index).map(|m| m.id.clone()) {
                            return self.handle_email(EmailMessage::SelectMail(id));
                        }
                    }
                    return Command::none(); // NEVER accept IMAP updates for Drafts
                }
                let mut cmds = Vec::new();
                if folder == self.email.active_folder {
                    self.email.update(EmailMessage::EnvelopesLoaded(folder.clone(), envs));
                    if self.email.active_mail_id.is_none()
                        && !self.email.is_composing
                        && !self.email.is_logging_in
                    {
                        if let Some(id) = self.email.envelopes.get(self.email.selected_index).map(|m| m.id.clone()) {
                            cmds.push(self.handle_email(EmailMessage::SelectMail(id)));
                        }
                    }
                }
                let folder_for_fetch = folder.clone();
                let folder_for_resp = folder;
                cmds.push(Command::perform(
                    async move {
                        moonbeauty::libraries::backend::email::client::fetch_from_network(
                            &folder_for_fetch,
                        )
                        .await
                    },
                    move |res| match res {
                        Ok(envs) => Message::Email(EmailMessage::NetworkEnvelopesLoaded(
                            folder_for_resp,
                            envs,
                        )),
                        Err(_) => Message::Email(EmailMessage::SyncDone),
                    },
                ));
                return Command::batch(cmds);
            }
            EmailMessage::NetworkEnvelopesLoaded(folder, envs) => {
                if folder == "Drafts" || folder != self.email.active_folder {
                    return Command::none();
                }
                self.email.update(EmailMessage::NetworkEnvelopesLoaded(folder, envs));
                if self.email.active_mail_id.is_none()
                    && !self.email.is_composing
                    && !self.email.is_logging_in
                {
                    if let Some(id) = self.email.envelopes.get(self.email.selected_index).map(|m| m.id.clone()) {
                        return self.handle_email(EmailMessage::SelectMail(id));
                    }
                }
            }
            EmailMessage::ToggleStar => {
                if !self
                    .email
                    .visible_envelopes()
                    .iter()
                    .any(|(i, _)| *i == self.email.selected_index)
                {
                    return Command::none();
                }
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let id = mail.id.clone();
                    let folder = self.email.active_folder.clone();
                    let source = moonbeauty::libraries::backend::email::local_sort::source_folder_for(&id, &folder)
                        .unwrap_or_else(|| folder.clone());
                    let star = !mail.is_starred;
                    self.email.update(EmailMessage::ToggleStar);
                    return Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::toggle_star(
                                &id, &source, star,
                            )
                            .await
                        },
                        |res| match res {
                            Ok(_) => Message::Email(EmailMessage::SyncDone),
                            Err(e) => Message::Email(EmailMessage::Error(e)),
                        },
                    );
                }
            }
            EmailMessage::DeleteSelected => {
                if self.email.is_composing || self.email.is_logging_in || self.email.is_adding_folder {
                    return Command::none();
                }
                let vis_pos = self
                    .email
                    .visible_envelopes()
                    .iter()
                    .position(|(i, _)| *i == self.email.selected_index);
                if vis_pos.is_none() {
                    return Command::none();
                }
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    if mail.is_starred {
                        self.email.imap_error =
                            Some("Cannot delete a starred email. Unstar it first.".into());
                    } else {
                        let id = mail.id.clone();
                        let mail_clone = mail.clone();
                        let folder = self.email.active_folder.clone();
                        let source = moonbeauty::libraries::backend::email::local_sort::source_folder_for(&id, &folder)
                            .unwrap_or_else(|| folder.clone());

                        if folder == "Drafts" {
                            if let Err(e) = moonbeauty::libraries::backend::email::client::delete_draft(&id) {
                                self.email.imap_error = Some(e);
                                return Command::none();
                            }
                            self.email.envelopes = moonbeauty::libraries::backend::email::client::load_drafts();
                            self.email.parsed_cache.clear();
                            let next_id = self.email.select_after_removal(vis_pos);
                            let cmds = self.email_post_remove_commands(next_id);
                            return Command::batch(cmds);
                        }

                        self.email.envelopes.remove(self.email.selected_index);
                        let next_id = self.email.select_after_removal(vis_pos);
                        let mut cmds = self.email_post_remove_commands(next_id);

                        moonbeauty::libraries::backend::email::client::save_index(
                            &folder,
                            &self.email.envelopes,
                        );
                        if source != folder {
                            let mut src_envs =
                                moonbeauty::libraries::backend::email::client::load_index(&source);
                            src_envs.retain(|e| e.id != id);
                            moonbeauty::libraries::backend::email::client::save_index(
                                &source,
                                &src_envs,
                            );
                        }

                        let is_trash = folder == "Trash" || source == "Trash";

                        if !is_trash {
                            let mut trash_envs =
                                moonbeauty::libraries::backend::email::client::load_index("Trash");
                            trash_envs.insert(0, mail_clone);
                            moonbeauty::libraries::backend::email::client::save_index(
                                "Trash",
                                &trash_envs,
                            );
                        }

                        cmds.push(Command::perform(
                            async move {
                                if is_trash {
                                    moonbeauty::libraries::backend::email::client::delete_message(
                                        &id, &source,
                                    )
                                    .await
                                } else {
                                    moonbeauty::libraries::backend::email::client::move_to_trash(
                                        &id, &source,
                                    )
                                    .await
                                }
                            },
                            |res| match res {
                                Ok(_) => Message::Email(EmailMessage::SyncDone),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            },
                        ));
                        return Command::batch(cmds);
                    }
                }
            }
            EmailMessage::CopyGmailFilter => {
                let folder = self.email.active_folder.clone();
                if let Some(filter_text) =
                    moonbeauty::libraries::backend::email::local_sort::gmail_filter_text_for(
                        &folder,
                    )
                {
                    return Command::perform(
                        async move {
                            if let Ok(mut child) = tokio::process::Command::new("wl-copy")
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                            {
                                use tokio::io::AsyncWriteExt;
                                if let Some(mut stdin) = child.stdin.take() {
                                    let _ = stdin.write_all(filter_text.as_bytes()).await;
                                }
                                let _ = child.wait().await;
                            }
                            let _ = tokio::process::Command::new("notify-send")
                                .args([
                                    "-a",
                                    "Email",
                                    "Gmail Filter Copied",
                                    "Paste into Gmail Settings → Filters → Create new filter",
                                ])
                                .spawn();
                        },
                        |_| Message::Email(EmailMessage::SyncDone),
                    );
                } else {
                    // Not a local folder — inform the user
                    let _ = std::process::Command::new("notify-send")
                        .args([
                            "-a",
                            "Email",
                            "No filter available",
                            "Select a locally-sorted folder (like Steam (local)) first",
                        ])
                        .spawn();
                }
            }
            EmailMessage::MarkSpam => {
                let vis_pos = self
                    .email
                    .visible_envelopes()
                    .iter()
                    .position(|(i, _)| *i == self.email.selected_index);
                if vis_pos.is_none() {
                    return Command::none();
                }
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let id = mail.id.clone();
                    let folder = self.email.active_folder.clone();
                    let source = moonbeauty::libraries::backend::email::local_sort::source_folder_for(&id, &folder)
                        .unwrap_or_else(|| folder.clone());
                    self.email.envelopes.remove(self.email.selected_index);
                    let next_id = self.email.select_after_removal(vis_pos);
                    let mut cmds = self.email_post_remove_commands(next_id);
                    moonbeauty::libraries::backend::email::client::save_index(
                        &folder,
                        &self.email.envelopes,
                    );
                    if source != folder {
                        let mut src_envs =
                            moonbeauty::libraries::backend::email::client::load_index(&source);
                        src_envs.retain(|e| e.id != id);
                        moonbeauty::libraries::backend::email::client::save_index(
                            &source,
                            &src_envs,
                        );
                    }
                    cmds.push(Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::move_to_spam(
                                &id, &source,
                            )
                            .await
                        },
                        |res| match res {
                            Ok(_) => Message::Email(EmailMessage::SyncDone),
                            Err(e) => Message::Email(EmailMessage::Error(e)),
                        },
                    ));
                    return Command::batch(cmds);
                }
            }
            EmailMessage::MarkNotSpam => {
                let vis_pos = self
                    .email
                    .visible_envelopes()
                    .iter()
                    .position(|(i, _)| *i == self.email.selected_index);
                if vis_pos.is_none() {
                    return Command::none();
                }
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let id = mail.id.clone();
                    self.email.envelopes.remove(self.email.selected_index);
                    let next_id = self.email.select_after_removal(vis_pos);
                    let mut cmds = self.email_post_remove_commands(next_id);
                    moonbeauty::libraries::backend::email::client::save_index(
                        "Spam",
                        &self.email.envelopes,
                    );
                    cmds.push(Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::mark_not_spam(&id).await
                        },
                        |res| match res {
                            Ok(_) => Message::Email(EmailMessage::SyncDone),
                            Err(e) => Message::Email(EmailMessage::Error(e)),
                        },
                    ));
                    return Command::batch(cmds);
                }
            }
            EmailMessage::SaveContact => {
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let email = mail.sender_email.clone();
                    if !email.is_empty() {
                        return Command::perform(
                            async move {
                                moonbeauty::libraries::backend::email::client::save_contact(&email)
                                    .await
                            },
                            |res| match res {
                                Ok(_) => Message::Email(EmailMessage::DraftSaved),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            },
                        );
                    }
                }
            }
            EmailMessage::OpenInBrowser
            | EmailMessage::GetAppPassword
            | EmailMessage::SelectContact(_)
            | EmailMessage::SelectAutocompleteNext
            | EmailMessage::SelectAutocompletePrev
            | EmailMessage::ConfirmAutocomplete
            | EmailMessage::StartAddFolder
            | EmailMessage::AddFolderChanged(_)
            | EmailMessage::SubmitAddFolder
            | EmailMessage::RemoveFolder(_)
            | EmailMessage::AttachmentsPicked(_)
            | EmailMessage::RemoveAttachment(_)
            | EmailMessage::ManualAttachChanged(_)
            | EmailMessage::SubmitManualAttach => {
                self.email.update(msg);
            }
            EmailMessage::AddAttachment => {
                return Command::perform(
                    async {
                        let files = rfd::AsyncFileDialog::new()
                            .add_filter("All Files", &["*"])
                            .pick_files()
                            .await;
                        let paths = files
                            .map(|f| {
                                f.into_iter()
                                    .map(|file| file.path().to_path_buf())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        eprintln!("[attach] rfd picked: {:?}", paths);
                        paths
                    },
                    |paths| Message::Email(EmailMessage::AttachmentsPicked(paths)),
                );
            }
            EmailMessage::OpenComposer => {
                self.email.fullscreen_read = false;
                self.email.reply_from_fullview = false;
                self.email.update(msg);
                return iced::widget::operation::focus(moonbeauty::overlays::email::COMPOSE_TO_ID)
                    .map(Message::Email);
            }
            EmailMessage::SaveLogin => {
                let cfg = moonbeauty::libraries::backend::email::client::EmailConfig {
                    email: self.email.login_email.clone(),
                    password: Some(self.email.login_password.clone()),
                    password_cmd: None,
                    imap_host: self.email.login_imap.clone(),
                    imap_port: 993,
                    smtp_host: self.email.login_smtp.clone(),
                    smtp_port: 465,
                    sops_key: if self.email.login_sops_key.trim().is_empty() {
                        None
                    } else {
                        Some(self.email.login_sops_key.clone())
                    },
                };
                let use_sops = self.email.use_sops;
                match moonbeauty::libraries::backend::email::client::save_config(&cfg, use_sops) {
                    Ok(_) => {
                        self.email.is_logging_in = false;
                        self.email.login_error = None;
                        return Command::batch(vec![
                            Command::perform(
                                async move {
                                    moonbeauty::libraries::backend::email::client::fetch_envelopes(
                                        "INBOX",
                                    )
                                    .await
                                },
                                |res| match res {
                                    Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(
                                        "INBOX".to_string(),
                                        envs,
                                    )),
                                    Err(e) => Message::Email(EmailMessage::Error(e)),
                                },
                            ),
                        ]);
                    }
                    Err(e) => {
                        self.email.login_error = Some(e);
                    }
                }
            }
            _ => self.email.update(msg),
        }
        Command::none()
    }
}
