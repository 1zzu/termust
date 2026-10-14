//! Turns the vault's persistent data into ordinary Rust values and back.
//!
//! Two version numbers travel with the data: `schema_version` describes the
//! SQLite layout and `crypto_version` describes how a key or a payload was
//! encrypted. Keeping them separate lets the storage format and the crypto
//! format move on their own.

use serde::{Deserialize, Serialize};

use super::VaultError;

/// The SQLite layout this build writes and understands.
pub const SCHEMA_VERSION: i64 = 1;

/// The crypto format this build writes and understands.
pub const CRYPTO_VERSION: i64 = 1;

/// The version of the encrypted object envelope.
pub const ENVELOPE_VERSION: u8 = 1;

/// The Argon2id identifier stored in `vault_meta.kdf_algorithm`.
pub const KDF_ARGON2ID: &str = "argon2id";

/// The single `vault_meta` row: everything needed to derive the Key Encryption
/// Key again and to unwrap the Vault Key. It never holds the master password,
/// the Key Encryption Key or the Vault Key itself.
#[derive(Debug, Clone)]
pub struct VaultMeta {
    pub id: String,
    pub schema_version: i64,
    pub crypto_version: i64,
    pub kdf_algorithm: String,
    pub kdf_salt: Vec<u8>,
    pub kdf_memory_cost: u32,
    pub kdf_time_cost: u32,
    pub kdf_parallelism: u32,
    pub wrapped_vault_key: Vec<u8>,
    pub wrap_nonce: Vec<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A `vault_objects` row: the object as it sits on disk, still encrypted.
#[derive(Debug, Clone)]
pub struct StoredObject {
    pub id: String,
    pub kind: String,
    pub encrypted_payload: Vec<u8>,
    pub nonce: Vec<u8>,
    pub crypto_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

/// A vault object once it has been decrypted, as the rest of the app sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultObject {
    pub id: String,
    pub kind: String,
    pub schema_version: u8,
    pub payload: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// What is actually encrypted for one vault object.
///
/// The envelope carries its own schema version so the payload format can change
/// later without touching the SQLite layout: all that a migration needs is to
/// decrypt, read the version and rewrite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub schema_version: u8,
    pub kind: String,
    pub payload: String,
}

impl Envelope {
    pub fn new(kind: &str, payload: &str) -> Self {
        Self {
            schema_version: ENVELOPE_VERSION,
            kind: kind.to_string(),
            payload: payload.to_string(),
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, VaultError> {
        serde_json::to_vec(self).map_err(|_| VaultError::Corrupt("object envelope"))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VaultError> {
        serde_json::from_slice(bytes).map_err(|_| VaultError::Corrupt("object envelope"))
    }
}
