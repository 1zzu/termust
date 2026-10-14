//! Encrypted, portable backups of the local vault, and restoring one.
//!
//! A backup is a copy of the vault's domain data — groups, hosts, credentials,
//! known hosts, snippets and tombstones — written to one file, encrypted under a
//! password the user chooses for that file. It is deliberately *not* a copy of
//! the database: it holds no vault metadata, no wrapped Vault Key and no
//! ciphertext from the source vault, so restoring it into a vault with a
//! different master password re-encrypts every record with that vault's own key.
//!
//! Three commands, and one rule they all follow: read and check everything before
//! writing anything. Export reads the whole vault and validates the snapshot it
//! just built before an atomic file write; inspect reads and validates and writes
//! nothing at all; restore validates the entire file before one SQLite
//! transaction replaces the table.
//!
//! The module is split by concern:
//!
//! - `model.rs`: the file format's types and the IPC contract.
//! - `format.rs`: turning a payload into backup bytes and back.
//! - `service.rs`: the vault reads, the file handling and the restore.
//! - this file: the Tauri commands and the error mapping.

mod format;
pub(crate) mod model;
mod service;

#[cfg(test)]
mod tests;

use std::fmt;

use tauri::{AppHandle, Runtime, State};

use crate::ssh::SshState;
use crate::vault::{VaultError, blocking};

use model::{
    BackupSummary, ExportRequest, ExportSummary, InspectRequest, RestoreRequest, RestoreSummary,
};

/// The service calls a test drives directly, so the live round trip can export
/// and restore without an IPC round trip. Test-only: the app only ever reaches
/// them through the three commands below, which add the session check.
#[cfg(test)]
pub(crate) use service::{export, inspect, restore};

