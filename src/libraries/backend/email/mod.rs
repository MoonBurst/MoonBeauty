//! Email backend: IMAP/SMTP client, local folder sorting, contacts.
//!
//! - `client`: config, IMAP fetch, SMTP send, body parsing, contacts
//! - `local_sort`: sender-based local folder auto-sorting

pub mod client;
pub mod local_sort;

pub use client::*;
pub use local_sort::*;
