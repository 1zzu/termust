//! Groups, hosts and credentials stored as encrypted vault objects.
//!
//! There is no separate table and no separate encryption for these records:
//! each one is a `vault_objects` row whose `kind` is `group`, `host` or
//! `credential`, written and read through the Phase 1A vault API. This module
//! adds the domain on top: the record shapes, the validation and the
//! relationships between them.
//!
//! The frontend never sends an object kind, a nonce, ciphertext, a crypto
//! version or a database path, and it never receives a credential secret. Every
//! command takes plain domain fields and answers with a domain type.
//!
//! - `model.rs`: the records, the request DTOs and the validation.
//! - `service.rs`: the CRUD and the relationship rules.
//! - this file: the Tauri commands and the error mapping.

mod model;
mod service;

#[cfg(test)]
mod tests;

use std::fmt;

use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

use crate::vault::{VaultError, blocking};

use model::{CredentialSummary, Group, KnownHost};

/// The pieces the SSH layer needs from here: an approved host key is an
/// inventory record like any other, and a saved host is read the same way a
/// command reads it. A [`Credential`] holds a secret, so it stays crate-private
/// and never reaches a command's answer.
pub(crate) use model::{Credential, CredentialSecret, Host, HostInput, KnownKey};
pub(crate) use service::{credential_for_host, find_known_host, remember_known_host};

/// The service calls other parts of the crate need: the OpenSSH import creates a
/// host through the same validation the host form uses, and reads the saved
/// hosts the same way a command does. Renamed, because the commands above
/// already own these names.
pub(crate) use service::{create_host as create_host_record, list_hosts as list_host_records};

/// What a portable backup needs from here: the one function that decodes a
/// record from its kind and its plaintext payload, and the one that re-checks a
/// decoded record against the same rules the commands apply. A backup is a copy
/// of the vault, so it has to read, check and write the very same records the
/// commands do.
pub(crate) use service::{Record, check_record, decode_record};

/// The service calls a test needs to set up a record the way the commands would.
/// Test-only, and renamed for the same reason.
#[cfg(test)]
pub(crate) use service::{
    create_group as create_group_record, create_password_credential as create_password_record,
    create_private_key_credential as create_private_key_record, list_groups as list_group_records,
    list_known_hosts as list_known_host_records, update_host as update_host_record,
};

