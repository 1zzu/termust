//! Importing hosts from a basic OpenSSH client config.
//!
//! This is a deliberately small subset, not an OpenSSH-compatible parser: one
//! literal alias per `Host` block, `HostName`, `User` and `Port`, and a note for
//! everything else. The parts it does not read are never guessed at — they are
//! reported, and the user decides what to do about them.
//!
//! Nothing in this module reads a file. The frontend reads the config the user
//! chose and sends its text; a `HostName`, an `Include` or an `IdentityFile` can
//! therefore never name something this code opens. Nothing runs a process,
//! resolves a name or opens a socket either: [`parser::parse`] is a pure
//! function over text.
//!
//! The module is split the way the rest of the crate is:
//!
//! - `model.rs`: the preview, the results and the request DTOs.
//! - `parser.rs`: the pure parser, and its own unit tests.
//! - `service.rs`: the duplicate check and the writes, through the host service.
//! - this file: the two Tauri commands and the error mapping.

pub(crate) mod model;
mod parser;
mod service;

#[cfg(test)]
mod tests;

use std::fmt;

use tauri::{AppHandle, Runtime};

use crate::vault::{VaultError, blocking};

pub(crate) use model::{ImportPreview, ImportRequest, ImportSummary, PreviewRequest};

/// The parser and service calls the tests drive directly, so a preview can be
/// checked without an IPC round trip.
#[cfg(test)]
pub(crate) use model::MAX_CONFIG_BYTES;
#[cfg(test)]
pub(crate) use parser::parse;
#[cfg(test)]
pub(crate) use service::{import, preview, same_target};

/// Everything an OpenSSH import can fail with.
///
/// The variants are for this code's own use; the `Display` text is what the
/// frontend sees, and it never contains the config, a value from it, a
/// ciphertext, a SQL statement or a database path.
#[derive(Debug)]
pub enum OpenSshError {
    /// Something the user can fix. The message names the problem, never a value.
    Invalid(String),
    /// Anything the vault itself reported.
    Vault(VaultError),
}

impl fmt::Display for OpenSshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            // The vault's own messages are safe to show, except for a storage
            // failure, whose text comes from SQLite.
            Self::Vault(VaultError::Storage(_)) => {
                formatter.write_str("The vault could not be read or written on this device.")
            }
            Self::Vault(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for OpenSshError {}

impl From<VaultError> for OpenSshError {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

impl From<crate::inventory::InventoryError> for OpenSshError {
    /// Only the read behind the duplicate check goes through this: a host that
    /// cannot be created is reported per entry, never as a failure of the whole
    /// import.
    fn from(error: crate::inventory::InventoryError) -> Self {
        match error {
            // A locked vault stays a locked vault rather than becoming a
            // validation problem, so the message is the vault's own.
            crate::inventory::InventoryError::Vault(error) => Self::Vault(error),
            other => Self::Invalid(other.to_string()),
        }
    }
}

// --- commands -------------------------------------------------------------

/// Parses a config and reports what could be imported. Writes nothing.
#[tauri::command]
pub async fn preview_openssh_config<R: Runtime>(
    app: AppHandle<R>,
    request: PreviewRequest,
) -> Result<ImportPreview, String> {
    blocking(app, move |vault| service::preview(vault, &request.content)).await
}

/// Imports the entries the user selected, one host at a time.
#[tauri::command]
pub async fn import_openssh_hosts<R: Runtime>(
    app: AppHandle<R>,
    request: ImportRequest,
) -> Result<ImportSummary, String> {
    blocking(app, move |vault| {
        service::import(vault, &request.content, &request.selected_entry_indexes)
    })
    .await
}
