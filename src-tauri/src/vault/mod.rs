//! The encrypted local vault.
//!
//! A vault is one SQLite database in the app data directory holding two kinds
//! of rows: a single `vault_meta` row with the salt, the Argon2id parameters
//! and the wrapped Vault Key, and any number of `vault_objects` rows holding
//! XChaCha20-Poly1305 ciphertext. It never stores the master password, the
//! Vault Key, or a plaintext payload.
//!
//! While the vault is unlocked the Vault Key lives only in this process's
//! memory. Locking drops it, which zeroizes the bytes; the same happens when the
//! app exits and the state is torn down. There is no auto-lock timer yet.
//!
//! The module is split by concern:
//!
//! - `crypto.rs`: deriving keys and encrypting/decrypting.
//! - `storage.rs`: the SQLite schema and queries.
//! - `model.rs`: the stored types and the version numbers.
//!
//! This file holds the vault state, the object API the tests use directly, and
//! the four Tauri commands the frontend can call.

mod crypto;
mod model;
mod storage;

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use zeroize::Zeroizing;

use crypto::{KdfParams, SecretKey};
use model::{CRYPTO_VERSION, Envelope, KDF_ARGON2ID, SCHEMA_VERSION, StoredObject, VaultMeta};

// `VaultObject` is what the object API answers with, so callers outside this
// module need to be able to name it. `now` is also how a domain record builds
// its timestamps, so both are part of the vault's public API.
pub use model::VaultObject;
pub use storage::{DB_FILE, now};

/// What the frontend is told about the vault. The windows the app can be in are
/// exactly these three.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultStatus {
    /// No vault exists on this device yet.
    Uninitialized,
    /// A vault exists but the Vault Key is not in memory.
    Locked,
    /// The Vault Key is in memory and objects can be read.
    Unlocked,
}

/// Every failure the vault can report.
///
/// The variants exist so this code can tell its own failures apart, but only the
/// generic `Unlock` message is ever shown for a failed unlock: anything more
/// specific about *why* it failed would tell an attacker whether the password or
/// the data was wrong.
#[derive(Debug)]
pub enum VaultError {
    /// Used for every unlock failure that could reveal the password's validity.
    Unlock,
    /// An object operation was attempted while the vault was locked.
    Locked,
    /// The database has no vault yet.
    NotInitialized,
    /// A vault already exists on this device.
    AlreadyInitialized,
    /// The master password was empty.
    EmptyPassword,
    /// The requested object is not there, or was deleted.
    NotFound,
    /// A version this build does not understand.
    UnsupportedVersion { found: i64, supported: i64 },
    /// Data that should decrypt but does not.
    Corrupt(&'static str),
    /// The stored key-derivation parameters are invalid.
    Kdf,
    /// Encryption or decryption failed.
    Crypto,
    /// The operating system's random source failed.
    Random,
    /// The database could not be read or written.
    Storage(String),
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The one message an unlock failure ever shows, whatever its cause.
            Self::Unlock => formatter.write_str("Invalid master password or corrupted vault"),
            Self::Locked => formatter.write_str("The vault is locked. Unlock it first."),
            Self::NotInitialized => formatter.write_str("No vault has been created yet."),
            Self::AlreadyInitialized => {
                formatter.write_str("A vault already exists on this device.")
            }
            Self::EmptyPassword => formatter.write_str("Enter a master password."),
            Self::NotFound => formatter.write_str("That object does not exist."),
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "This vault uses version {found}, but this build supports version {supported}."
            ),
            Self::Corrupt(what) => write!(formatter, "Vault data is corrupted ({what})."),
            Self::Kdf => formatter.write_str("The stored key-derivation parameters are invalid."),
            Self::Crypto => formatter.write_str("Vault data could not be decrypted."),
            Self::Random => formatter.write_str("The operating system's random source failed."),
            Self::Storage(message) => write!(formatter, "Vault storage error: {message}"),
        }
    }
}

impl std::error::Error for VaultError {}

