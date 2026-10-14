//! Exporting a vault, inspecting a backup, and restoring one.
//!
//! The three operations share one rule: **nothing is written before everything
//! has been read and checked**. An export reads the whole vault, decrypts every
//! record and checks the snapshot it just built before it creates a file; an
//! inspect reads and checks the file and writes nothing at all; a restore reads,
//! decrypts and validates the entire file — including re-encrypting every record
//! with this vault's key — before the first row of the current vault is touched.
//!
//! The order inside a restore matters and is not an accident: the request is
//! checked, then the file is read and bounded, then the envelope is checked, then
//! the payload is decrypted, then the whole snapshot is validated, and only then
//! does one SQLite transaction replace the table. A failure anywhere before that
//! transaction leaves the vault exactly as it was.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sha2::{Digest as _, Sha256};

use crate::inventory::{self, InventoryError, Record};
use crate::snippets::{self, KIND_SNIPPET, SnippetError};
use crate::vault::crypto::KdfParams;
// Only a real build writes the production cost: the tests use the cheap
// parameters so the suite stays fast.
#[cfg(not(test))]
use crate::vault::crypto::PRODUCTION;
use crate::vault::{CRYPTO_VERSION, SnapshotObject, Vault, VaultError, now};

use super::model::{
    BackupObject, BackupPayload, BackupSummary, ExportRequest, ExportSummary, InspectRequest,
    KindCount, MAX_BACKUP_BYTES, MAX_OBJECTS, MAX_SOURCE_APP_VERSION_LEN, MIN_PASSWORD_LEN,
    PAYLOAD_VERSION, REPLACE_CONFIRMATION, RESTORE_MODE_REPLACE, RestoreRequest, RestoreSummary,
};
use super::{BackupError, format};

/// The key-derivation parameters a new backup is written with.
///
/// The tests use the cheap ones so the suite stays fast; a real build always
/// writes the production cost, the same cost the vault is created with.
#[cfg(not(test))]
const KDF: KdfParams = PRODUCTION;
#[cfg(test)]
const KDF: KdfParams = crate::vault::crypto::TEST;

// --- export ---------------------------------------------------------------

/// Writes the whole vault to one encrypted file.
///
/// Reading is all this does to the vault, so it is safe while a session is open:
/// the snapshot is one consistent read and no object is changed, not even its
/// `updated_at`.
pub(crate) fn export(vault: &Vault, request: &ExportRequest) -> Result<ExportSummary, BackupError> {
    ensure_unlocked(vault)?;

    let password = request.backup_password.as_str();
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(BackupError::PasswordTooShort);
    }

    let destination = writable_path(&request.destination_path)?;
    if destination.exists() && !request.overwrite {
        return Err(BackupError::DestinationExists);
    }

    let payload = snapshot(vault)?;

    // Fail closed: the file is written only once every record in it has been read
    // back and checked, so an export can never quietly lose an object it did not
    // understand.
    let stats = validate(&payload)?;

    let bytes = format::encode(&payload, password, KDF)?;
    let written = bytes.len() as u64;
    write_atomically(&destination, &bytes, request.overwrite)?;

    Ok(ExportSummary {
        file_name: file_name(&destination),
        bytes: written,
        created_at: payload.created_at,
        total_objects: stats.total,
        active_objects: stats.active,
        deleted_objects: stats.deleted,
        count_by_kind: stats.by_kind,
    })
}

/// The whole vault as a backup payload.
///
/// Tombstones come along: a backup is a copy of the table, not of the active
/// lists, so deleting a record and restoring the backup must not bring it back.
fn snapshot(vault: &Vault) -> Result<BackupPayload, BackupError> {
    let objects: Vec<BackupObject> = vault
        .snapshot_objects()?
        .into_iter()
        .map(|object| BackupObject {
            id: object.id,
            kind: object.kind,
            payload: object.payload,
            crypto_version: object.crypto_version,
            created_at: object.created_at,
            updated_at: object.updated_at,
            deleted_at: object.deleted_at,
        })
        .collect();

    Ok(BackupPayload {
        payload_version: PAYLOAD_VERSION,
        created_at: now(),
        source_app_version: env!("CARGO_PKG_VERSION").to_string(),
        objects,
    })
}

// --- inspect --------------------------------------------------------------