/// Everything an inventory operation can fail with.
///
/// The variants are for this code's own use; the `Display` text is what the
/// frontend sees, and it never contains a secret, a ciphertext, a SQL statement
/// or a database path.
#[derive(Debug)]
pub enum InventoryError {
    /// A field the user can fix. The message names the field, never its value.
    Invalid(String),
    /// Another group already uses this name.
    DuplicateGroupName(String),
    /// A host still points at this group.
    GroupInUse,
    /// A host still points at this credential.
    CredentialInUse,
    /// A saved host was asked for by an id that is unknown or deleted.
    HostNotFound,
    /// A saved host does not describe a usable target.
    IncompleteHost,
    /// A saved host has no credential linked to it.
    HostHasNoCredential,
    /// A saved host points at a credential that is unknown or deleted.
    CredentialNotFound,
    /// An object was stored with a payload version this build does not know.
    UnsupportedPayloadVersion { found: u64, supported: u8 },
    /// An object was read as a kind it is not.
    WrongKind { expected: &'static str },
    /// An object was found of a kind this build does not write at all.
    ///
    /// The kind itself is deliberately absent: it comes from stored or imported
    /// data, and no message here repeats untrusted text.
    UnknownKind,
    /// The encrypted object could not be decrypted or parsed.
    Corrupt,
    /// Guards a failure that cannot happen for a record this module wrote.
    Internal,
    /// Anything the vault itself reported.
    Vault(VaultError),
}

impl fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::DuplicateGroupName(name) => {
                write!(formatter, "A group named \"{name}\" already exists.")
            }
            Self::GroupInUse => formatter
                .write_str("That group is still used by a host. Move or delete those hosts first."),
            Self::CredentialInUse => formatter.write_str(
                "That credential is still used by a host. Replace it on those hosts first.",
            ),
            Self::HostNotFound => formatter.write_str("That saved host no longer exists."),
            Self::IncompleteHost => formatter.write_str(
                "This saved host is missing its address, port or username. Edit it before connecting.",
            ),
            Self::HostHasNoCredential => formatter.write_str(
                "This saved host has no credential yet. Edit the host and choose one.",
            ),
            Self::CredentialNotFound => formatter.write_str(
                "The credential for this saved host no longer exists. Edit the host and choose another one.",
            ),
            Self::UnsupportedPayloadVersion { found, supported } => write!(
                formatter,
                "That object uses payload version {found}, but this build supports version {supported}."
            ),
            Self::WrongKind { expected } => {
                write!(formatter, "That object is not a {expected}.")
            }
            Self::UnknownKind => formatter.write_str(
                "That object is of a kind this build does not know how to read.",
            ),
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

impl std::error::Error for InventoryError {}

impl From<VaultError> for InventoryError {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

// --- groups ---------------------------------------------------------------

#[tauri::command]
pub async fn list_groups(app: AppHandle) -> Result<Vec<Group>, String> {
    blocking(app, |vault| service::list_groups(vault)).await
}

#[tauri::command]
pub async fn create_group(app: AppHandle, name: String) -> Result<Group, String> {
    blocking(app, move |vault| service::create_group(vault, &name)).await
}

#[tauri::command]
pub async fn rename_group(app: AppHandle, id: String, name: String) -> Result<Group, String> {
    blocking(app, move |vault| service::rename_group(vault, &id, &name)).await
}

#[tauri::command]
pub async fn delete_group(app: AppHandle, id: String) -> Result<(), String> {
    blocking(app, move |vault| service::delete_group(vault, &id)).await
}

// --- hosts ----------------------------------------------------------------

#[tauri::command]
pub async fn list_hosts(app: AppHandle) -> Result<Vec<Host>, String> {
    blocking(app, |vault| service::list_hosts(vault)).await
}

#[tauri::command]
pub async fn get_host(app: AppHandle, id: String) -> Result<Host, String> {
    blocking(app, move |vault| service::get_host(vault, &id)).await
}

#[tauri::command]
pub async fn create_host(app: AppHandle, host: HostInput) -> Result<Host, String> {
    blocking(app, move |vault| service::create_host(vault, &host)).await
}

#[tauri::command]
pub async fn update_host(app: AppHandle, id: String, host: HostInput) -> Result<Host, String> {
    blocking(app, move |vault| service::update_host(vault, &id, &host)).await
}

#[tauri::command]
pub async fn delete_host(app: AppHandle, id: String) -> Result<(), String> {
    blocking(app, move |vault| service::delete_host(vault, &id)).await
}

// --- credentials ----------------------------------------------------------

#[tauri::command]
pub async fn list_credentials(app: AppHandle) -> Result<Vec<CredentialSummary>, String> {
    blocking(app, |vault| service::list_credentials(vault)).await
}

#[tauri::command]
pub async fn create_password_credential(
    app: AppHandle,
    name: String,
    password: String,
) -> Result<CredentialSummary, String> {
    blocking(app, move |vault| {
        // The secret is wiped on the blocking thread once it has been stored.
        let password = Zeroizing::new(password);
        service::create_password_credential(vault, &name, password.as_str())
    })
    .await
}

#[tauri::command]
pub async fn create_private_key_credential(
    app: AppHandle,
    name: String,
    private_key: String,
    passphrase: Option<String>,
) -> Result<CredentialSummary, String> {
    blocking(app, move |vault| {
        let private_key = Zeroizing::new(private_key);
        let passphrase = passphrase.map(Zeroizing::new);
        service::create_private_key_credential(
            vault,
            &name,
            private_key.as_str(),
            passphrase.as_ref().map(|value| value.as_str()),
        )
    })
    .await
}

#[tauri::command]
pub async fn rename_credential(
    app: AppHandle,
    id: String,
    name: String,
) -> Result<CredentialSummary, String> {
    blocking(app, move |vault| {
        service::rename_credential(vault, &id, &name)
    })
    .await
}

#[tauri::command]
pub async fn replace_password(app: AppHandle, id: String, password: String) -> Result<(), String> {
    blocking(app, move |vault| {
        let password = Zeroizing::new(password);
        service::replace_password(vault, &id, password.as_str())
    })
    .await
}

#[tauri::command]
pub async fn replace_private_key(
    app: AppHandle,
    id: String,
    private_key: String,
    passphrase: Option<String>,
) -> Result<(), String> {
    blocking(app, move |vault| {
        let private_key = Zeroizing::new(private_key);
        let passphrase = passphrase.map(Zeroizing::new);
        service::replace_private_key(
            vault,
            &id,
            private_key.as_str(),
            passphrase.as_ref().map(|value| value.as_str()),
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_credential(app: AppHandle, id: String) -> Result<(), String> {
    blocking(app, move |vault| service::delete_credential(vault, &id)).await
}

// --- known hosts ----------------------------------------------------------

#[tauri::command]
pub async fn list_known_hosts<R: Runtime>(app: AppHandle<R>) -> Result<Vec<KnownHost>, String> {
    blocking(app, |vault| service::list_known_hosts(vault)).await
}

/// Forgets one approved key. There is deliberately no command that writes one:
/// a key is only ever trusted through the SSH host-key prompt.
#[tauri::command]
pub async fn forget_known_host<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    blocking(app, move |vault| service::forget_known_host(vault, &id)).await
}
