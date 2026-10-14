//! The records stored inside the vault: groups, hosts and credentials.
//!
//! Each record is also the encrypted payload for its object kind, so the JSON
//! inside the ciphertext is exactly the struct below. There is one type per
//! record, which means no second model can drift away from what is really
//! stored; the `version` field is what lets the format change later.
//!
//! Credential secrets are wrapped in `Zeroizing`, so their bytes are wiped when
//! the record is dropped, and the `Debug` implementations redact them so they
//! cannot reach a log or a test failure message.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::InventoryError;

/// The object kind of a group. Kinds are part of the encryption's Additional
/// Authenticated Data, so changing one makes existing objects unreadable.
pub const KIND_GROUP: &str = "group";
pub const KIND_HOST: &str = "host";
pub const KIND_CREDENTIAL: &str = "credential";
pub const KIND_KNOWN_HOST: &str = "known_host";

/// The payload version this build writes for every new record.
pub const PAYLOAD_VERSION: u8 = 1;

/// Field limits. They live in one place so the backend and the UI agree.
pub const MAX_NAME_LEN: usize = 100;
pub const MAX_LABEL_LEN: usize = 100;
pub const MAX_ADDRESS_LEN: usize = 255;
pub const MAX_USERNAME_LEN: usize = 100;
/// A key algorithm name and a `SHA256:` fingerprint are both short.
pub const MAX_ALGORITHM_LEN: usize = 64;
pub const MAX_FINGERPRINT_LEN: usize = 128;
/// A password, a private key or a passphrase can be large, but not unbounded.
pub const MAX_SECRET_LEN: usize = 16 * 1024;

/// A group of hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub version: u8,
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A saved SSH target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub version: u8,
    pub id: String,
    pub label: String,
    pub address: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<String>,
    pub credential_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A credential and its secret, as it is stored. Only the Rust side ever sees
/// this type; the frontend receives a [`CredentialSummary`].
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub version: u8,
    pub id: String,
    pub name: String,
    pub secret: CredentialSecret,
    pub created_at: i64,
    pub updated_at: i64,
}

/// The secret half of a credential.
#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialSecret {
    Password {
        password: Zeroizing<String>,
    },
    PrivateKey {
        private_key: Zeroizing<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<Zeroizing<String>>,
    },
}

impl CredentialSecret {
    pub fn kind(&self) -> CredentialKind {
        match self {
            Self::Password { .. } => CredentialKind::Password,
            Self::PrivateKey { .. } => CredentialKind::PrivateKey,
        }
    }
}

/// Redacts the secret: neither the value nor its length reaches the output.
impl fmt::Debug for CredentialSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Password { .. } => formatter.write_str("Password { password: \"***\" }"),
            Self::PrivateKey { passphrase, .. } => formatter
                .debug_struct("PrivateKey")
                .field("private_key", &"***")
                .field("passphrase", &passphrase.as_ref().map(|_| "***"))
                .finish(),
        }
    }
}

/// The kind of secret a credential holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    Password,
    PrivateKey,
}

/// Everything the frontend is allowed to know about a stored credential. There
/// is deliberately no secret field, so a secret can never be sent back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSummary {
    pub id: String,
    pub name: String,
    pub kind: CredentialKind,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Credential> for CredentialSummary {
    fn from(credential: Credential) -> Self {
        // The secret is dropped (and zeroized) when `credential` goes out of
        // scope; only these fields survive.
        let kind = credential.secret.kind();
        Self {
            id: credential.id,
            name: credential.name,
            kind,
            created_at: credential.created_at,
            updated_at: credential.updated_at,
        }
    }
}

/// What the frontend sends to create or update a host.
///
/// The id, the timestamps and the object kind are all chosen by the backend
/// from the fields below; nothing the frontend sends can name them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInput {
    pub label: String,
    pub address: String,
    /// Kept wide so an out-of-range port is a validation error rather than a
    /// deserialization error.
    pub port: i64,
    pub username: String,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub credential_id: Option<String>,
}

/// One SSH host key this device has been told to trust.
///
/// The record is keyed by `host_key_id`, the canonical identity the SSH layer
/// builds from the host and the port. The address itself lives inside the
/// encrypted payload, never in a plaintext column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub version: u8,
    pub id: String,
    pub host_key_id: String,
    pub key_algorithm: String,
    pub fingerprint_sha256: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// The key half of a known host: what a server has to present to be trusted.
///
/// Both parts are compared. A key that keeps its fingerprint but changes its
/// algorithm is a different key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownKey {
    pub algorithm: String,
    pub fingerprint: String,
}

impl KnownHost {
    pub fn key(&self) -> KnownKey {
        KnownKey {
            algorithm: self.key_algorithm.clone(),
            fingerprint: self.fingerprint_sha256.clone(),
        }
    }
}

/// A required text field, trimmed, without building the trimmed copy.
///
/// This is the rule itself. `required` hands back the trimmed string a command
/// stores; the backup validator uses this one to check a record it decoded
/// without allocating a second copy of it.
pub(super) fn check_required<'a>(
    value: &'a str,
    field: &str,
    max: usize,
) -> Result<&'a str, InventoryError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(InventoryError::Invalid(format!("{field} cannot be empty.")));
    }
    if trimmed.chars().count() > max {
        return Err(InventoryError::Invalid(format!(
            "{field} must be at most {max} characters."
        )));
    }
    Ok(trimmed)
}

/// A required text field, trimmed to its final value.
///
/// The message names the field only; it never repeats the value.
pub fn required(value: &str, field: &str, max: usize) -> Result<String, InventoryError> {
    Ok(check_required(value, field, max)?.to_string())
}

/// A port a TCP connection can actually use.
pub fn valid_port(port: i64) -> Result<u16, InventoryError> {
    if !(1..=65535).contains(&port) {
        return Err(InventoryError::Invalid(
            "Port must be between 1 and 65535.".to_string(),
        ));
    }
    Ok(port as u16)
}

/// An optional relationship id. Absent and empty both mean "no relationship".
pub fn optional_id(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// A secret: never empty and never unbounded, without copying it.
///
/// A secret is never trimmed, because a password may legitimately contain
/// leading or trailing spaces, so the value a command stores is the value it
/// was given. The backup validator uses this to check a decoded credential
/// without making a second copy of its secret.
pub(super) fn check_secret(value: &str, field: &str) -> Result<(), InventoryError> {
    if value.is_empty() {
        return Err(InventoryError::Invalid(format!("{field} cannot be empty.")));
    }
    if value.len() > MAX_SECRET_LEN {
        return Err(InventoryError::Invalid(format!(
            "{field} is longer than {MAX_SECRET_LEN} bytes."
        )));
    }
    Ok(())
}

/// A secret exactly as the user gave it: never trimmed, because a password may
/// legitimately contain leading or trailing spaces.
pub fn secret(value: &str, field: &str) -> Result<Zeroizing<String>, InventoryError> {
    check_secret(value, field)?;
    Ok(Zeroizing::new(value.to_string()))
}

/// An optional secret: absent and empty both mean "not set".
pub fn optional_secret(value: Option<&str>) -> Result<Option<Zeroizing<String>>, InventoryError> {
    match value {
        None => Ok(None),
        Some("") => Ok(None),
        Some(text) => Ok(Some(secret(text, "Passphrase")?)),
    }
}