/// Reads a backup file and reports how much data it holds.
///
/// The answer is counts only: a preview must never hand a secret, a name or an
/// address to the frontend, and this is where that is decided.
pub(crate) fn inspect(
    vault: &Vault,
    request: &InspectRequest,
) -> Result<BackupSummary, BackupError> {
    ensure_unlocked(vault)?;

    let path = readable_path(&request.source_path)?;
    let bytes = read_backup_file(&path)?;
    let payload = format::decode(&bytes, request.backup_password.as_str())?;
    let stats = validate(&payload)?;

    Ok(BackupSummary {
        created_at: payload.created_at,
        source_app_version: payload.source_app_version,
        total_objects: stats.total,
        active_objects: stats.active,
        deleted_objects: stats.deleted,
        count_by_kind: stats.by_kind,
        // The digest of the very bytes that were just read, so a restore can
        // name this file rather than whatever is at the path later.
        file_digest: file_digest(&bytes),
    })
}

// --- restore --------------------------------------------------------------

/// Everything a restore request has to say before anything at all happens.
///
/// The mode and the typed phrase are checked here as well as in the UI: a
/// disabled button is a convenience, and the backend never relies on it. The
/// session check is not part of this: it belongs to the restore guard in
/// `backup::mod`, which has to see the sessions and claim them in one atomic
/// step rather than looking at them separately.
pub(crate) fn check_restore_request(request: &RestoreRequest) -> Result<(), BackupError> {
    check_replace(request)
}

/// The two fields a replace has to carry, checked on their own as well.
fn check_replace(request: &RestoreRequest) -> Result<(), BackupError> {
    if request.mode != RESTORE_MODE_REPLACE {
        return Err(BackupError::UnsupportedMode);
    }
    if request.confirmation != REPLACE_CONFIRMATION {
        return Err(BackupError::ConfirmationRequired);
    }
    Ok(())
}

/// Replaces every record in this vault with the contents of a backup file.
///
/// The vault keeps its identity: its Vault Key, its master password and its
/// key-derivation parameters are untouched, and every restored record is
/// encrypted again with the key of *this* vault. The backup password only ever
/// opened the file.
pub(crate) fn restore(
    vault: &Vault,
    request: &RestoreRequest,
) -> Result<RestoreSummary, BackupError> {
    ensure_unlocked(vault)?;
    check_replace(request)?;

    let path = readable_path(&request.source_path)?;
    let bytes = read_backup_file(&path)?;
    // The file that is actually on disk has to be the one the user previewed.
    // This runs before any crypto, so a swapped file costs no derivation.
    if file_digest(&bytes) != request.file_digest {
        return Err(BackupError::FileChanged);
    }
    let payload = format::decode(&bytes, request.backup_password.as_str())?;
    let stats = validate(&payload)?;
    let created_at = payload.created_at;

    let objects: Vec<SnapshotObject> = payload
        .objects
        .into_iter()
        .map(|object| SnapshotObject {
            id: object.id,
            kind: object.kind,
            payload: object.payload,
            crypto_version: object.crypto_version,
            created_at: object.created_at,
            updated_at: object.updated_at,
            deleted_at: object.deleted_at,
        })
        .collect();
    let replaced = objects.len();

    // One transaction inside the vault: the old rows and the whole snapshot are a
    // single unit of work, so there is no state where half of it is in place.
    vault.replace_objects(&objects)?;

    Ok(RestoreSummary {
        file_name: file_name(&path),
        created_at,
        replaced_objects: replaced,
        active_objects: stats.active,
        deleted_objects: stats.deleted,
        count_by_kind: stats.by_kind,
    })
}

// --- snapshot validation --------------------------------------------------

/// The counts a summary is built from.
struct Stats {
    total: usize,
    active: usize,
    deleted: usize,
    by_kind: Vec<KindCount>,
}

/// What one record contributes to the snapshot's relations.
///
/// Nothing borrows the record, so a credential's secret is wiped the moment its
/// record has been checked instead of living on for the rest of the validation.
enum Facts {
    Group {
        /// Carried only to check that no two live groups share a name.
        name: String,
    },
    Host {
        group_id: Option<String>,
        credential_id: Option<String>,
    },
    Credential,
    KnownHost {
        host_key_id: String,
    },
    Snippet,
}

