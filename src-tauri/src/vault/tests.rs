//! Tests for the vault.
//!
//! Every test gets its own temporary directory and deletes it afterwards, so no
//! test ever touches a real vault or the app data directory. New vaults use the
//! cheap Argon2id parameters from `crypto::TEST`; nothing else changes, so these
//! tests exercise the same code paths the app does.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use uuid::Uuid;

use super::model::{CRYPTO_VERSION, ENVELOPE_VERSION};
use super::storage;
use super::{DB_FILE, Vault, VaultError, VaultStatus};

/// The master password every test creates its vault with.
const PASSWORD: &str = "a-strong-master-password";

/// A temporary directory that removes itself, and the database path inside it.
struct TempVault {
    directory: PathBuf,
}

impl TempVault {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("termust-vault-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("the temporary directory was created");
        Self { directory }
    }

    fn database(&self) -> PathBuf {
        self.directory.join(DB_FILE)
    }

    /// A fresh handle on the same database, as if the app had been restarted.
    fn vault(&self) -> Vault {
        Vault::with_test_kdf(self.database())
    }
}

impl Drop for TempVault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

/// A vault that has been created and is left unlocked.
fn created_vault() -> (TempVault, Vault) {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).expect("the vault was created");
    assert_eq!(vault.status().unwrap(), VaultStatus::Unlocked);
    (temp, vault)
}

/// An object row as SQLite holds it, read without going through the vault.
struct RawObject {
    encrypted_payload: Vec<u8>,
    nonce: Vec<u8>,
    deleted_at: Option<i64>,
}

fn read_raw_object(path: &Path, id: &str) -> RawObject {
    Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT encrypted_payload, nonce, deleted_at FROM vault_objects WHERE id = ?1",
            params![id],
            |row| {
                Ok(RawObject {
                    encrypted_payload: row.get(0)?,
                    nonce: row.get(1)?,
                    deleted_at: row.get(2)?,
                })
            },
        )
        .expect("the object row exists")
}

fn corrupt_wrapped_key(path: &Path) {
    let connection = Connection::open(path).unwrap();
    let mut wrapped: Vec<u8> = connection
        .query_row("SELECT wrapped_vault_key FROM vault_meta", [], |row| {
            row.get(0)
        })
        .unwrap();
    wrapped[0] ^= 0x01;
    connection
        .execute(
            "UPDATE vault_meta SET wrapped_vault_key = ?1",
            params![wrapped],
        )
        .unwrap();
}

