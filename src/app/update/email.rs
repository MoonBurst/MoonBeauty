//! Email-related update handlers, extracted from `app/update.rs`.
//!
//! Covers `Message::Email(msg)` — the entire email overlay dispatch.

#![allow(clippy::collapsible_match)]

use crate::app::{Message, MoonBeautyShellApp};
use iced::Task as Command;
use moonbeauty::overlays::email::EmailMessage;

impl MoonBeautyShellApp {
    pub(crate) fn handle_email(&mut self, msg: EmailMessage) -> Command<Message> {
        match msg {
            EmailMessage::Dismiss => {
                if self.email.fullscreen_read {
                    self.email.fullscreen_read = false;
                    return Command::none();
                }
                if self.email.is_composing {
                    let to = self.email.compose_to.clone();
                    let sub = self.email.compose_subject.clone();
                    let body = self.email.compose_body.text();
                    self.email.update(EmailMessage::CancelCompose);
                    return Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::save_draft(
                                &to, &sub, &body,
                            )
                            .await
                        },
                        |_| Message::Email(EmailMessage::DraftSaved),
                    );
                } else {
                    if self.is_standalone {
                        std::process::exit(0);
                    }
                    self.active_overlay = None;
                }
            }
            EmailMessage::SelectFolder(f) => {
                self.email.update(EmailMessage::SelectFolder(f.clone()));
                let f_fetch = f.clone();
                let f_resp = f;
                return Command::batch(vec![
                    Command::done(Message::Email(EmailMessage::SetLoading)),
                    Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::fetch_envelopes(&f_fetch)
                                .await
                        },
                        move |res| match res {
                            Ok(envs) => Message::Email(EmailMessage::EnvelopesLoaded(f_resp, envs)),
                            Err(e) => Message::Email(EmailMessage::Error(e)),
                        },
                    ),
                ]);
            }
            EmailMessage::SelectMail(id) => {
                let folder = self.email.active_folder.clone();
                // If we're in a local folder, the email actually lives
                // in the Gmail folder it was assigned from (usually INBOX).
                let source = moonbeauty::libraries::backend::email::local_sort::source_folder_for(
                    &id, &folder,
                )
                .unwrap_or_else(|| folder.clone());
                self.email.update(EmailMessage::SelectMail(id.clone()));
                // Cache hit in email.rs::update -> parsed_email is Some,
                // no need to spawn an async re-fetch.
                if self.email.active_mail_id.as_deref() == Some(&id)
                    && self.email.parsed_email.is_some()
                {
                    return Command::none();
                }
                return Command::perform(
                    async move {
                        moonbeauty::libraries::backend::email::client::read_message(&id, &source)
                            .await
                    },
                    |body| Message::Email(EmailMessage::MessageLoaded(body)),
                );
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
                            },
                        )),
                        Err(e) => Message::Email(EmailMessage::MessageLoaded(
                            moonbeauty::libraries::backend::email::client::ParsedEmail {
                                text: format!("✗ Send failed: {}", e),
                                images: Vec::new(),
                                attachments: Vec::new(),
                            },
                        )),
                    },
                );
            }
            EmailMessage::EnvelopesLoaded(folder, envs) => {
                if folder == self.email.active_folder {
                    self.email
                        .update(EmailMessage::EnvelopesLoaded(folder.clone(), envs));
                }
                let folder_for_fetch = folder.clone();
                let folder_for_resp = folder;
                return Command::perform(
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
                );
            }
            EmailMessage::NetworkEnvelopesLoaded(folder, envs) => {
                // Local auto-sort only on INBOX (where the sort rules apply)
                if folder == "INBOX" && self.settings.email_auto_sort_threshold > 0 {
                    let threshold = self.settings.email_auto_sort_threshold;
                    moonbeauty::libraries::backend::email::local_sort::auto_sort(
                        &envs, &folder, threshold,
                    );
                }
                if std::env::var_os("MBS_DEBUG").is_some() {
                    eprintln!(
                        "[sync] NetworkEnvelopesLoaded folder={:?} count={} active={:?}",
                        folder,
                        envs.len(),
                        self.email.active_folder
                    );
                }
                moonbeauty::libraries::backend::email::client::save_index(&folder, &envs);
                if folder == self.email.active_folder {
                    self.email
                        .update(EmailMessage::EnvelopesLoaded(folder.clone(), envs.clone()));
                }
                return Command::perform(
                    async move {
                        moonbeauty::libraries::backend::email::client::sync_bodies_background(
                            folder,
                        )
                        .await
                    },
                    |_| Message::Email(EmailMessage::SyncDone),
                );
            }
            EmailMessage::SyncDone => {}
            EmailMessage::SelectMailNext | EmailMessage::SelectMailPrev => {
                if self.email.fullscreen_read {
                    return Command::none();
                }
                if self.email.is_composing && !self.email.get_autocomplete_matches().is_empty() {
                    let target = match msg {
                        EmailMessage::SelectMailNext => EmailMessage::SelectAutocompleteNext,
                        _ => EmailMessage::SelectAutocompletePrev,
                    };
                    self.email.update(target);
                } else if !self.email.is_composing && !self.email.is_logging_in {
                    self.email.update(msg);
                    // Arrow navigation is PREVIEW-ONLY. It must never
                    // enter fullview — that is Enter's job.
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        let id = mail.id.clone();
                        // Move the visible list so the selected row stays
                        // on screen. iced has no auto-scroll-to-selection
                        // for scrollables, so we emit scroll_to with an
                        // AbsoluteOffset computed from the row index.
                        //
                        // Row height ≈ fs_main*1.2 + fs_sub*1.2 + 24 px
                        // padding/border. The 200px anchor keeps the
                        // selected row below mid-viewport so context is
                        // visible above.
                        let fs_main = self.theme.font_size_overlay as f32;
                        let fs_sub = (fs_main - 2.0).max(8.0);
                        let row_h = fs_main * 1.2 + fs_sub * 1.2 + 24.0;
                        let idx = self.email.selected_index as f32;
                        let scroll_y = (idx * row_h - 200.0).max(0.0);
                        let scroll_cmd: Command<Message> = iced::widget::operation::scroll_to(
                            iced::widget::Id::new("email_mail_list"),
                            iced::widget::scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: scroll_y,
                            },
                        );
                        let fetch_cmd = self.update(Message::Email(EmailMessage::SelectMail(id)));
                        return Command::batch(vec![fetch_cmd, scroll_cmd]);
                    }
                }
            }
            EmailMessage::SelectFolderNext | EmailMessage::SelectFolderPrev => {
                if !self.email.is_composing && !self.email.is_logging_in {
                    let old_folder = self.email.active_folder.clone();
                    self.email.update(msg);
                    if self.email.active_folder != old_folder {
                        let f = self.email.active_folder.clone();
                        let f_fetch = f.clone();
                        let f_resp = f;
                        return Command::batch(vec![
                            Command::done(Message::Email(EmailMessage::SetLoading)),
                            Command::perform(
                                async move {
                                    moonbeauty::libraries::backend::email::client::fetch_envelopes(
                                        &f_fetch,
                                    )
                                    .await
                                },
                                move |res| match res {
                                    Ok(envs) => {
                                        Message::Email(EmailMessage::EnvelopesLoaded(f_resp, envs))
                                    }
                                    Err(e) => Message::Email(EmailMessage::Error(e)),
                                },
                            ),
                        ]);
                    }
                }
            }
            EmailMessage::OpenSelected => {
                if std::env::var_os("MBS_DEBUG").is_some() {
                    eprintln!(
                        "[email-opensel] fullscreen={} composing={} logging_in={} active_id={:?} envelopes={} parsed={}",
                        self.email.fullscreen_read,
                        self.email.is_composing,
                        self.email.is_logging_in,
                        self.email.active_mail_id,
                        self.email.envelopes.len(),
                        self.email.parsed_email.is_some(),
                    );
                }
                // Enter in fullview -> reply compose (To/Subject prefilled).
                // Enter in 3-pane on already-loaded mail -> enter fullview.
                // Enter in 3-pane on unloaded mail -> load + preview.
                if self.email.fullscreen_read {
                    let reply_env = self
                        .email
                        .active_mail_id
                        .as_ref()
                        .and_then(|id| self.email.envelopes.iter().find(|e| &e.id == id))
                        .cloned();
                    if let Some(env) = reply_env {
                        // Prefer sender_email; fall back to display name if
                        // the header couldn't be parsed into a bare address.
                        let to_addr = if !env.sender_email.trim().is_empty() {
                            env.sender_email.clone()
                        } else if !env.sender.trim().is_empty() && env.sender.trim() != "Unknown" {
                            env.sender.clone()
                        } else {
                            String::new()
                        };
                        let subj = env.subject.trim();
                        let reply_subject = if subj.is_empty() {
                            "Re:".to_string()
                        } else if subj.to_lowercase().starts_with("re:") {
                            subj.to_string()
                        } else {
                            format!("Re: {}", subj)
                        };
                        // Quote the original so the user has context. The
                        // quoted block sits below the cursor, standard mail
                        // client convention.
                        let orig_text = self
                            .email
                            .parsed_email
                            .as_ref()
                            .map(|p| p.text.clone())
                            .unwrap_or_default();
                        let mut reply_body = String::new();
                        reply_body.push_str("\n\n--- Original message ---\n");
                        if !env.sender_email.trim().is_empty() && env.sender_email != env.sender {
                            reply_body.push_str(&format!(
                                "From: {} <{}>\n",
                                env.sender, env.sender_email
                            ));
                        } else {
                            reply_body.push_str(&format!("From: {}\n", env.sender));
                        }
                        reply_body.push_str(&format!("Subject: {}\n\n", env.subject));
                        if orig_text.is_empty() {
                            reply_body.push_str("(Original body unavailable)\n");
                        } else {
                            for line in orig_text.lines() {
                                reply_body.push('>');
                                reply_body.push(' ');
                                reply_body.push_str(line);
                                reply_body.push('\n');
                            }
                        }
                        if std::env::var_os("MBS_DEBUG").is_some() {
                            eprintln!(
                                "[email-reply] to={:?} subject={:?} body_chars={}",
                                to_addr,
                                reply_subject,
                                reply_body.chars().count()
                            );
                        }
                        self.email.is_composing = true;
                        self.email.compose_to = to_addr;
                        self.email.compose_subject = reply_subject;
                        self.email.compose_body =
                            iced::widget::text_editor::Content::with_text(&reply_body);
                        self.email.active_compose_field = 2;
                        self.email.compose_attachments.clear();
                        self.email.fullscreen_read = false;
                        self.email.reply_from_fullview = true;
                        return iced::widget::operation::focus(iced::widget::Id::new(
                            moonbeauty::overlays::email::COMPOSE_BODY_ID,
                        ))
                        .map(Message::Email);
                    }
                    // No loaded envelope to reply to; just exit.
                    self.email.fullscreen_read = false;
                    return Command::none();
                }
                if !self.email.is_composing && !self.email.is_logging_in {
                    if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                        let id = mail.id.clone();
                        if self.email.active_mail_id.as_deref() == Some(&id)
                            && self.email.parsed_email.is_some()
                        {
                            self.email.fullscreen_read = true;
                            return Command::none();
                        }
                        let folder = self.email.active_folder.clone();
                        self.email.update(EmailMessage::SelectMail(id.clone()));
                        return Command::perform(
                            async move {
                                moonbeauty::libraries::backend::email::client::read_message(
                                    &id, &folder,
                                )
                                .await
                            },
                            |body| Message::Email(EmailMessage::MessageLoaded(body)),
                        );
                    }
                }
            }
            EmailMessage::ToggleStar => {
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let id = mail.id.clone();
                    let folder = self.email.active_folder.clone();
                    let star = !mail.is_starred;
                    self.email.update(EmailMessage::ToggleStar);
                    return Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::toggle_star(
                                &id, &folder, star,
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
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    if mail.is_starred {
                        self.email.imap_error =
                            Some("Cannot delete a starred email. Unstar it first.".into());
                    } else {
                        let id = mail.id.clone();
                        let mail_clone = mail.clone();
                        let folder = self.email.active_folder.clone();

                        self.email.envelopes.remove(self.email.selected_index);
                        if self.email.selected_index >= self.email.envelopes.len()
                            && !self.email.envelopes.is_empty()
                        {
                            self.email.selected_index = self.email.envelopes.len() - 1;
                        }
                        self.email.parsed_email = None;
                        self.email.active_mail_id = None;

                        moonbeauty::libraries::backend::email::client::save_index(
                            &folder,
                            &self.email.envelopes,
                        );

                        let is_trash = folder == "Trash";

                        if !is_trash {
                            let mut trash_envs =
                                moonbeauty::libraries::backend::email::client::load_index("Trash");
                            trash_envs.insert(0, mail_clone);
                            moonbeauty::libraries::backend::email::client::save_index(
                                "Trash",
                                &trash_envs,
                            );
                        }

                        return Command::perform(
                            async move {
                                if is_trash {
                                    moonbeauty::libraries::backend::email::client::delete_message(
                                        &id, &folder,
                                    )
                                    .await
                                } else {
                                    moonbeauty::libraries::backend::email::client::move_to_trash(
                                        &id, &folder,
                                    )
                                    .await
                                }
                            },
                            |res| match res {
                                Ok(_) => Message::Email(EmailMessage::SyncDone),
                                Err(e) => Message::Email(EmailMessage::Error(e)),
                            },
                        );
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
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let id = mail.id.clone();
                    let folder = self.email.active_folder.clone();
                    self.email.envelopes.remove(self.email.selected_index);
                    if self.email.selected_index >= self.email.envelopes.len()
                        && !self.email.envelopes.is_empty()
                    {
                        self.email.selected_index = self.email.envelopes.len() - 1;
                    }
                    self.email.parsed_email = None;
                    self.email.active_mail_id = None;
                    moonbeauty::libraries::backend::email::client::save_index(
                        &folder,
                        &self.email.envelopes,
                    );
                    return Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::move_to_spam(
                                &id, &folder,
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
            EmailMessage::MarkNotSpam => {
                if let Some(mail) = self.email.envelopes.get(self.email.selected_index) {
                    let id = mail.id.clone();
                    self.email.envelopes.remove(self.email.selected_index);
                    if self.email.selected_index >= self.email.envelopes.len()
                        && !self.email.envelopes.is_empty()
                    {
                        self.email.selected_index = self.email.envelopes.len() - 1;
                    }
                    self.email.parsed_email = None;
                    self.email.active_mail_id = None;
                    moonbeauty::libraries::backend::email::client::save_index(
                        "Spam",
                        &self.email.envelopes,
                    );
                    return Command::perform(
                        async move {
                            moonbeauty::libraries::backend::email::client::mark_not_spam(&id).await
                        },
                        |res| match res {
                            Ok(_) => Message::Email(EmailMessage::SyncDone),
                            Err(e) => Message::Email(EmailMessage::Error(e)),
                        },
                    );
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
                            Command::done(Message::Email(EmailMessage::SetLoading)),
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
