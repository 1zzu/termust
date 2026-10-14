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

use tauri::AppHandle;
use zeroize::Zeroizing;

use crate::vault::{VaultError, blocking};

use model::{CredentialSummary, Group, Host, HostInput};

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