fn corrupt_object_payload(path: &Path, id: &str) {
    let connection = Connection::open(path).unwrap();
    let mut payload: Vec<u8> = connection
        .query_row(
            "SELECT encrypted_payload FROM vault_objects WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    payload[0] ^= 0x01;
    connection
        .execute(
            "UPDATE vault_objects SET encrypted_payload = ?1 WHERE id = ?2",
            params![payload, id],
        )
        .unwrap();
}

fn set_meta_crypto_version(path: &Path, version: i64) {
    Connection::open(path)
        .unwrap()
        .execute(
            "UPDATE vault_meta SET crypto_version = ?1",
            params![version],
        )
        .unwrap();
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// --- Lifecycle ------------------------------------------------------------

#[test]
fn unlocking_uses_the_parameters_stored_in_the_vault() {
    // The other tests create their vaults with the cheap test parameters. This
    // one creates a vault with the real production parameters, which also proves
    // those settings are valid at runtime, and then unlocks it with a handle
    // whose own parameters are the cheap ones. It can only succeed if unlocking
    // reads the stored parameters instead of the defaults.
    let temp = TempVault::new();
    let mut vault = Vault::new(temp.database());
    vault.create(PASSWORD).unwrap();
    let id = vault.put_object("note", "payload").unwrap();
    drop(vault);

    let mut reopened = Vault::with_test_kdf(temp.database());
    reopened.unlock(PASSWORD).unwrap();

    assert_eq!(reopened.get_object(&id).unwrap().payload, "payload");
}

#[test]
fn a_fresh_database_starts_uninitialized() {
    let temp = TempVault::new();
    assert_eq!(temp.vault().status().unwrap(), VaultStatus::Uninitialized);
}

#[test]
fn create_leaves_the_vault_unlocked_and_lock_closes_it() {
    let (_temp, mut vault) = created_vault();

    vault.lock();

    assert_eq!(vault.status().unwrap(), VaultStatus::Locked);
}

#[test]
fn locking_an_already_locked_vault_is_not_an_error() {
    let (_temp, mut vault) = created_vault();
    vault.lock();

    vault.lock();

    assert_eq!(vault.status().unwrap(), VaultStatus::Locked);
}

#[test]
fn a_second_vault_cannot_be_created() {
    let (_temp, mut vault) = created_vault();

    assert!(matches!(
        vault.create(PASSWORD),
        Err(VaultError::AlreadyInitialized)
    ));
}

#[test]
fn an_empty_password_does_not_create_a_vault() {
    let temp = TempVault::new();
    let mut vault = temp.vault();

    assert!(matches!(vault.create(""), Err(VaultError::EmptyPassword)));
    assert_eq!(vault.status().unwrap(), VaultStatus::Uninitialized);
}

#[test]
fn the_right_password_unlocks_the_vault() {
    let (_temp, mut vault) = created_vault();
    vault.lock();

    vault.unlock(PASSWORD).expect("the password was right");

    assert_eq!(vault.status().unwrap(), VaultStatus::Unlocked);
}

#[test]
fn a_wrong_password_reports_only_the_generic_error() {
    let (_temp, mut vault) = created_vault();
    vault.lock();

    let error = vault.unlock("not the password").unwrap_err();

    assert_eq!(
        error.to_string(),
        "Invalid master password or corrupted vault"
    );
    assert_eq!(vault.status().unwrap(), VaultStatus::Locked);
}

#[test]
fn reopening_the_database_reports_locked() {
    let (temp, vault) = created_vault();
    drop(vault);

    let reopened = temp.vault();

    assert_eq!(reopened.status().unwrap(), VaultStatus::Locked);
}

// --- Objects --------------------------------------------------------------

#[test]
fn an_object_round_trips_through_a_reopened_database() {
    let (temp, vault) = created_vault();
    let id = vault.put_object("note", r#"{"marker":"hello"}"#).unwrap();
    drop(vault);

    let mut reopened = temp.vault();
    reopened.unlock(PASSWORD).unwrap();
    let object = reopened.get_object(&id).unwrap();

    assert_eq!(object.kind, "note");
    assert_eq!(object.schema_version, ENVELOPE_VERSION);
    assert_eq!(object.payload, r#"{"marker":"hello"}"#);
}

#[test]
fn updating_an_object_uses_a_fresh_nonce() {
    let (_temp, vault) = created_vault();
    let id = vault.put_object("note", "first").unwrap();
    let before = read_raw_object(vault.path(), &id);

    vault.update_object(&id, "second").unwrap();
    let after = read_raw_object(vault.path(), &id);

    assert_ne!(before.nonce, after.nonce, "the nonce was reused on update");
    assert_ne!(before.encrypted_payload, after.encrypted_payload);
    assert_eq!(vault.get_object(&id).unwrap().payload, "second");
}

#[test]
fn deleting_an_object_marks_it_instead_of_removing_the_row() {
    let (_temp, vault) = created_vault();
    let id = vault.put_object("note", "payload").unwrap();

    vault.delete_object(&id).unwrap();

    let raw = read_raw_object(vault.path(), &id);
    assert!(
        raw.deleted_at.is_some(),
        "the row was removed instead of marked"
    );
    assert!(matches!(vault.get_object(&id), Err(VaultError::NotFound)));
}

#[test]
fn listing_returns_active_objects_only() {
    let (_temp, vault) = created_vault();
    let kept = vault.put_object("note", "keep").unwrap();
    let removed = vault.put_object("note", "remove").unwrap();
    let host = vault.put_object("host", "a different kind").unwrap();

    vault.delete_object(&removed).unwrap();

    let all: Vec<String> = vault
        .list_objects(None)
        .unwrap()
        .into_iter()
        .map(|object| object.id)
        .collect();
    assert!(all.contains(&kept), "the active object is missing");
    assert!(all.contains(&host), "the other kind is missing");
    assert!(!all.contains(&removed), "a deleted object was listed");

    let notes = vault.list_objects(Some("note")).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, kept);
}

#[test]
fn object_operations_are_refused_while_locked() {
    let (_temp, mut vault) = created_vault();
    let id = vault.put_object("note", "payload").unwrap();
    vault.lock();

    assert!(matches!(
        vault.put_object("note", "x"),
        Err(VaultError::Locked)
    ));
    assert!(matches!(vault.get_object(&id), Err(VaultError::Locked)));
    assert!(matches!(
        vault.update_object(&id, "x"),
        Err(VaultError::Locked)
    ));
    assert!(matches!(vault.delete_object(&id), Err(VaultError::Locked)));
    assert!(matches!(vault.list_objects(None), Err(VaultError::Locked)));
}

// --- Storage --------------------------------------------------------------

#[test]
fn migrating_an_empty_database_creates_the_schema_and_runs_again_safely() {
    let temp = TempVault::new();
    let connection = storage::open(&temp.database()).unwrap();

    // Running the migration again must change nothing.
    storage::migrate(&connection).unwrap();
    storage::migrate(&connection).unwrap();

    let tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('vault_meta', 'vault_objects')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 2);
    assert!(storage::load_meta(&connection).unwrap().is_none());
}

#[test]
fn the_database_never_contains_the_password_or_a_payload_in_the_clear() {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).unwrap();

    let payload_marker = "termust-plaintext-marker-9f3c";
    let id = vault.put_object("note", payload_marker).unwrap();

    // The row really is there and really is encrypted...
    let raw = read_raw_object(vault.path(), &id);
    assert!(!contains(&raw.encrypted_payload, payload_marker.as_bytes()));

    // ...and nothing readable is left in the file itself.
    let bytes = fs::read(temp.database()).unwrap();
    assert!(
        !contains(&bytes, payload_marker.as_bytes()),
        "the payload was stored in the clear"
    );
    assert!(
        !contains(&bytes, PASSWORD.as_bytes()),
        "the master password was stored"
    );
}

// --- Corruption -----------------------------------------------------------

#[test]
fn a_corrupted_wrapped_key_gives_the_generic_unlock_error() {
    let (_temp, mut vault) = created_vault();
    vault.lock();
    corrupt_wrapped_key(vault.path());

    let error = vault.unlock(PASSWORD).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Invalid master password or corrupted vault"
    );
}