/// One vault database, plus the Vault Key while it is unlocked.
pub struct Vault {
    path: PathBuf,
    /// The parameters a *new* vault is created with. Unlocking always uses the
    /// parameters stored in the vault itself.
    kdf: KdfParams,
    unlocked: Option<Unlocked>,
}

/// Everything that exists only while the vault is open: the Vault Key, and
/// nothing else. Dropping this value is what locks the vault.
struct Unlocked {
    key: SecretKey,
}

impl Vault {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            kdf: crypto::PRODUCTION,
            unlocked: None,
        }
    }

    /// A vault whose new vaults use the cheap test parameters. Test-only, so it
    /// cannot reach a real build.
    #[cfg(test)]
    pub fn with_test_kdf(path: PathBuf) -> Self {
        Self {
            path,
            kdf: crypto::TEST,
            unlocked: None,
        }
    }

    /// The database file this vault reads and writes.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether a vault exists and whether it is currently open.
    pub fn status(&self) -> Result<VaultStatus, VaultError> {
        if self.unlocked.is_some() {
            return Ok(VaultStatus::Unlocked);
        }

        let connection = self.connection()?;
        match storage::load_meta(&connection)? {
            Some(meta) => {
                check_supported(&meta)?;
                Ok(VaultStatus::Locked)
            }
            None => Ok(VaultStatus::Uninitialized),
        }
    }

    /// Creates the vault and leaves it unlocked.
    pub fn create(&mut self, password: &str) -> Result<(), VaultError> {
        if password.is_empty() {
            return Err(VaultError::EmptyPassword);
        }
        if self.status()? != VaultStatus::Uninitialized {
            return Err(VaultError::AlreadyInitialized);
        }

        let vault_id = new_id()?;
        let salt = crypto::random_bytes(crypto::SALT_LEN)?;
        let kek = crypto::derive_kek(password, &salt, self.kdf)?;
        let vault_key = SecretKey::generate()?;
        let wrapped = crypto::wrap_vault_key(&kek, &vault_key, &vault_id)?;

        let now = storage::now();
        let meta = VaultMeta {
            id: vault_id.clone(),
            schema_version: SCHEMA_VERSION,
            crypto_version: CRYPTO_VERSION,
            kdf_algorithm: KDF_ARGON2ID.to_string(),
            kdf_salt: salt,
            kdf_memory_cost: self.kdf.memory_cost,
            kdf_time_cost: self.kdf.time_cost,
            kdf_parallelism: self.kdf.parallelism,
            wrapped_vault_key: wrapped.ciphertext,
            wrap_nonce: wrapped.nonce,
            created_at: now,
            updated_at: now,
        };

        let connection = self.connection()?;
        storage::insert_meta(&connection, &meta)?;

        // The master password was only ever borrowed, and the Key Encryption Key
        // is dropped here; from now on the Vault Key alone opens the objects.
        drop(kek);
        self.unlocked = Some(Unlocked { key: vault_key });
        Ok(())
    }

    /// Opens the vault with the master password.
    ///
    /// A wrong password, a damaged wrapped key, a damaged nonce and a failed
    /// authentication all produce the same generic error.
    pub fn unlock(&mut self, password: &str) -> Result<(), VaultError> {
        let connection = self.connection()?;
        let meta = storage::load_meta(&connection)?.ok_or(VaultError::NotInitialized)?;

        // Version problems are the user's to act on, so they are reported as
        // themselves instead of being folded into the generic unlock error.
        check_supported(&meta)?;

        let unlocked = open_with(&meta, password).map_err(|_| VaultError::Unlock)?;
        self.unlocked = Some(unlocked);
        Ok(())
    }

    /// Forgets the Vault Key. Locking an already locked vault does nothing.
    pub fn lock(&mut self) {
        // Dropping the key is what zeroizes it.
        self.unlocked = None;
    }

    /// Encrypts a new object and returns its id.
    pub fn put_object(&self, kind: &str, payload: &str) -> Result<String, VaultError> {
        let id = new_id()?;
        self.put_object_with_id(&id, kind, payload)?;
        Ok(id)
    }

    /// Encrypts a new object under an id the caller already chose.
    ///
    /// `put_object` is the usual entry point. This exists for callers whose
    /// payload contains the id as well, so that the two can never drift apart:
    /// the id is part of the object's Additional Authenticated Data.
    pub fn put_object_with_id(
        &self,
        id: &str,
        kind: &str,
        payload: &str,
    ) -> Result<(), VaultError> {
        let unlocked = self.unlocked()?;
        let plaintext = Envelope::new(kind, payload).to_bytes()?;
        let encrypted = crypto::encrypt_object(&unlocked.key, id, kind, &plaintext)?;

        let now = storage::now();
        let object = StoredObject {
            id: id.to_string(),
            kind: kind.to_string(),
            encrypted_payload: encrypted.ciphertext,
            nonce: encrypted.nonce,
            crypto_version: CRYPTO_VERSION,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        let connection = self.connection()?;
        storage::insert_object(&connection, &object)
    }

    /// Reads and decrypts one active object.
    pub fn get_object(&self, id: &str) -> Result<VaultObject, VaultError> {
        let unlocked = self.unlocked()?;
        let connection = self.connection()?;
        let object = storage::load_object(&connection, id)?.ok_or(VaultError::NotFound)?;
        if object.deleted_at.is_some() {
            return Err(VaultError::NotFound);
        }
        decrypt_object(&unlocked.key, &object)
    }

    /// Re-encrypts an object's payload with a fresh nonce.
    pub fn update_object(&self, id: &str, payload: &str) -> Result<(), VaultError> {
        let unlocked = self.unlocked()?;
        let connection = self.connection()?;
        let object = storage::load_object(&connection, id)?.ok_or(VaultError::NotFound)?;
        if object.deleted_at.is_some() {
            return Err(VaultError::NotFound);
        }

        let plaintext = Envelope::new(&object.kind, payload).to_bytes()?;
        let encrypted = crypto::encrypt_object(&unlocked.key, id, &object.kind, &plaintext)?;

        storage::update_object(
            &connection,
            &StoredObject {
                encrypted_payload: encrypted.ciphertext,
                nonce: encrypted.nonce,
                updated_at: storage::now(),
                ..object
            },
        )
    }

    /// Soft-deletes an object: the row stays, `deleted_at` is set.
    pub fn delete_object(&self, id: &str) -> Result<(), VaultError> {
        self.unlocked()?;
        let connection = self.connection()?;
        let object = storage::load_object(&connection, id)?.ok_or(VaultError::NotFound)?;
        if object.deleted_at.is_some() {
            return Err(VaultError::NotFound);
        }
        storage::mark_deleted(&connection, id, storage::now())
    }

    /// The active objects, optionally limited to one `kind`.
    pub fn list_objects(&self, kind: Option<&str>) -> Result<Vec<VaultObject>, VaultError> {
        let unlocked = self.unlocked()?;
        let connection = self.connection()?;
        storage::list_objects(&connection, kind)?
            .iter()
            .map(|object| decrypt_object(&unlocked.key, object))
            .collect()
    }

    fn unlocked(&self) -> Result<&Unlocked, VaultError> {
        self.unlocked.as_ref().ok_or(VaultError::Locked)
    }

    fn connection(&self) -> Result<rusqlite::Connection, VaultError> {
        storage::open(&self.path)
    }
}