/// Checks a whole snapshot and returns the counts it will be reported with.
///
/// This is the only place that decides whether a snapshot is usable, and it runs
/// to the end before a restore touches anything. Everything the file could claim
/// is checked here: supported payload and crypto versions, sane metadata,
/// unique ids, known kinds, consistent timestamps, decodable records that pass
/// the domain's own field rules, ids that match the object that carries them,
/// unique live group names, and the relations between hosts, groups and
/// credentials. A record a command would refuse to write is a record a restore
/// refuses to read, so a backup can never introduce data the app itself would
/// not create.
fn validate(payload: &BackupPayload) -> Result<Stats, BackupError> {
    let objects = &payload.objects;
    check_payload(payload)?;
    if objects.len() > MAX_OBJECTS {
        return Err(BackupError::TooManyObjects);
    }

    let mut ids: HashSet<&str> = HashSet::new();
    let mut active_groups: HashSet<&str> = HashSet::new();
    let mut active_group_names: HashSet<String> = HashSet::new();
    let mut active_credentials: HashSet<&str> = HashSet::new();
    let mut hosts: Vec<(Option<String>, Option<String>)> = Vec::new();
    let mut known_hosts: Vec<String> = Vec::new();
    let mut records: BTreeMap<&str, (usize, usize)> = BTreeMap::new();

    for object in objects {
        if object.id.trim().is_empty() {
            return Err(BackupError::InvalidRecord);
        }
        if !ids.insert(object.id.as_str()) {
            return Err(BackupError::DuplicateId);
        }
        if object.crypto_version != CRYPTO_VERSION {
            return Err(BackupError::UnsupportedCryptoVersion {
                found: object.crypto_version,
                supported: CRYPTO_VERSION,
            });
        }
        check_timestamps(object)?;

        let active = object.deleted_at.is_none();
        let counts = records.entry(object.kind.as_str()).or_insert((0, 0));
        if active {
            counts.0 += 1;
        } else {
            counts.1 += 1;
        }

        // A tombstone is checked exactly like a live record: its payload still
        // has to be the record its kind claims, so a damaged or unknown object
        // cannot hide behind a delete flag.
        match facts(object)? {
            Facts::Group { name } if active => {
                active_groups.insert(object.id.as_str());
                // Two live groups that differ only in case are impossible in the
                // vault the commands write, so a file that holds them is refused
                // rather than restored into a state the UI cannot explain.
                if !active_group_names.insert(name.to_ascii_lowercase()) {
                    return Err(BackupError::DuplicateGroupName);
                }
            }
            Facts::Credential if active => {
                active_credentials.insert(object.id.as_str());
            }
            Facts::Host {
                group_id,
                credential_id,
            } if active => {
                hosts.push((group_id, credential_id));
            }
            Facts::KnownHost { host_key_id } if active => {
                known_hosts.push(host_key_id);
            }
            _ => {}
        }
    }

    // A host may point at a group or a credential, and only at one that is in the
    // backup and still live: a dangling relation would be a record the user
    // cannot fix from the restored data alone.
    for (group_id, credential_id) in hosts {
        if let Some(group_id) = group_id
            && !active_groups.contains(group_id.as_str())
        {
            return Err(BackupError::InvalidRelation);
        }
        if let Some(credential_id) = credential_id
            && !active_credentials.contains(credential_id.as_str())
        {
            return Err(BackupError::InvalidRelation);
        }
    }

    // Two live trusted keys for the same host identity would make the host-key
    // check depend on which one was read first.
    let mut identities: HashSet<&str> = HashSet::new();
    for host_key_id in &known_hosts {
        if !identities.insert(host_key_id.as_str()) {
            return Err(BackupError::DuplicateKnownHost);
        }
    }

    let by_kind = records
        .into_iter()
        .map(|(kind, (active, deleted))| KindCount {
            kind: kind.to_string(),
            active,
            deleted,
        })
        .collect::<Vec<KindCount>>();
    let deleted = objects
        .iter()
        .filter(|object| object.deleted_at.is_some())
        .count();

    Ok(Stats {
        total: objects.len(),
        active: objects.len() - deleted,
        deleted,
        by_kind,
    })
}

/// The backup's own metadata: the version and the source string.
///
/// Both are already typed by the time they arrive; what is left is making sure
/// they are usable. The source version is informational, so a reasonable length
/// and a value that is really a value are all that is asked of it.
fn check_payload(payload: &BackupPayload) -> Result<(), BackupError> {
    if payload.payload_version != PAYLOAD_VERSION {
        return Err(BackupError::UnsupportedPayloadVersion {
            found: u64::from(payload.payload_version),
            supported: PAYLOAD_VERSION,
        });
    }
    if payload.created_at < 0 {
        return Err(BackupError::InvalidPayload);
    }
    let source = payload.source_app_version.as_str();
    if source.trim() != source
        || source.is_empty()
        || source.chars().count() > MAX_SOURCE_APP_VERSION_LEN
    {
        return Err(BackupError::InvalidPayload);
    }
    Ok(())
}