#[test]
fn a_corrupted_object_is_reported_without_its_plaintext() {
    let (_temp, vault) = created_vault();
    let marker = "termust-object-plaintext-1a2b";
    let id = vault.put_object("note", marker).unwrap();
    corrupt_object_payload(vault.path(), &id);

    let error = vault.get_object(&id).unwrap_err();
    let message = error.to_string();

    assert!(
        matches!(error, VaultError::Corrupt(_)),
        "expected a typed corruption error, got {error:?}"
    );
    assert!(!message.contains(marker), "the error leaked the payload");
}

#[test]
fn an_unsupported_crypto_version_is_reported_clearly() {
    let (_temp, mut vault) = created_vault();
    vault.lock();
    set_meta_crypto_version(vault.path(), CRYPTO_VERSION + 1);

    let status_error = vault.status().unwrap_err();
    assert!(matches!(
        status_error,
        VaultError::UnsupportedVersion { found, supported }
            if found == CRYPTO_VERSION + 1 && supported == CRYPTO_VERSION
    ));

    // The version problem is told apart from a wrong password.
    let unlock_error = vault.unlock(PASSWORD).unwrap_err();
    assert!(matches!(
        unlock_error,
        VaultError::UnsupportedVersion { .. }
    ));
    assert_ne!(
        unlock_error.to_string(),
        "Invalid master password or corrupted vault"
    );
}