/// A random version 4 UUID, for a new object.
///
/// The bytes come from the same random source as the keys rather than
/// `Uuid::new_v4`, so a failing source is reported as an error here too instead
/// of panicking.
pub fn new_id() -> Result<String, VaultError> {
    let mut bytes = crypto::random_bytes(16)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    Uuid::from_slice(&bytes)
        .map(|uuid| uuid.to_string())
        .map_err(|_| VaultError::Random)
}

/// Derives the Key Encryption Key and unwraps the Vault Key. Every failure here
/// is an unlock failure, so the caller maps it to the generic message.
fn open_with(meta: &VaultMeta, password: &str) -> Result<Unlocked, VaultError> {
    let params = KdfParams {
        memory_cost: meta.kdf_memory_cost,
        time_cost: meta.kdf_time_cost,
        parallelism: meta.kdf_parallelism,
    };
    let kek = crypto::derive_kek(password, &meta.kdf_salt, params)?;
    let key = crypto::unwrap_vault_key(&kek, &meta.wrapped_vault_key, &meta.wrap_nonce, &meta.id)?;
    Ok(Unlocked { key })
}

/// Rejects a vault written by a newer build, or with a different KDF.
fn check_supported(meta: &VaultMeta) -> Result<(), VaultError> {
    if meta.schema_version > SCHEMA_VERSION {
        return Err(VaultError::UnsupportedVersion {
            found: meta.schema_version,
            supported: SCHEMA_VERSION,
        });
    }
    if meta.crypto_version > CRYPTO_VERSION {
        return Err(VaultError::UnsupportedVersion {
            found: meta.crypto_version,
            supported: CRYPTO_VERSION,
        });
    }
    if meta.kdf_algorithm != KDF_ARGON2ID {
        return Err(VaultError::Corrupt("unknown key derivation algorithm"));
    }
    Ok(())
}

