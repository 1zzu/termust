//! The snippet record stored inside the vault.
//!
//! A snippet is also the encrypted payload for its object kind, so the JSON
//! inside the ciphertext is exactly the struct below. Its `version` field is
//! what lets the format change later, the same way the inventory records do it.

use serde::{Deserialize, Serialize};

use super::SnippetError;

/// The object kind of a snippet. Kinds are part of the encryption's Additional
/// Authenticated Data, so changing this makes existing objects unreadable.
pub const KIND_SNIPPET: &str = "snippet";

/// The payload version this build writes.
pub const PAYLOAD_VERSION: u8 = 1;

/// A snippet name is a short label, counted in characters like the other names.
pub const MAX_NAME_LEN: usize = 120;
/// The content limit, counted in bytes of UTF-8 rather than characters.
pub const MAX_CONTENT_LEN: usize = 64 * 1024;

/// One saved command.
///
/// The whole record is encrypted, name and content together, so no column of the
/// database ever holds either of them. A snippet deliberately has no relation to
/// a host, a group or a credential, and v1 has no tags, folders, variables or
/// secret fields of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub version: u8,
    pub id: String,
    pub name: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// What the frontend sends to create or update a snippet.
///
/// The id and the timestamps are chosen by the backend, and
/// `deny_unknown_fields` turns an attempt to send one into a refused request
/// rather than a field that is quietly ignored.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnippetInput {
    pub name: String,
    pub content: String,
}

/// What the frontend sends to run a snippet.
///
/// Only an id crosses the boundary: the content is read from the vault, so
/// nothing the frontend sends can decide what a session receives, and there is
/// no way to send arbitrary text through this command.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunSnippetRequest {
    pub session_id: String,
    pub snippet_id: String,
}

impl RunSnippetRequest {
    /// Rejects a request that cannot name both a session and a snippet. The text
    /// is what the command reports, so it is already the final message.
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.session_id.trim().is_empty() {
            return Err("The request named no session.".to_string());
        }
        if self.snippet_id.trim().is_empty() {
            return Err("The request named no snippet.".to_string());
        }
        Ok(())
    }
}

/// A required name, trimmed, without building the trimmed copy.
///
/// This is the rule itself. `name` hands back the trimmed string a command
/// stores; the backup validator uses this one to check a decoded snippet
/// without allocating a second copy of it.
pub(super) fn check_name(value: &str) -> Result<&str, SnippetError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SnippetError::Invalid(
            "Snippet name cannot be empty.".to_string(),
        ));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(SnippetError::Invalid(format!(
            "Snippet name must be at most {MAX_NAME_LEN} characters."
        )));
    }
    Ok(trimmed)
}

/// A required name, trimmed to its final value. The message names the field
/// only; it never repeats the value.
pub(super) fn name(value: &str) -> Result<String, SnippetError> {
    Ok(check_name(value)?.to_string())
}

/// The content rule, without building the copy. The content is never trimmed
/// and never normalised, so a command stores exactly what it checked.
pub(super) fn check_content(value: &str) -> Result<&str, SnippetError> {
    if value.is_empty() {
        return Err(SnippetError::Invalid(
            "Snippet content cannot be empty.".to_string(),
        ));
    }
    if value.len() > MAX_CONTENT_LEN {
        return Err(SnippetError::Invalid(format!(
            "Snippet content is longer than {MAX_CONTENT_LEN} bytes."
        )));
    }
    if value.contains('\0') {
        return Err(SnippetError::Invalid(
            "Snippet content cannot contain a NUL character.".to_string(),
        ));
    }
    Ok(value)
}

/// The content exactly as the user wrote it: never trimmed, never normalised,
/// with its quoting and line breaks left alone.
///
/// Nothing here looks at what the text means. Termust does not parse shell
/// syntax, so the only checks are ones that cannot disagree with a shell: a
/// non-empty payload, a byte limit, and no NUL byte, which no terminal input
/// can carry.
pub(super) fn content(value: &str) -> Result<String, SnippetError> {
    Ok(check_content(value)?.to_string())
}
