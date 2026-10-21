use iced::widget::{button, column, container, row, scrollable, text, Stack};
use iced::{Alignment, Color, Element, Length};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::config::theme::ShellTheme;
use crate::style::shape_box::{ShapeBox, ShapeType};
use crate::style::TextExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeItem {
    pub id: String,
    pub subject: Option<String>,
    pub from: Option<SenderInfo>,
    pub flags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderInfo {
    pub name: Option<String>,
    pub addr: Option<String>,
}

#[derive(Debug, Clone)]
pub enum EmailMessage {
    SelectFolder(String),
    SelectMail(String),
    MailBodyLoaded(String),
    OpenComposer,
    ToChanged(String),
    SubjectChanged(String),
    BodyChanged(String),
    SendMail,
    Dismiss,
}

pub struct EmailOverlay {
    pub folders: Vec<&'static str>,
    pub active_folder: String,
    pub envelopes: Vec<EnvelopeItem>,
    pub active_mail_id: Option<String>,
    pub mail_body: String,
    pub is_composing: bool,
    pub compose_to: String,
    pub compose_subject: String,
    pub compose_body: String,
}

impl Default for EmailOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailOverlay {
    pub fn new() -> Self {
        Self {
            folders: vec!["INBOX", "Starred", "Sent", "Drafts", "Trash", "Spam"],
            active_folder: "INBOX".into(),
            envelopes: Vec::new(),
            active_mail_id: None,
            mail_body: String::new(),
            is_composing: false,
            compose_to: String::new(),
            compose_subject: String::new(),
            compose_body: String::new(),
        }
    }

    pub async fn fetch_envelopes(folder: &str) -> Vec<EnvelopeItem> {
        let Ok(out) = Command::new("himalaya")
            .args(["envelope", "list", "--folder", folder, "--page-size", "40", "--output", "json"])
            .output()
            .await
        else {
            return Vec::new();
        };

        serde_json::from_slice(&out.stdout).unwrap_or_default()
    }

    pub async fn read_message(id: &str, folder: &str) -> String {
        let Ok(out) = Command::new("himalaya")
            .args(["message", "read", id, "--folder", folder])
            .output()
            .await
        else {
            return "Failed to load email message.".into();
        };

        String::from_utf8_lossy(&out.stdout).to_string()
    }

    pub async fn send_message(to: &str, subject: &str, body: &str) {
        let raw = format!("To: {}\nSubject: {}\n\n{}", to, subject, body);
        let child = Command::new("himalaya")
            .args(["template", "send"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .ok();

        if let Some(mut c) = child {
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = c.stdin.take() {
                let _ = stdin.write_all(raw.as_bytes()).await;
            }
        }
    }

    pub fn update(&mut self, message: EmailMessage) {
        match message {
            EmailMessage::SelectFolder(f) => {
                self.active_folder = f;
            }
            EmailMessage::SelectMail(id) => {
                self.active_mail_id = Some(id);
            }
            EmailMessage::MailBodyLoaded(b) => {
                self.mail_body = b;
            }
            EmailMessage::OpenComposer => {
                self.is_composing = true;
            }
            EmailMessage::ToChanged(t) => {
                self.compose_to = t;
            }
            EmailMessage::SubjectChanged(s) => {
                self.compose_subject = s;
            }
            EmailMessage::BodyChanged(b) => {
                self.compose_body = b;
            }
            EmailMessage::SendMail => {
                let to = self.compose_to.clone();
                let sub = self.compose_subject.clone();
                let b = self.compose_body.clone();
                self.is_composing = false;
                self.compose_to.clear();
                self.compose_subject.clear();
                self.compose_body.clear();
                tokio::spawn(async move {
                    Self::send_message(&to, &sub, &b).await;
                });
            }
            EmailMessage::Dismiss => {
                self.is_composing = false;
            }
        }
    }

    pub fn view<'a>(&'a self, theme: &ShellTheme) -> Element<'a, EmailMessage> {
        let mut folder_col = column![].spacing(6);
        for f in &self.folders {
            let is_sel = &self.active_folder == f;
            let col = if is_sel { theme.success } else { theme.text };
            let f_clone = f.to_string();

            let f_btn = button(text(*f).size(theme.font_size_normal).bold().color(col))
                .on_press(EmailMessage::SelectFolder(f_clone))
                .padding([8, 12])
                .style(|_t, _s| button::Style { background: None, ..Default::default() });

            folder_col = folder_col.push(f_btn);
        }

        let mut mail_col = column![].spacing(6);
        for mail in &self.envelopes {
            let id = mail.id.clone();
            let sender = mail.from.as_ref().and_then(|f| f.name.clone().or_else(|| f.addr.clone())).unwrap_or_else(|| "Unknown".into());
            let subject = mail.subject.clone().unwrap_or_else(|| "(no subject)".into());

            let row_item = button(
                column![
                    text(sender).size(theme.font_size_normal).bold().color(theme.text),
                    text(subject).size(theme.font_size_normal - 2).color(theme.muted),
                ]
                .spacing(2),
            )
            .on_press(EmailMessage::SelectMail(id))
            .padding(10)
            .width(Length::Fill)
            .style(|_t, _s| button::Style { background: None, ..Default::default() });

            mail_col = mail_col.push(row_item);
        }

        let preview_content = scrollable(
            text(&self.mail_body)
                .size(theme.font_size_normal)
                .color(theme.text),
        )
        .height(Length::Fill);

        let main_row = row![
            container(folder_col).width(Length::Fixed(180.0)).padding(10),
            container(scrollable(mail_col)).width(Length::Fixed(360.0)).padding(10),
            container(preview_content).width(Length::Fill).padding(14),
        ]
        .spacing(12);

        let card_bg = ShapeBox::new()
            .shape(ShapeType::Rounded(12))
            .background(theme.background)
            .border(theme.border, theme.border_width);

        let card_content = container(
            column![
                row![
                    text("✉ EMAIL CLIENT (HIMALAYA)")
                        .size(theme.font_size_overlay)
                        .color(theme.text),
                    row![].width(Length::Fill),
                    button(text("✏ Compose").size(12).bold().color(theme.success))
                        .on_press(EmailMessage::OpenComposer)
                        .style(|_t, _s| button::Style { background: None, ..Default::default() }),
                ]
                .align_y(Alignment::Center),
                main_row,
                text("ESC to dismiss  •  Ctrl+Enter to send in composer")
                    .size(11)
                    .color(theme.muted)
                    .align_x(Alignment::Center),
            ]
            .spacing(14)
            .padding(20),
        )
        .width(Length::Fixed(1280.0))
        .height(Length::Fixed(800.0));

        let card = Stack::new().push(card_bg.view()).push(card_content);

        let backdrop = button(container(column![]).width(Length::Fill).height(Length::Fill))
            .on_press(EmailMessage::Dismiss)
            .style(|_t, _s| button::Style {
                background: Some(iced::Background::Color(Color::from_rgba8(0, 0, 0, 0.6))),
                ..Default::default()
            });

        Stack::new()
            .push(backdrop)
            .push(container(card).center_x(Length::Fill).center_y(Length::Fill))
            .into()
    }
}
