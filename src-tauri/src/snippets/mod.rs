//! Snippets: short commands kept encrypted in the vault.
//!
//! A snippet is one more `vault_objects` row whose `kind` is `snippet`, written
//! and read through the Phase 1A vault API. There is no separate table, no
//! separate encryption, and no relation to a host, a group or a credential.
//!
//! Nothing in this module decides to run anything. A snippet is only ever sent
//! to a session the user already has, through `run_snippet`, after the frontend
//! has shown its full content and the user confirmed. There is no auto-run on
//! connect, no local shell, and no way to send text the vault did not resolve:
//! the command takes an id, never content.
//!
//! The module is split the way the inventory is:
//!
//! - `model.rs`: the record, the request DTOs and the validation.
//! - `service.rs`: the CRUD.
//! - this file: the Tauri commands, the error mapping and the execution path.

mod model;
mod service;

#[cfg(test)]
mod tests;

use std::fmt;

use tauri::{AppHandle, Runtime, State};

use crate::ssh::{self, SshState};
use crate::vault::{VaultError, blocking};

pub(crate) use model::{RunSnippetRequest, Snippet, SnippetInput};

/// The service calls the live and snippet tests need, renamed because the
/// commands below already own these names.
#[cfg(test)]
pub(crate) use service::{create_snippet as create_snippet_record, get_snippet};

/// Everything a snippet operation can fail with.
///
/// The variants are for this code's own use; the `Display` text is what the
/// frontend sees, and it never contains the snippet's name or content, a
/// ciphertext, a SQL statement or a database path.
#[derive(Debug)]
pub enum SnippetError {
    /// A field the user can fix. The message names the field, never its value.
    Invalid(String),
    /// The snippet was asked for by an id that is unknown or deleted.
    NotFound,
    /// An object was stored with a payload version this build does not know.
    UnsupportedPayloadVersion { found: u64, supported: u8 },
    /// An object was read as a kind it is not.
    WrongKind { expected: &'static str },
    /// The encrypted object could not be decrypted or parsed.
    Corrupt,
    /// Guards a failure that cannot happen for a record this module wrote.
    Internal,
    /// Anything the vault itself reported.
    Vault(VaultError),
}

impl fmt::Display for SnippetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::NotFound => formatter.write_str("That snippet no longer exists."),
            Self::UnsupportedPayloadVersion { found, supported } => write!(
                formatter,
                "That object uses payload version {found}, but this build supports version {supported}."
            ),
            Self::WrongKind { expected } => {
                write!(formatter, "That object is not a {expected}.")
            }
            Self::Corrupt => {
                formatter.write_str("A stored object is corrupted and could not be read.")
            }
            Self::Internal => formatter.write_str("That operation could not be completed."),
            // The vault's own messages are safe to show, except for a storage
            // failure, whose text comes from SQLite.
            Self::Vault(VaultError::Storage(_)) => {
                formatter.write_str("The vault could not be read or written on this device.")
            }
            Self::Vault(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for SnippetError {}

impl From<VaultError> for SnippetError {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

// --- CRUD -----------------------------------------------------------------

#[tauri::command]
pub async fn list_snippets<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Snippet>, String> {
    blocking(app, |vault| service::list_snippets(vault)).await
}

#[tauri::command]
pub async fn create_snippet<R: Runtime>(
    app: AppHandle<R>,
    snippet: SnippetInput,
) -> Result<Snippet, String> {
    blocking(app, move |vault| service::create_snippet(vault, &snippet)).await
}

#[tauri::command]
pub async fn update_snippet<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    snippet: SnippetInput,
) -> Result<Snippet, String> {
    blocking(app, move |vault| {
        service::update_snippet(vault, &id, &snippet)
    })
    .await
}

#[tauri::command]
pub async fn delete_snippet<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    blocking(app, move |vault| service::delete_snippet(vault, &id)).await
}

// --- running one ----------------------------------------------------------

/// Sends one stored snippet to one session.
///
/// The frontend names a session and a snippet and nothing else. The content is
/// read from the vault here, so it is the only thing a session can be sent
/// through this command, and the command never writes to a local process.
#[tauri::command]
pub async fn run_snippet<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SshState>,
    request: RunSnippetRequest,
) -> Result<(), String> {
    run(&app, &state, &request).await
}

/// The testable half of `run_snippet`: read the snippet, then send it.
///
/// The vault is read in one blocking call, which takes the lock and releases it
/// again before this function returns. Nothing below that call can hold it, so
/// the lock is never held while the session is written to.
async fn run<R: Runtime>(
    app: &AppHandle<R>,
    state: &SshState,
    request: &RunSnippetRequest,
) -> Result<(), String> {
    request.validate()?;

    let snippet_id = request.snippet_id.clone();
    let snippet = blocking(app.clone(), move |vault| {
        service::get_snippet(vault, &snippet_id)
    })
    .await?;

    send_content(state, &request.session_id, &snippet.content)
}

/// Hands one snippet's content to one session.
///
/// This is the whole execution path apart from reading the vault, and it takes
/// no vault handle at all: the lock cannot be held here. The input goes through
/// the same sender terminal keystrokes use, so there is no second input path.
pub(crate) fn send_content(
    state: &SshState,
    session_id: &str,
    content: &str,
) -> Result<(), String> {
    ssh::write(state, session_id, content)?;
    match submit_keystroke(content) {
        Some(submit) => ssh::write(state, session_id, submit),
        None => Ok(()),
    }
}

/// The one submit keystroke, or `None` when the content already ends with one.
///
/// Enter is a carriage return in the terminal, so that is the byte the keyboard
/// sends and the byte used here. A snippet that already ends with a line break
/// is never given a second one, which would submit an empty command as well.
fn submit_keystroke(content: &str) -> Option<&'static str> {
    match content.as_bytes().last() {
        Some(b'\n') | Some(b'\r') => None,
        _ => Some("\r"),
    }
}