/// Everything a backup operation can fail with.
///
/// The variants exist so this code can tell its own failures apart, but the
/// `Display` text is what the frontend shows: it never contains the backup
/// password, a decrypted record, an object's payload, a file's contents or an
/// untrusted string from a file the user opened. A wrong password and a damaged
/// file share one message on purpose, so a failed open is not a password oracle.
#[derive(Debug)]
pub enum BackupError {
    /// The path is empty, has no file name, or names a directory.
    InvalidPath,
    /// The file could not be read.
    CannotRead,
    /// The file could not be written.
    CannotWrite,
    /// The user picked a directory where a file was needed.
    NotAFile,
    /// The file is larger than this build will handle.
    TooLarge,
    /// The destination already holds a file and the caller did not ask to
    /// replace it.
    DestinationExists,
    /// The backup password is shorter than this build accepts.
    PasswordTooShort,
    /// A key-derivation parameter set this build will not run.
    Kdf,
    /// The file is not a backup at all.
    NotABackup,
    /// A format version this build does not read.
    UnsupportedVersion { found: u8, supported: u8 },
    /// A key-derivation algorithm this build does not read.
    UnsupportedKdf,
    /// A cipher this build does not read.
    UnsupportedCipher,
    /// The password was wrong, or the file is damaged. One message for both.
    CannotOpen,
    /// A record of a kind this build does not know.
    UnknownKind,
    /// Two records with the same id.
    DuplicateId,
    /// A record that does not parse, disagrees with its own id, or breaks one of
    /// the field rules the commands apply.
    InvalidRecord,
    /// The backup's own metadata is missing, negative or oversized.
    InvalidPayload,
    /// Two live groups whose names differ only in case.
    DuplicateGroupName,
    /// The file changed between the preview and the restore.
    FileChanged,
    /// A payload that could not be serialised. Guards an impossible case.
    InvalidSnapshot,
    /// A record encrypted with a crypto version this build does not write.
    UnsupportedCryptoVersion { found: i64, supported: i64 },
    /// A record written with a payload version this build does not know.
    UnsupportedPayloadVersion { found: u64, supported: u8 },
    /// A saved host points at a group or credential the backup does not hold.
    InvalidRelation,
    /// Two live trusted keys for the same host identity.
    DuplicateKnownHost,
    /// More records than this build will restore in one go.
    TooManyObjects,
    /// A restore mode other than replacing the vault.
    UnsupportedMode,
    /// The typed confirmation phrase was not the one this build requires.
    ConfirmationRequired,
    /// A session is open or connecting, so its records must not change.
    SessionActive,
    /// Anything the vault itself reported.
    Vault(VaultError),
}

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => formatter.write_str("Choose a file for the backup."),
            Self::CannotRead => formatter.write_str("That file could not be read."),
            Self::CannotWrite => formatter.write_str("The backup could not be written."),
            Self::NotAFile => formatter.write_str("Choose a backup file, not a folder."),
            Self::TooLarge => write!(
                formatter,
                "That file is larger than the {} byte limit for a backup.",
                model::MAX_BACKUP_BYTES
            ),
            Self::DestinationExists => formatter.write_str(
                "A file with that name is already there. Choose another name, or confirm replacing it.",
            ),
            Self::PasswordTooShort => write!(
                formatter,
                "The backup password must be at least {} characters.",
                model::MIN_PASSWORD_LEN
            ),
            Self::Kdf => formatter.write_str(
                "This backup's key derivation is not one this build is willing to run.",
            ),
            Self::NotABackup => formatter.write_str("That file is not a Termust backup."),
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "This backup uses format version {found}, but this build supports version {supported}."
            ),
            Self::UnsupportedKdf => {
                formatter.write_str("This backup uses a key derivation this build does not support.")
            }
            Self::UnsupportedCipher => {
                formatter.write_str("This backup uses a cipher this build does not support.")
            }
            Self::CannotOpen => formatter.write_str(
                "The backup could not be opened. The password is wrong or the file is damaged.",
            ),
            Self::UnknownKind => formatter
                .write_str("The backup holds a kind of record this build does not know how to read."),
            Self::DuplicateId => {
                formatter.write_str("The backup holds two records with the same id.")
            }
            Self::InvalidRecord => {
                formatter.write_str("The backup holds a record that is not valid.")
            }
            Self::InvalidPayload => {
                formatter.write_str("The backup's own details are not valid.")
            }
            Self::DuplicateGroupName => {
                formatter.write_str("The backup holds two groups with the same name.")
            }
            Self::FileChanged => formatter.write_str(
                "That file changed since it was checked. Choose it again and read it before restoring.",
            ),
            Self::InvalidSnapshot => formatter.write_str("The backup could not be prepared."),
            Self::UnsupportedCryptoVersion { found, supported } => write!(
                formatter,
                "A record in this backup uses crypto version {found}, but this build supports version {supported}."
            ),
            Self::UnsupportedPayloadVersion { found, supported } => write!(
                formatter,
                "A record in this backup uses payload version {found}, but this build supports version {supported}."
            ),
            Self::InvalidRelation => formatter.write_str(
                "The backup points a saved host at a group or a credential it does not hold.",
            ),
            Self::DuplicateKnownHost => {
                formatter.write_str("The backup holds two trusted keys for the same host.")
            }
            Self::TooManyObjects => write!(
                formatter,
                "The backup holds more than {} records.",
                model::MAX_OBJECTS
            ),
            Self::UnsupportedMode => {
                formatter.write_str("Only replacing this vault with a backup is supported.")
            }
            Self::ConfirmationRequired => {
                formatter.write_str("Type RESTORE to confirm that this vault will be replaced.")
            }
            Self::SessionActive => {
                formatter.write_str("Disconnect the SSH session before restoring a backup.")
            }
            // The vault's own messages are safe to show, except for a storage
            // failure, whose text comes from SQLite.
            Self::Vault(VaultError::Storage(_)) => {
                formatter.write_str("The vault could not be read or written on this device.")
            }
            Self::Vault(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for BackupError {}

// --- export ---------------------------------------------------------------

/// Writes the whole vault to one encrypted file the user chose.
///
/// The request carries the destination the native save dialog returned and the
/// backup password, twice-confirmed in the frontend. The password is wiped when
/// the request is dropped, which is when the command returns.
#[tauri::command]
pub async fn export_encrypted_backup<R: Runtime>(
    app: AppHandle<R>,
    request: ExportRequest,
) -> Result<ExportSummary, String> {
    blocking(app, move |vault| service::export(vault, &request)).await
}

// --- inspect --------------------------------------------------------------

/// Reads a backup file and reports how much data it holds.
///
/// This is a read: nothing in the vault and nothing on disk changes. The answer
/// is counts, never a secret and never a whole payload.
#[tauri::command]
pub async fn inspect_encrypted_backup<R: Runtime>(
    app: AppHandle<R>,
    request: InspectRequest,
) -> Result<BackupSummary, String> {
    blocking(app, move |vault| service::inspect(vault, &request)).await
}

// --- restore --------------------------------------------------------------

/// Replaces every record in this vault with the contents of a backup file.
///
/// The request has to name the replace mode and the typed phrase, and the
/// restore takes the session guard here — before the vault lock is taken and
/// before the file is opened — so a refused restore has touched nothing at all
/// and a session can neither stand in its way after the check nor start while
/// it runs. The guard then moves into the blocking work and is released by
/// `Drop` when that work really ends, on every path out: success, failure and
/// panic alike. Everything else happens inside one transaction in the vault.
#[tauri::command]
pub async fn restore_encrypted_backup<R: Runtime>(
    app: AppHandle<R>,
    sessions: State<'_, SshState>,
    request: RestoreRequest,
) -> Result<RestoreSummary, String> {
    service::check_restore_request(&request).map_err(|error| error.to_string())?;
    let guard = sessions
        .begin_maintenance()
        .ok_or_else(|| BackupError::SessionActive.to_string())?;

    // The guard travels into the blocking closure rather than staying in this
    // future. Dropping this future — a cancelled command — drops the blocking
    // task's join handle, but a task that has already started runs to the end;
    // a guard owned here would be released while the restore was still replacing
    // the vault, and a new session could start in the middle of it. Owned by the
    // work, the guard lives exactly as long as the work does. An early return
    // above already dropped it, which is the refused path.
    blocking(app, move |vault| {
        let _guard = guard;
        service::restore(vault, &request)
    })
    .await
}