/// What one record is, once it has been decoded the way a command would read it.
///
/// The record's own id has to be the id of the object carrying it: a file that
/// disagrees with itself is refused rather than restored half-guessed. The
/// record is also put through the same field rules the commands apply, so a
/// payload that merely parses is not enough to be restored.
fn facts(object: &BackupObject) -> Result<Facts, BackupError> {
    if object.kind == KIND_SNIPPET {
        let snippet = snippets::decode_snippet(object.payload.as_str())?;
        check_id(object, &snippet.id)?;
        check_record_timestamps(object, snippet.created_at, snippet.updated_at)?;
        snippets::check_snippet(&snippet)?;
        return Ok(Facts::Snippet);
    }

    let record = inventory::decode_record(object.kind.as_str(), object.payload.as_str())?;
    inventory::check_record(&record)?;
    match record {
        Record::Group(group) => {
            check_id(object, &group.id)?;
            check_record_timestamps(object, group.created_at, group.updated_at)?;
            Ok(Facts::Group { name: group.name })
        }
        Record::Host(host) => {
            check_id(object, &host.id)?;
            check_record_timestamps(object, host.created_at, host.updated_at)?;
            Ok(Facts::Host {
                group_id: host.group_id,
                credential_id: host.credential_id,
            })
        }
        Record::Credential(credential) => {
            check_id(object, &credential.id)?;
            check_record_timestamps(object, credential.created_at, credential.updated_at)?;
            Ok(Facts::Credential)
        }
        Record::KnownHost(known) => {
            check_id(object, &known.id)?;
            check_record_timestamps(object, known.created_at, known.updated_at)?;
            Ok(Facts::KnownHost {
                host_key_id: known.host_key_id,
            })
        }
    }
}

fn check_id(object: &BackupObject, id: &str) -> Result<(), BackupError> {
    if id != object.id {
        return Err(BackupError::InvalidRecord);
    }
    Ok(())
}

/// The record's own timestamps have to be real and to agree with the row that
/// carries it.
///
/// Nothing here is something a command can write: a record created or updated
/// before the epoch, a record updated before it was created, and a payload that
/// claims a newer history than its own metadata are all refused. Equality is not
/// required, and neither is an exact match: the commands write the row's
/// timestamps with their own `now()` a moment later, and deleting a record moves
/// the row forward while the payload keeps its last update, so internal
/// timestamps that are *older* than the object's are the normal case.
fn check_record_timestamps(
    object: &BackupObject,
    created_at: i64,
    updated_at: i64,
) -> Result<(), BackupError> {
    if created_at < 0
        || updated_at < created_at
        || created_at > object.created_at
        || updated_at > object.updated_at
    {
        return Err(BackupError::InvalidRecord);
    }
    Ok(())
}

/// Timestamps have to be real and consistent: a record cannot have been updated
/// or deleted before it was created, and a tombstone cannot be older than the
/// update that preceded it.
fn check_timestamps(object: &BackupObject) -> Result<(), BackupError> {
    if object.created_at < 0 || object.updated_at < object.created_at {
        return Err(BackupError::InvalidRecord);
    }
    if let Some(deleted_at) = object.deleted_at
        && deleted_at < object.updated_at
    {
        return Err(BackupError::InvalidRecord);
    }
    Ok(())
}

// --- files ----------------------------------------------------------------

/// A path a backup may be written to.
///
/// The path comes from the native save dialog, so this only refuses what the
/// backend must never do on its own: write to a directory, to a path with no file
/// name, or into a directory that does not exist. The backend never creates a
/// directory and never follows a path out of the file the user picked.
fn writable_path(value: &str) -> Result<PathBuf, BackupError> {
    let path = usable_path(value)?;
    if !directory_of(&path)?.is_dir() {
        return Err(BackupError::InvalidPath);
    }
    Ok(path)
}

/// A path a backup may be read from.
fn readable_path(value: &str) -> Result<PathBuf, BackupError> {
    let path = usable_path(value)?;
    if path.is_dir() {
        return Err(BackupError::NotAFile);
    }
    Ok(path)
}

fn usable_path(value: &str) -> Result<PathBuf, BackupError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(BackupError::InvalidPath);
    }
    let path = PathBuf::from(trimmed);
    if path.file_name().is_none() {
        return Err(BackupError::InvalidPath);
    }
    Ok(path)
}

fn directory_of(path: &Path) -> Result<&Path, BackupError> {
    Ok(match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    })
}

