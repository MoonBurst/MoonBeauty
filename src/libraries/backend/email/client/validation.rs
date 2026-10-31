//! Validation helpers for IMAP commands.
//!
//! Both functions are security-critical: they prevent crafted strings
//! from being interpolated into IMAP commands sent to the server.

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