fn decrypt_object(key: &SecretKey, object: &StoredObject) -> Result<VaultObject, VaultError> {
    if object.crypto_version > CRYPTO_VERSION {
        return Err(VaultError::UnsupportedVersion {
            found: object.crypto_version,
            supported: CRYPTO_VERSION,
        });
    }

    let plaintext = crypto::decrypt_object(
        key,
        &object.id,
        &object.kind,
        &object.encrypted_payload,
        &object.nonce,
    )
    .map_err(|_| VaultError::Corrupt("object payload"))?;

    let envelope = Envelope::from_bytes(&plaintext).map_err(|_| VaultError::Corrupt("envelope"))?;

    Ok(VaultObject {
        id: object.id.clone(),
        kind: object.kind.clone(),
        schema_version: envelope.schema_version,
        payload: envelope.payload,
        created_at: object.created_at,
        updated_at: object.updated_at,
    })
}

/// The vault as the Tauri commands see it: one instance behind a mutex.
pub struct VaultState {
    vault: Mutex<Vault>,
}

impl VaultState {
    pub fn new(vault: Vault) -> Self {
        Self {
            vault: Mutex::new(vault),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vault> {
        // A poisoned lock only means an earlier command panicked; the vault is
        // still usable, so recover instead of failing every later command.
        self.vault
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Runs one blocking vault operation on the blocking pool.
///
/// Both Argon2id and SQLite block the calling thread, so no command does its
/// work on the UI thread. The error type is generic so the inventory commands,
/// which report their own error type, reach the vault through the same path.
pub(crate) async fn blocking<T, E, F>(app: AppHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    E: fmt::Display + Send + 'static,
    F: FnOnce(&mut Vault) -> Result<T, E> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<VaultState>();
        let mut vault = state.lock();
        work(&mut vault)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Reports whether a vault exists and whether it is open.
#[tauri::command]
pub async fn vault_status(app: AppHandle) -> Result<VaultStatus, String> {
    blocking(app, |vault| vault.status()).await
}

/// Creates a new vault and leaves it unlocked.
#[tauri::command]
pub async fn create_vault(app: AppHandle, password: String) -> Result<VaultStatus, String> {
    // The password is wiped as soon as this command is done with it.
    let password = Zeroizing::new(password);
    blocking(app, move |vault| {
        vault.create(&password)?;
        vault.status()
    })
    .await
}

/// Unlocks the vault with the master password.
#[tauri::command]
pub async fn unlock_vault(app: AppHandle, password: String) -> Result<VaultStatus, String> {
    let password = Zeroizing::new(password);
    blocking(app, move |vault| {
        vault.unlock(&password)?;
        vault.status()
    })
    .await
}

/// Forgets the Vault Key.
#[tauri::command]
pub async fn lock_vault(app: AppHandle) -> Result<VaultStatus, String> {
    blocking(app, |vault| {
        vault.lock();
        vault.status()
    })
    .await
}