/// The file's name, which is all a summary reports.
///
/// The rest of the path is the user's directory layout; the UI has no use for it
/// and a report should not repeat it.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Reads a backup file, refusing anything that is not a regular file inside the
/// size limit.
///
/// The bytes come from one open handle and never more than `MAX_BACKUP_BYTES + 1`
/// of them are read from it: a file that is replaced or grown between the size
/// check and the read can therefore not make this allocate past the limit, and a
/// file that turns out to be longer is refused instead of truncated. There is no
/// unbounded read of a backup file anywhere.
///
/// A symbolic link is refused too, best-effort: the check and the open are two
/// steps, so a link that appears between them would still be followed. That is
/// acceptable here — the path is one the user picked in the native dialog — and
/// it costs no guarantee that matters, because the byte limit is applied to the
/// stream that was really opened rather than to the path.
fn read_backup_file(path: &Path) -> Result<Vec<u8>, BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BackupError::CannotRead)?;
    if !metadata.is_file() {
        return Err(BackupError::NotAFile);
    }

    let file = fs::File::open(path).map_err(|_| BackupError::CannotRead)?;
    let length = file.metadata().map_err(|_| BackupError::CannotRead)?.len();
    if length > MAX_BACKUP_BYTES as u64 {
        return Err(BackupError::TooLarge);
    }

    read_bounded(file, MAX_BACKUP_BYTES)
}

/// Reads at most `limit` bytes from a stream, and refuses a longer one.
///
/// Exactly one byte past the limit is read, so a stream that is over the limit is
/// told apart from one that is exactly at it without ever holding more than
/// `limit + 1` bytes or following the stream to its end. The tests drive this
/// directly, which is how the stop-at-the-limit behaviour is proved without a
/// filesystem that can change under the read.
pub(super) fn read_bounded<R: Read>(reader: R, limit: usize) -> Result<Vec<u8>, BackupError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BackupError::CannotRead)?;
    if bytes.len() > limit {
        return Err(BackupError::TooLarge);
    }
    Ok(bytes)
}

/// Publishes the backup bytes at the destination, as a whole file or not at all.
///
/// The bytes go to a temporary file in the destination's own directory — never
/// somewhere that could be a different filesystem — are flushed to the platform
/// and are then published there:
///
/// - with `overwrite`, the destination is replaced in one move. This is the one
///   portable way to do it: on Unix `rename` replaces an existing file, while on
///   Windows `std::fs::rename` refuses to, and `tempfile` picks the right
///   primitive for each platform.
/// - without `overwrite`, the move is no-clobber, and that decision belongs to
///   the filesystem rather than to a look at the destination first. A file that
///   appears between the caller's check and this write is therefore never
///   replaced, which is what closes the check-then-rename race.
///
/// A temporary file that is never published removes itself: `tempfile` deletes
/// it when it is dropped, on every error path and on a panic alike. The bytes
/// are always the ciphertext envelope, so no plaintext ever reaches a temporary
/// file either. `tempfile` creates the file with owner-only permissions on the
/// platforms that have them.
pub(super) fn write_atomically(
    destination: &Path,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), BackupError> {
    let directory = directory_of(destination)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".termust-backup-")
        .tempfile_in(directory)
        .map_err(|_| BackupError::CannotWrite)?;

    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|_| BackupError::CannotWrite)?;

    if overwrite {
        temporary
            .persist(destination)
            .map_err(|_| BackupError::CannotWrite)?;
    } else {
        temporary.persist_noclobber(destination).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                BackupError::DestinationExists
            } else {
                BackupError::CannotWrite
            }
        })?;
    }
    Ok(())
}

/// The digest a preview reports and a restore has to match.
///
/// SHA-256 over the file's bytes, written as `sha256:<base64>`. It is not a
/// secret and not a key; it exists so a restore names the exact file the user
/// previewed instead of whatever happens to be at that path later.
fn file_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{}", BASE64.encode(digest.as_slice()))
}

fn ensure_unlocked(vault: &Vault) -> Result<(), BackupError> {
    if vault.is_unlocked() {
        return Ok(());
    }
    Err(BackupError::Vault(VaultError::Locked))
}

impl From<VaultError> for BackupError {
    fn from(error: VaultError) -> Self {
        // Storage failures carry SQLite's own text, so they are not shown.
        Self::Vault(error)
    }
}

impl From<InventoryError> for BackupError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::Vault(error) => Self::Vault(error),
            InventoryError::UnsupportedPayloadVersion { found, supported } => {
                Self::UnsupportedPayloadVersion { found, supported }
            }
            InventoryError::UnknownKind => Self::UnknownKind,
            _ => Self::InvalidRecord,
        }
    }
}

impl From<SnippetError> for BackupError {
    fn from(error: SnippetError) -> Self {
        match error {
            SnippetError::Vault(error) => Self::Vault(error),
            SnippetError::UnsupportedPayloadVersion { found, supported } => {
                Self::UnsupportedPayloadVersion { found, supported }
            }
            _ => Self::InvalidRecord,
        }
    }
}
