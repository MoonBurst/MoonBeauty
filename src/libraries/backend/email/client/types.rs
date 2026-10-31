//! Shared email data types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeEnvelope {
    pub id: String,
    pub subject: String,
    pub sender: String,
    pub sender_email: String,
    pub is_starred: bool,
    pub is_read: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedEmail {
    pub text: String,
    pub images: Vec<std::path::PathBuf>,
    pub attachments: Vec<(String, Vec<u8>)>,
}
