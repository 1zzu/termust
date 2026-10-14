//! The SQLite side of the vault.
//!
//! This module only talks to `rusqlite`. It knows the schema, never the keys:
//! every value it reads or writes is either a plain metadata column or opaque
//! ciphertext. The caller runs these blocking calls off the UI thread.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::VaultError;
use super::model::{SCHEMA_VERSION, StoredObject, VaultMeta};

/// The database file name inside the app data directory.
pub const DB_FILE: &str = "termust.db";

/// The first schema version. `IF NOT EXISTS` makes it safe to run again.
const CREATE_V1: &str = "
CREATE TABLE IF NOT EXISTS vault_meta (
    id                  TEXT    PRIMARY KEY,
    schema_version      INTEGER NOT NULL,
    crypto_version      INTEGER NOT NULL,
    kdf_algorithm       TEXT    NOT NULL,
    kdf_salt            BLOB    NOT NULL,
    kdf_memory_cost     INTEGER NOT NULL,
    kdf_time_cost       INTEGER NOT NULL,
    kdf_parallelism     INTEGER NOT NULL,
    wrapped_vault_key   BLOB    NOT NULL,
    wrap_nonce          BLOB    NOT NULL,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS vault_objects (
    id                  TEXT    PRIMARY KEY,
    kind                TEXT    NOT NULL,
    encrypted_payload   BLOB    NOT NULL,
    nonce               BLOB    NOT NULL,
    crypto_version      INTEGER NOT NULL,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    deleted_at          INTEGER
);
";

/// Opens the database, creating its directory and schema when needed.
pub fn open(path: &Path) -> Result<Connection, VaultError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| VaultError::Storage(error.to_string()))?;
    }
    let connection = Connection::open(path).map_err(storage_error)?;
    migrate(&connection)?;
    Ok(connection)
}

/// Brings the schema up to `SCHEMA_VERSION`, or reports a version this build
/// does not understand. Running it on an up-to-date database does nothing.
pub fn migrate(connection: &Connection) -> Result<(), VaultError> {
    let current: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(storage_error)?;

    if current > SCHEMA_VERSION {
        return Err(VaultError::UnsupportedVersion {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }

    if current < 1 {
        connection.execute_batch(CREATE_V1).map_err(storage_error)?;
        connection
            .pragma_update(None, "user_version", 1)
            .map_err(storage_error)?;
    }

    Ok(())
}

/// Unix time in seconds, used for the human-facing timestamp columns only.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

pub fn load_meta(connection: &Connection) -> Result<Option<VaultMeta>, VaultError> {
    connection
        .query_row(
            "SELECT id, schema_version, crypto_version, kdf_algorithm, kdf_salt,
                    kdf_memory_cost, kdf_time_cost, kdf_parallelism,
                    wrapped_vault_key, wrap_nonce, created_at, updated_at
             FROM vault_meta
             LIMIT 1",
            [],
            |row| {
                Ok(VaultMeta {
                    id: row.get("id")?,
                    schema_version: row.get("schema_version")?,
                    crypto_version: row.get("crypto_version")?,
                    kdf_algorithm: row.get("kdf_algorithm")?,
                    kdf_salt: row.get("kdf_salt")?,
                    kdf_memory_cost: row.get("kdf_memory_cost")?,
                    kdf_time_cost: row.get("kdf_time_cost")?,
                    kdf_parallelism: row.get("kdf_parallelism")?,
                    wrapped_vault_key: row.get("wrapped_vault_key")?,
                    wrap_nonce: row.get("wrap_nonce")?,
                    created_at: row.get("created_at")?,
                    updated_at: row.get("updated_at")?,
                })
            },
        )
        .optional()
        .map_err(storage_error)
}

pub fn insert_meta(connection: &Connection, meta: &VaultMeta) -> Result<(), VaultError> {
    connection
        .execute(
            "INSERT INTO vault_meta (
                id, schema_version, crypto_version, kdf_algorithm, kdf_salt,
                kdf_memory_cost, kdf_time_cost, kdf_parallelism,
                wrapped_vault_key, wrap_nonce, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                meta.id,
                meta.schema_version,
                meta.crypto_version,
                meta.kdf_algorithm,
                meta.kdf_salt,
                meta.kdf_memory_cost,
                meta.kdf_time_cost,
                meta.kdf_parallelism,
                meta.wrapped_vault_key,
                meta.wrap_nonce,
                meta.created_at,
                meta.updated_at,
            ],
        )
        .map_err(storage_error)?;
    Ok(())
}

pub fn insert_object(connection: &Connection, object: &StoredObject) -> Result<(), VaultError> {
    connection
        .execute(
            "INSERT INTO vault_objects (
                id, kind, encrypted_payload, nonce, crypto_version,
                created_at, updated_at, deleted_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                object.id,
                object.kind,
                object.encrypted_payload,
                object.nonce,
                object.crypto_version,
                object.created_at,
                object.updated_at,
                object.deleted_at,
            ],
        )
        .map_err(storage_error)?;
    Ok(())
}

/// Rewrites an object's ciphertext, nonce and `updated_at`.
pub fn update_object(connection: &Connection, object: &StoredObject) -> Result<(), VaultError> {
    connection
        .execute(
            "UPDATE vault_objects
             SET encrypted_payload = ?2, nonce = ?3, crypto_version = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                object.id,
                object.encrypted_payload,
                object.nonce,
                object.crypto_version,
                object.updated_at,
            ],
        )
        .map_err(storage_error)?;
    Ok(())
}

/// Marks an object deleted without removing the row.
pub fn mark_deleted(connection: &Connection, id: &str, deleted_at: i64) -> Result<(), VaultError> {
    connection
        .execute(
            "UPDATE vault_objects SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, deleted_at],
        )
        .map_err(storage_error)?;
    Ok(())
}

pub fn load_object(connection: &Connection, id: &str) -> Result<Option<StoredObject>, VaultError> {
    connection
        .query_row(
            "SELECT id, kind, encrypted_payload, nonce, crypto_version,
                    created_at, updated_at, deleted_at
             FROM vault_objects
             WHERE id = ?1",
            params![id],
            object_from_row,
        )
        .optional()
        .map_err(storage_error)
}

/// Active objects, optionally limited to one `kind`.
pub fn list_objects(
    connection: &Connection,
    kind: Option<&str>,
) -> Result<Vec<StoredObject>, VaultError> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, encrypted_payload, nonce, crypto_version,
                    created_at, updated_at, deleted_at
             FROM vault_objects
             WHERE deleted_at IS NULL AND (?1 IS NULL OR kind = ?1)
             ORDER BY updated_at DESC, id",
        )
        .map_err(storage_error)?;

    let rows = statement
        .query_map(params![kind], object_from_row)
        .map_err(storage_error)?;

    let mut objects = Vec::new();
    for row in rows {
        objects.push(row.map_err(storage_error)?);
    }
    Ok(objects)
}

/// Every object row, tombstones included.
///
/// A portable backup is a copy of the whole table, so this is the one read that
/// does not filter on `deleted_at`.
pub fn list_all_objects(connection: &Connection) -> Result<Vec<StoredObject>, VaultError> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, encrypted_payload, nonce, crypto_version,
                    created_at, updated_at, deleted_at
             FROM vault_objects
             ORDER BY id",
        )
        .map_err(storage_error)?;

    let rows = statement
        .query_map([], object_from_row)
        .map_err(storage_error)?;

    let mut objects = Vec::new();
    for row in rows {
        objects.push(row.map_err(storage_error)?);
    }
    Ok(objects)
}

/// Replaces every row of `vault_objects` with the given snapshot, inside one
/// transaction.
///
/// The delete and every insert are one unit of work: either the whole snapshot
/// is committed or the table is exactly as it was. `vault_meta` is never
/// touched, so the vault's own identity, Vault Key and master password survive a
/// restore unchanged.
pub fn replace_objects(
    connection: &mut Connection,
    objects: &[StoredObject],
) -> Result<(), VaultError> {
    let transaction = connection.transaction().map_err(storage_error)?;

    transaction
        .execute("DELETE FROM vault_objects", [])
        .map_err(storage_error)?;

    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO vault_objects (
                    id, kind, encrypted_payload, nonce, crypto_version,
                    created_at, updated_at, deleted_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .map_err(storage_error)?;

        for object in objects {
            statement
                .execute(params![
                    object.id,
                    object.kind,
                    object.encrypted_payload,
                    object.nonce,
                    object.crypto_version,
                    object.created_at,
                    object.updated_at,
                    object.deleted_at,
                ])
                .map_err(storage_error)?;
        }
    }

    // The commit is the only point where the old rows disappear for good.
    transaction.commit().map_err(storage_error)
}

fn object_from_row(row: &Row<'_>) -> rusqlite::Result<StoredObject> {
    Ok(StoredObject {
        id: row.get("id")?,
        kind: row.get("kind")?,
        encrypted_payload: row.get("encrypted_payload")?,
        nonce: row.get("nonce")?,
        crypto_version: row.get("crypto_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        deleted_at: row.get("deleted_at")?,
    })
}

fn storage_error(error: rusqlite::Error) -> VaultError {
    VaultError::Storage(error.to_string())
}
