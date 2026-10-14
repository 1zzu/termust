//! Behaviour tests for encrypted vault backups.
//!
//! Every test runs against its own temporary vault database and its own temporary
//! directory for backup files, created with the cheap Argon2id parameters, and
//! removes both afterwards. No test touches a real vault, the app data directory,
//! the network or another process.
//!
//! What is checked here is the part behind the three commands: the format, the
//! file handling, the snapshot validation and the atomic replace. The wire
//! contract is pinned in `lib.rs`, and the end-to-end path — a restored host
//! connecting to a real server — in `ssh::live`.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::inventory::{
    CredentialSecret, HostInput, KnownKey, create_group_record, create_host_record,
    create_password_record,
};
use crate::snippets::SnippetInput;
use crate::ssh::SshState;
use crate::vault::crypto::{KDF_LIMITS, TEST, derive_kek, open_backup, seal_backup};
use crate::vault::test_support::TempVault;
use crate::vault::{CRYPTO_VERSION, SnapshotObject, Vault, VaultError, VaultState};

use super::model::{
    BACKUP_AAD, BackupObject, BackupPayload, FORMAT_VERSION, MAX_BACKUP_BYTES, MAX_OBJECTS,
    PAYLOAD_VERSION, REPLACE_CONFIRMATION, RESTORE_MODE_REPLACE, RestoreRequest,
};
use super::{BackupError, format, model, service};

/// The master password the source vault is created with.
const SOURCE_PASSWORD: &str = "source-master-password";
/// The master password the destination vault is created with. Different on
/// purpose: a backup has to survive a move to another vault.
const DESTINATION_PASSWORD: &str = "destination-master-password";
/// The backup password, which opens the file and nothing else.
const BACKUP_PASSWORD: &str = "a-strong-backup-password";

/// Distinctive values for the plaintext scans. None of them may appear in a
/// backup file, in a database, or in an error.
const SECRET_MARKER: &str = "marker-credential-secret-4b7c";
const SNIPPET_NAME_MARKER: &str = "marker-snippet-name-2d91";
const SNIPPET_CONTENT_MARKER: &str = "marker-snippet-content-8a3f";
const ADDRESS_MARKER: &str = "marker-address-6e15.example.com";
const USERNAME_MARKER: &str = "marker-username-9c40";
const KNOWN_HOST_ID_MARKER: &str = "marker-known-host-1f83.example.com:2222";
const FINGERPRINT_MARKER: &str = "SHA256:marker-fingerprint-5a26";

// --- scaffolding ----------------------------------------------------------

/// A temporary directory that removes itself, for the files an export writes.
struct TempFiles {
    directory: PathBuf,
}

impl TempFiles {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("termust-backup-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("the temporary directory was created");
        Self { directory }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    fn text_path(&self, name: &str) -> String {
        self.path(name).to_string_lossy().into_owned()
    }

    /// What is really in the directory, sorted, so a test can say "nothing was
    /// left behind" and "nothing was overwritten".
    fn names(&self) -> Vec<String> {
        file_names(&self.directory)
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn file_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("the directory was read")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// A temporary vault that has been created and is left unlocked.
fn unlocked_vault(password: &str) -> (TempVault, Vault) {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(password).expect("the vault was created");
    (temp, vault)
}

/// Every logical row of a vault, as text, for comparing before and after an
/// operation. The vault metadata is included, so a test can also say "the vault
/// itself did not change".
fn database_state(path: &Path) -> Vec<String> {
    let connection = Connection::open(path).expect("the database opened");
    let queries = [
        "SELECT id, schema_version, crypto_version, kdf_algorithm, hex(kdf_salt),
                kdf_memory_cost, kdf_time_cost, kdf_parallelism, hex(wrapped_vault_key),
                hex(wrap_nonce), created_at, updated_at
         FROM vault_meta ORDER BY id",
        "SELECT id, kind, hex(encrypted_payload), hex(nonce), crypto_version,
                created_at, updated_at, deleted_at
         FROM vault_objects ORDER BY id",
    ];

    let mut state = Vec::new();
    for query in queries {
        let mut statement = connection.prepare(query).expect("the query prepared");
        let columns = statement.column_count();
        let mut rows = statement.query([]).expect("the query ran");
        while let Some(row) = rows.next().expect("a row") {
            let mut line = String::new();
            for index in 0..columns {
                let value: rusqlite::types::Value = row.get(index).expect("a value");
                line.push_str(&format!("{value:?}|"));
            }
            state.push(line);
        }
    }
    state
}

/// Every stored object of one kind on disk, as `(id, ciphertext)`, tombstones
/// included.
fn stored_objects(path: &Path, kind: &str) -> Vec<(String, Vec<u8>)> {
    let connection = Connection::open(path).expect("the database opened");
    let mut statement = connection
        .prepare("SELECT id, encrypted_payload FROM vault_objects WHERE kind = ?1 ORDER BY id")
        .expect("the query prepared");
    let rows = statement
        .query_map([kind], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("the query ran");
    rows.map(|row| row.expect("a row")).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// The ids and contents of everything a vault holds, read the way the app reads
/// them, for comparing a restore against what was exported.
fn contents(vault: &Vault) -> String {
    let groups = crate::inventory::list_group_records(vault).expect("the groups were listed");
    let hosts = crate::inventory::list_host_records(vault).expect("the hosts were listed");
    let known =
        crate::inventory::list_known_host_records(vault).expect("the known hosts were listed");
    let mut lines = Vec::new();
    for group in groups {
        lines.push(format!("group {} {}", group.id, group.name));
    }
    for host in hosts {
        lines.push(format!(
            "host {} {} {} {} {:?} {:?}",
            host.id, host.label, host.address, host.username, host.group_id, host.credential_id
        ));
    }
    for record in known {
        lines.push(format!(
            "known {} {} {} {}",
            record.id, record.host_key_id, record.key_algorithm, record.fingerprint_sha256
        ));
    }
    lines.sort();
    lines.join("\n")
}

/// One of every record, plus a deleted host so tombstones travel too. Every
/// value is a marker.
fn fill(vault: &Vault) -> Fixture {
    let group = create_group_record(vault, "Work").expect("the group was created");
    let credential =
        create_password_record(vault, "Prod", SECRET_MARKER).expect("the credential was created");
    let host = create_host_record(
        vault,
        &HostInput {
            label: "Prod web".to_string(),
            address: ADDRESS_MARKER.to_string(),
            port: 2222,
            username: USERNAME_MARKER.to_string(),
            group_id: Some(group.id.clone()),
            credential_id: Some(credential.id.clone()),
        },
    )
    .expect("the host was created");
    let known = crate::inventory::remember_known_host(
        vault,
        KNOWN_HOST_ID_MARKER,
        &KnownKey {
            algorithm: "ssh-ed25519".to_string(),
            fingerprint: FINGERPRINT_MARKER.to_string(),
        },
    )
    .expect("the known host was recorded");
    let snippet = crate::snippets::create_snippet_record(
        vault,
        &SnippetInput {
            name: SNIPPET_NAME_MARKER.to_string(),
            content: SNIPPET_CONTENT_MARKER.to_string(),
        },
    )
    .expect("the snippet was created");

    // A host that is deleted before the export: the backup has to carry it as a
    // tombstone, and the schema has no column for a plaintext address, so this
    // also proves a deleted record is not kept anywhere else.
    let deleted = create_host_record(
        vault,
        &HostInput {
            label: "Old box".to_string(),
            address: "old.example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            group_id: None,
            credential_id: None,
        },
    )
    .expect("the second host was created");
    vault
        .delete_object(&deleted.id)
        .expect("the host was deleted");

    Fixture {
        group: group.id,
        credential: credential.id,
        host: host.id,
        known_host: known.id,
        snippet: snippet.id,
        deleted_host: deleted.id,
    }
}

struct Fixture {
    group: String,
    credential: String,
    host: String,
    known_host: String,
    snippet: String,
    deleted_host: String,
}

// --- requests -------------------------------------------------------------

fn export_request(path: &Path, password: &str) -> model::ExportRequest {
    model::ExportRequest {
        destination_path: path.to_string_lossy().into_owned(),
        backup_password: Zeroizing::new(password.to_string()),
        overwrite: false,
    }
}

fn inspect_request(path: &Path, password: &str) -> model::InspectRequest {
    model::InspectRequest {
        source_path: path.to_string_lossy().into_owned(),
        backup_password: Zeroizing::new(password.to_string()),
    }
}

fn restore_request(path: &Path, password: &str) -> RestoreRequest {
    RestoreRequest {
        source_path: path.to_string_lossy().into_owned(),
        backup_password: Zeroizing::new(password.to_string()),
        mode: RESTORE_MODE_REPLACE.to_string(),
        confirmation: REPLACE_CONFIRMATION.to_string(),
        file_digest: digest_of(path),
    }
}

/// What a preview would report for a file: the digest a restore has to match.
///
/// A file that is not there has an empty digest, which a restore never looks at
/// because it fails to read the file first.
fn digest_of(path: &Path) -> String {
    use base64::Engine as _;
    use sha2::{Digest as _, Sha256};

    match fs::read(path) {
        Ok(bytes) => {
            let digest = Sha256::digest(&bytes);
            format!(
                "sha256:{}",
                base64::engine::general_purpose::STANDARD.encode(digest.as_slice())
            )
        }
        Err(_) => String::new(),
    }
}

// --- hand-built backup files ----------------------------------------------

/// One record of a backup built by a test, rather than by an export.
fn object(id: &str, kind: &str, payload: String, deleted_at: Option<i64>) -> BackupObject {
    BackupObject {
        id: id.to_string(),
        kind: kind.to_string(),
        payload: Zeroizing::new(payload),
        crypto_version: CRYPTO_VERSION,
        created_at: 10,
        updated_at: 10,
        deleted_at,
    }
}

fn group_object(id: &str) -> BackupObject {
    object(
        id,
        "group",
        json!({ "version": 1, "id": id, "name": "Work", "createdAt": 10, "updatedAt": 10 })
            .to_string(),
        None,
    )
}

fn credential_object(id: &str) -> BackupObject {
    object(
        id,
        "credential",
        json!({
            "version": 1,
            "id": id,
            "name": "Prod",
            "secret": { "kind": "password", "password": SECRET_MARKER },
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        None,
    )
}

fn host_object(id: &str, group_id: Option<&str>, credential_id: Option<&str>) -> BackupObject {
    object(
        id,
        "host",
        json!({
            "version": 1,
            "id": id,
            "label": "Prod web",
            "address": ADDRESS_MARKER,
            "port": 2222,
            "username": USERNAME_MARKER,
            "groupId": group_id,
            "credentialId": credential_id,
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        None,
    )
}

fn known_host_object(id: &str, host_key_id: &str, deleted_at: Option<i64>) -> BackupObject {
    object(
        id,
        "known_host",
        json!({
            "version": 1,
            "id": id,
            "hostKeyId": host_key_id,
            "keyAlgorithm": "ssh-ed25519",
            "fingerprintSha256": FINGERPRINT_MARKER,
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        deleted_at,
    )
}

fn payload(objects: Vec<BackupObject>) -> BackupPayload {
    BackupPayload {
        payload_version: PAYLOAD_VERSION,
        created_at: 1_700_000_000,
        source_app_version: "0.1.0".to_string(),
        objects,
    }
}

/// Writes a hand-built payload to a file, encrypted with the test parameters —
/// the same envelope a real export writes, without going through the vault.
fn write_sealed(path: &Path, payload: &BackupPayload, password: &str) {
    let bytes = format::encode(payload, password, TEST).expect("the payload was encrypted");
    fs::write(path, bytes).expect("the backup was written");
}

/// Rewrites a backup file with a different head, leaving its ciphertext alone.
fn rewrite_envelope(path: &Path, change: impl FnOnce(&mut serde_json::Value)) {
    let bytes = fs::read(path).expect("the backup was read");
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).expect("the head parsed");
    change(&mut envelope);
    fs::write(
        path,
        serde_json::to_vec_pretty(&envelope).expect("the head was written"),
    )
    .expect("the backup was rewritten");
}

// --- export ---------------------------------------------------------------

#[test]
fn an_export_writes_one_encrypted_file_and_an_inspect_reads_it() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("termust-backup-2026-10-14.termust-backup");

    let exported = service::export(&vault, &export_request(&destination, BACKUP_PASSWORD))
        .expect("the export succeeded");

    assert_eq!(
        exported.file_name,
        "termust-backup-2026-10-14.termust-backup"
    );
    assert_eq!(exported.total_objects, 6);
    assert_eq!(exported.active_objects, 5);
    assert_eq!(exported.deleted_objects, 1);
    assert!(exported.bytes > 0);
    assert!(exported.created_at > 0);
    assert_eq!(files.names(), vec![exported.file_name.clone()]);

    let inspected = service::inspect(&vault, &inspect_request(&destination, BACKUP_PASSWORD))
        .expect("the inspect succeeded");
    assert_eq!(
        inspected,
        model::BackupSummary {
            created_at: exported.created_at,
            source_app_version: env!("CARGO_PKG_VERSION").to_string(),
            total_objects: 6,
            active_objects: 5,
            deleted_objects: 1,
            count_by_kind: exported.count_by_kind.clone(),
            file_digest: digest_of(&destination),
        }
    );
    assert_eq!(
        inspected.count_by_kind,
        vec![
            model::KindCount {
                kind: "credential".to_string(),
                active: 1,
                deleted: 0
            },
            model::KindCount {
                kind: "group".to_string(),
                active: 1,
                deleted: 0
            },
            model::KindCount {
                kind: "host".to_string(),
                active: 1,
                deleted: 1
            },
            model::KindCount {
                kind: "known_host".to_string(),
                active: 1,
                deleted: 0
            },
            model::KindCount {
                kind: "snippet".to_string(),
                active: 1,
                deleted: 0
            },
        ]
    );
}

#[test]
fn an_empty_vault_exports_and_restores() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    let destination = files.path("empty.termust-backup");

    let exported = service::export(&vault, &export_request(&destination, BACKUP_PASSWORD))
        .expect("the export succeeded");
    assert_eq!(exported.total_objects, 0);
    assert_eq!(exported.count_by_kind, vec![]);

    let inspected = service::inspect(&vault, &inspect_request(&destination, BACKUP_PASSWORD))
        .expect("the inspect succeeded");
    assert_eq!(inspected.total_objects, 0);
    assert_eq!(inspected.active_objects, 0);
    assert_eq!(inspected.source_app_version, env!("CARGO_PKG_VERSION"));

    // Restoring nothing is still a replace: the destination is emptied.
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&target);
    let summary = service::restore(&target, &restore_request(&destination, BACKUP_PASSWORD))
        .expect("the restore succeeded");
    assert_eq!(summary.replaced_objects, 0);
    assert_eq!(contents(&target), "");
}

#[test]
fn two_exports_of_one_vault_share_no_bytes() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);

    let first_path = files.path("first.termust-backup");
    let second_path = files.path("second.termust-backup");
    service::export(&vault, &export_request(&first_path, BACKUP_PASSWORD)).expect("the export");
    service::export(&vault, &export_request(&second_path, BACKUP_PASSWORD)).expect("the export");

    let first: serde_json::Value =
        serde_json::from_slice(&fs::read(&first_path).unwrap()).expect("the first head");
    let second: serde_json::Value =
        serde_json::from_slice(&fs::read(&second_path).unwrap()).expect("the second head");

    // A fresh salt and a fresh nonce for every export, so neither the key nor the
    // ciphertext can repeat.
    assert_ne!(first["kdf"]["salt"], second["kdf"]["salt"]);
    assert_ne!(first["nonce"], second["nonce"]);
    assert_ne!(first["ciphertext"], second["ciphertext"]);

    // Both still open with the same password.
    for path in [&first_path, &second_path] {
        service::inspect(&vault, &inspect_request(path, BACKUP_PASSWORD)).expect("it opens");
    }
}

#[test]
fn the_backup_file_carries_only_the_head_in_the_clear() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("head.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    let head: serde_json::Value =
        serde_json::from_slice(&fs::read(&destination).unwrap()).expect("the head parsed");
    let keys: Vec<String> = head
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "cipher".to_string(),
            "ciphertext".to_string(),
            "formatVersion".to_string(),
            "kdf".to_string(),
            "magic".to_string(),
            "nonce".to_string(),
        ],
        "the head grew a field"
    );

    assert_eq!(head["magic"], super::model::MAGIC);
    assert_eq!(head["formatVersion"], 1);
    assert_eq!(head["cipher"], super::model::BACKUP_CIPHER);
    assert_eq!(head["kdf"]["algorithm"], super::model::BACKUP_KDF);
    assert_eq!(head["kdf"]["memoryCost"], TEST.memory_cost);
    assert_eq!(head["kdf"]["timeCost"], TEST.time_cost);
    assert_eq!(head["kdf"]["parallelism"], TEST.parallelism);

    // The head carries no domain data at all: no counts, no names, no addresses,
    // not even how many records the file holds.
    let text = fs::read_to_string(&destination).expect("the file is text");
    assert!(!text.contains("objects"), "the head named the record list");
    assert!(!text.contains("createdAt"), "the head named a timestamp");
    for marker in [
        ADDRESS_MARKER,
        USERNAME_MARKER,
        SECRET_MARKER,
        SNIPPET_CONTENT_MARKER,
        SNIPPET_NAME_MARKER,
        KNOWN_HOST_ID_MARKER,
        FINGERPRINT_MARKER,
        SOURCE_PASSWORD,
        BACKUP_PASSWORD,
    ] {
        assert!(
            !text.contains(marker),
            "the backup file holds {marker} in the clear"
        );
    }
}

#[test]
fn a_backup_leaves_no_marker_in_the_destination_database() {
    let (source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let destination = files.path("restore.termust-backup");
    service::export(&source, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");
    service::restore(&target, &restore_request(&destination, BACKUP_PASSWORD))
        .expect("the restore succeeded");

    let database = fs::read(target.path()).expect("the database was read");
    for marker in [
        SECRET_MARKER,
        SNIPPET_CONTENT_MARKER,
        SNIPPET_NAME_MARKER,
        ADDRESS_MARKER,
        USERNAME_MARKER,
        FINGERPRINT_MARKER,
        SOURCE_PASSWORD,
        DESTINATION_PASSWORD,
        BACKUP_PASSWORD,
    ] {
        assert!(
            !contains(&database, marker.as_bytes()),
            "the restored database holds {marker} in the clear"
        );
    }

    // The restored vault is backed by its own database file, and the source vault
    // still holds everything it held before the export.
    assert_ne!(source.path(), target.path());
    assert_eq!(source_temp.database(), source.path());
    assert_eq!(contents(&source).lines().count(), 3);
}

#[test]
fn an_export_needs_a_password_of_at_least_twelve_characters() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    let destination = files.path("short.termust-backup");

    let error = service::export(&vault, &export_request(&destination, "eleven-char"))
        .expect_err("eleven characters is too short");

    assert!(matches!(error, BackupError::PasswordTooShort), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!(
            "The backup password must be at least {} characters.",
            model::MIN_PASSWORD_LEN
        )
    );
    assert_eq!(files.names(), Vec::<String>::new(), "a file was written");
}

#[test]
fn an_export_does_not_replace_a_file_it_was_not_told_to_replace() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("existing.termust-backup");
    fs::write(&destination, b"a backup the user already had").expect("the file was written");
    let before = fs::read(&destination).expect("the file was read");

    let error = service::export(&vault, &export_request(&destination, BACKUP_PASSWORD))
        .expect_err("an existing file stops the export");

    assert!(matches!(error, BackupError::DestinationExists), "{error:?}");
    assert_eq!(
        fs::read(&destination).expect("the file was read"),
        before,
        "the existing file was replaced"
    );
    assert_eq!(files.names(), vec!["existing.termust-backup".to_string()]);

    // With the user's answer, it is replaced — and only then.
    let replaced = model::ExportRequest {
        overwrite: true,
        ..export_request(&destination, BACKUP_PASSWORD)
    };
    service::export(&vault, &replaced).expect("the export succeeded");
    assert_ne!(fs::read(&destination).expect("the file was read"), before);
    assert_eq!(files.names(), vec!["existing.termust-backup".to_string()]);
    service::inspect(&vault, &inspect_request(&destination, BACKUP_PASSWORD))
        .expect("the replaced file opens");
}

#[test]
fn an_export_does_not_change_the_vault() {
    let (temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    service::export(
        &vault,
        &export_request(&files.path("x.backup"), BACKUP_PASSWORD),
    )
    .expect("the export succeeded");

    assert_eq!(
        database_state(&temp.database()),
        before,
        "an export wrote to the vault"
    );
}

#[test]
fn an_export_refuses_a_record_of_a_kind_it_does_not_know() {
    let (temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    vault
        .put_object("note", &json!({ "version": 1, "text": "hi" }).to_string())
        .expect("the object was written");
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    let error = service::export(
        &vault,
        &export_request(&files.path("x.backup"), BACKUP_PASSWORD),
    )
    .expect_err("an unknown kind fails the export closed");

    assert!(matches!(error, BackupError::UnknownKind), "{error:?}");
    assert_eq!(
        files.names(),
        Vec::<String>::new(),
        "a file was written anyway"
    );
    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn an_export_refuses_a_record_with_an_unsupported_payload_version() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    // A record written by a build that knows a newer payload version.
    let id = crate::vault::new_id().expect("an id");
    vault
        .put_object_with_id(
            &id,
            "group",
            &json!({ "version": 9, "id": id, "name": "Future", "createdAt": 1, "updatedAt": 1 })
                .to_string(),
        )
        .expect("the object was written");
    let files = TempFiles::new();

    let error = service::export(
        &vault,
        &export_request(&files.path("x.backup"), BACKUP_PASSWORD),
    )
    .expect_err("an unsupported payload version fails the export closed");

    assert!(
        matches!(
            error,
            BackupError::UnsupportedPayloadVersion {
                found: 9,
                supported: 1
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        files.names(),
        Vec::<String>::new(),
        "a file was written anyway"
    );
}

#[test]
fn an_export_refuses_a_corrupt_object() {
    let (temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    let connection = Connection::open(temp.database()).expect("the database opened");
    let mut payload: Vec<u8> = connection
        .query_row(
            "SELECT encrypted_payload FROM vault_objects WHERE kind = 'group'",
            [],
            |row| row.get(0),
        )
        .expect("the row exists");
    payload[0] ^= 0x01;
    connection
        .execute(
            "UPDATE vault_objects SET encrypted_payload = ?1 WHERE kind = 'group'",
            rusqlite::params![payload],
        )
        .expect("the row was damaged");
    let files = TempFiles::new();

    let error = service::export(
        &vault,
        &export_request(&files.path("x.backup"), BACKUP_PASSWORD),
    )
    .expect_err("a damaged object fails the export");

    assert!(
        matches!(error, BackupError::Vault(VaultError::Corrupt(_))),
        "{error:?}"
    );
    assert!(
        !error.to_string().contains(SECRET_MARKER),
        "the error repeated a secret"
    );
    assert_eq!(
        files.names(),
        Vec::<String>::new(),
        "a file was written anyway"
    );
}

#[test]
fn an_export_refuses_a_directory_or_an_empty_path() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();

    for (path, why) in [
        (String::new(), "an empty path"),
        ("   ".to_string(), "a path of spaces"),
        ("/".to_string(), "a path with no file name"),
        (
            files.text_path("no-such-directory/backup.termust-backup"),
            "a directory that is not there",
        ),
    ] {
        let request = model::ExportRequest {
            destination_path: path,
            backup_password: Zeroizing::new(BACKUP_PASSWORD.to_string()),
            overwrite: true,
        };
        let error = service::export(&vault, &request).expect_err(why);
        assert!(
            matches!(error, BackupError::InvalidPath),
            "{why}: {error:?}"
        );
    }
    assert_eq!(files.names(), Vec::<String>::new());
}

#[test]
fn a_failed_export_leaves_no_temporary_file_behind() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    let files = TempFiles::new();

    // A directory sitting where the file should go: the write is started and the
    // rename cannot replace it, which is exactly the case a temporary file would
    // survive if it were not cleaned up.
    let taken = files.path("taken.termust-backup");
    fs::create_dir(&taken).expect("the directory was created");
    fs::write(taken.join("inside"), b"not empty").expect("the file was written");
    let request = model::ExportRequest {
        overwrite: true,
        ..export_request(&taken, BACKUP_PASSWORD)
    };

    let error = service::export(&vault, &request).expect_err("a directory cannot be replaced");

    assert!(matches!(error, BackupError::CannotWrite), "{error:?}");
    assert_eq!(
        files.names(),
        vec!["taken.termust-backup".to_string()],
        "a temporary file was left behind"
    );
    assert!(taken.is_dir(), "the directory was removed");
}

#[cfg(unix)]
#[test]
fn an_exported_file_is_readable_only_by_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    let files = TempFiles::new();
    let destination = files.path("private.termust-backup");

    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    let mode = fs::metadata(&destination)
        .expect("the file exists")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the backup is readable by others");
}

#[test]
fn an_export_is_allowed_while_a_session_is_open() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);

    // Export only reads, so an open session does not stop it — while the same
    // session refuses a restore, which would change the records it was started
    // from. The refusal is the guard's: it cannot be taken while a session
    // exists, and the same critical section stops a session from appearing
    // afterwards.
    let sessions = SshState::default();
    sessions.record_input("session-1");

    service::export(
        &vault,
        &export_request(&files.path("x.backup"), BACKUP_PASSWORD),
    )
    .expect("an export reads only");
    assert!(
        sessions.begin_maintenance().is_none(),
        "a restore guard was taken while a session was open"
    );
}

// --- inspect --------------------------------------------------------------

#[test]
fn an_inspect_writes_nothing_at_all() {
    let (temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("read-only.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    let before_database = database_state(&temp.database());
    let before_directory = files.names();
    let before_file = fs::read(&destination).expect("the file was read");

    let inspected = service::inspect(&vault, &inspect_request(&destination, BACKUP_PASSWORD))
        .expect("the inspect succeeded");

    assert_eq!(inspected.total_objects, 6);
    assert_eq!(database_state(&temp.database()), before_database);
    assert_eq!(files.names(), before_directory);
    assert_eq!(
        fs::read(&destination).expect("the file was read"),
        before_file
    );
}

#[test]
fn the_backup_commands_refuse_a_locked_vault() {
    let (_temp, mut vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    let files = TempFiles::new();
    let destination = files.path("locked.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");
    vault.lock();

    let locked = "The vault is locked. Unlock it first.";
    let error = service::export(
        &vault,
        &export_request(&files.path("x.backup"), BACKUP_PASSWORD),
    )
    .expect_err("a locked vault exports nothing");
    assert_eq!(error.to_string(), locked);

    let error = service::inspect(&vault, &inspect_request(&destination, BACKUP_PASSWORD))
        .expect_err("a locked vault inspects nothing");
    assert_eq!(error.to_string(), locked);

    let error = service::restore(&vault, &restore_request(&destination, BACKUP_PASSWORD))
        .expect_err("a locked vault restores nothing");
    assert_eq!(error.to_string(), locked);
}

// --- wrong password, damage, and unsupported files -------------------------

/// Every way a file can fail to open, with the message the user sees.
fn refused(vault: &Vault, path: &Path, password: &str) -> BackupError {
    service::inspect(vault, &inspect_request(path, password)).expect_err("the file was refused")
}

#[test]
fn a_wrong_backup_password_is_refused_with_one_generic_message() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("secret.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");
    let before = contents(&vault);
    let with_space = format!("{BACKUP_PASSWORD} ");

    for wrong in [
        "not-the-backup-password",
        "",
        SOURCE_PASSWORD,
        with_space.as_str(),
    ] {
        let error = refused(&vault, &destination, wrong);
        assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
        assert_eq!(
            error.to_string(),
            "The backup could not be opened. The password is wrong or the file is damaged.",
            "the message has to be the same for every failure"
        );
        for marker in [BACKUP_PASSWORD, SOURCE_PASSWORD, SECRET_MARKER] {
            assert!(
                !error.to_string().contains(marker),
                "the error repeated {marker}"
            );
        }
    }

    // The vault was not touched by any of those attempts.
    assert_eq!(contents(&vault), before);
}

#[test]
fn a_flipped_ciphertext_byte_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("flipped.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    rewrite_envelope(&destination, |envelope| {
        let mut bytes = base64_decode(envelope["ciphertext"].as_str().expect("ciphertext"));
        bytes[0] ^= 0x01;
        envelope["ciphertext"] = json!(base64_encode(&bytes));
    });

    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
}

#[test]
fn a_flipped_authentication_tag_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("tag.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    // The tag is the last 16 bytes of the ciphertext, so this damages the
    // authentication check and nothing else.
    rewrite_envelope(&destination, |envelope| {
        let mut bytes = base64_decode(envelope["ciphertext"].as_str().expect("ciphertext"));
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        envelope["ciphertext"] = json!(base64_encode(&bytes));
    });

    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
}

#[test]
fn a_flipped_nonce_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("nonce.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    rewrite_envelope(&destination, |envelope| {
        let mut bytes = base64_decode(envelope["nonce"].as_str().expect("nonce"));
        bytes[0] ^= 0x01;
        envelope["nonce"] = json!(base64_encode(&bytes));
    });

    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
}

#[test]
fn a_truncated_file_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("truncated.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");
    let whole = fs::read(&destination).expect("the file was read");

    // Cut in the middle: the head itself is incomplete.
    let half = files.path("half.termust-backup");
    fs::write(&half, &whole[..whole.len() / 2]).expect("the file was written");
    let error = refused(&vault, &half, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::NotABackup), "{error:?}");

    // Cut inside the ciphertext, with the head still valid.
    rewrite_envelope(&destination, |envelope| {
        let bytes = base64_decode(envelope["ciphertext"].as_str().expect("ciphertext"));
        envelope["ciphertext"] = json!(base64_encode(&bytes[..bytes.len() / 2]));
    });
    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
}

#[test]
fn a_file_that_is_not_a_backup_at_all_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();

    for (content, why) in [
        (b"not a backup".to_vec(), "plain text"),
        (b"{}".to_vec(), "an empty object"),
        (b"[1, 2, 3]".to_vec(), "a list"),
        (Vec::new(), "an empty file"),
    ] {
        let path = files.path("junk.termust-backup");
        fs::write(&path, &content).expect("the file was written");
        let error = refused(&vault, &path, BACKUP_PASSWORD);
        assert!(matches!(error, BackupError::NotABackup), "{why}: {error:?}");
        assert!(!error.to_string().contains(BACKUP_PASSWORD));
    }
}

#[test]
fn a_file_with_the_wrong_magic_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("magic.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    rewrite_envelope(&destination, |envelope| {
        envelope["magic"] = json!("SOME_OTHER_BACKUP");
    });

    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::NotABackup), "{error:?}");
}

#[test]
fn a_format_version_this_build_does_not_read_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("version.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    rewrite_envelope(&destination, |envelope| {
        envelope["formatVersion"] = json!(2);
    });

    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(
        matches!(
            error,
            BackupError::UnsupportedVersion {
                found: 2,
                supported: 1
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "This backup uses format version 2, but this build supports version 1."
    );
}

#[test]
fn an_unknown_key_derivation_or_cipher_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);

    let destination = files.path("kdf.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");
    rewrite_envelope(&destination, |envelope| {
        envelope["kdf"]["algorithm"] = json!("pbkdf2-sha256");
    });
    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::UnsupportedKdf), "{error:?}");

    let destination = files.path("cipher.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");
    rewrite_envelope(&destination, |envelope| {
        envelope["cipher"] = json!("aes-256-gcm");
    });
    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::UnsupportedCipher), "{error:?}");
}

#[test]
fn key_derivation_parameters_over_the_limit_are_refused_before_any_derivation() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("costly.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    // A file that asks for more memory than this build will ever allocate. If the
    // parameters were used first, the password check would fail and this would be
    // reported as a wrong password; it is reported as a parameter problem, which
    // means the bounds were checked before Argon2id ran.
    rewrite_envelope(&destination, |envelope| {
        envelope["kdf"]["memoryCost"] = json!(4_000_000);
    });
    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::Kdf), "{error:?}");
    assert!(
        !error.to_string().contains("password"),
        "an out-of-range file was reported as a password problem: {error}"
    );

    // Every parameter one step over the build's own guard has to be refused,
    // before the derivation that would cost real memory or time.
    for change in [
        ("memoryCost", json!(KDF_LIMITS.memory_cost + 1)),
        ("timeCost", json!(KDF_LIMITS.time_cost + 1)),
        ("parallelism", json!(KDF_LIMITS.parallelism + 1)),
        ("timeCost", json!(100_000)),
        ("parallelism", json!(1_000)),
        ("memoryCost", json!(1)),
        ("timeCost", json!(0)),
        ("parallelism", json!(0)),
        ("memoryCost", json!(0)),
    ] {
        let path = files.path("costly2.termust-backup");
        fs::copy(&destination, &path).expect("the file was copied");
        rewrite_envelope(&path, |envelope| {
            envelope["kdf"][change.0] = change.1.clone();
        });
        let error = refused(&vault, &path, BACKUP_PASSWORD);
        assert!(matches!(error, BackupError::Kdf), "{}: {error:?}", change.0);
        assert!(
            !error.to_string().contains("password"),
            "{}: an out-of-range file was reported as a password problem: {error}",
            change.0
        );
    }

    // The production cost itself is inside the guard, not outside it: the file
    // the app writes can always be read by the app.
    assert!(
        crate::vault::crypto::validate_kdf(crate::vault::crypto::PRODUCTION).is_ok(),
        "the production KDF parameters are past this build's own limit"
    );
}

/// A file of exactly this length, made sparse so the test does not write its
/// bytes to disk.
fn grown(path: &Path, bytes: u64) {
    fs::File::create(path)
        .expect("the file was created")
        .set_len(bytes)
        .expect("the file was grown");
}

/// The limit is a maximum, not a threshold: a file at the limit is read and then
/// refused for what it holds, and one byte more is refused before its contents
/// matter. Neither touches the vault.
#[test]
fn a_file_at_the_size_limit_is_read_and_one_byte_over_it_is_refused() {
    let (temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    fill(&vault);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    let at_limit = files.path("at-limit.termust-backup");
    grown(&at_limit, MAX_BACKUP_BYTES as u64);
    let error = refused(&vault, &at_limit, BACKUP_PASSWORD);
    assert!(
        matches!(error, BackupError::NotABackup),
        "a file at the limit was refused for its size: {error:?}"
    );

    let over_limit = files.path("over-limit.termust-backup");
    grown(&over_limit, MAX_BACKUP_BYTES as u64 + 1);
    let error = refused(&vault, &over_limit, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::TooLarge), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!(
            "That file is larger than the {} byte limit for a backup.",
            MAX_BACKUP_BYTES
        )
    );

    assert_eq!(
        database_state(&temp.database()),
        before,
        "a refused file changed the vault"
    );
}

/// A reader a test can count: how many bytes were really taken out of it, not how
/// many it could have handed over.
struct CountingReader<R> {
    inner: R,
    read: usize,
}

impl<R: std::io::Read> std::io::Read for CountingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.read += read;
        Ok(read)
    }
}

/// The read stops one byte past the limit, so a stream is never followed to its
/// end to find out that it is too long. An endless reader proves the stop: no
/// filesystem timing is involved.
#[test]
fn a_bounded_read_stops_one_byte_past_the_limit() {
    let mut reader = CountingReader {
        inner: std::io::repeat(b'x'),
        read: 0,
    };

    let error = service::read_bounded(&mut reader, 8).expect_err("an endless stream is too long");

    assert!(matches!(error, BackupError::TooLarge), "{error:?}");
    assert_eq!(reader.read, 9, "the read did not stop at the limit");
}

/// And the boundary itself: exactly at the limit is a read, one byte more is not.
#[test]
fn a_bounded_read_keeps_the_limit_and_refuses_one_byte_more() {
    let limit = 4;

    let kept = service::read_bounded(&b"abcd"[..], limit).expect("a stream at the limit is read");
    assert_eq!(kept, b"abcd");

    let error =
        service::read_bounded(&b"abcde"[..], limit).expect_err("a longer stream is refused");
    assert!(matches!(error, BackupError::TooLarge), "{error:?}");
}

#[test]
fn a_directory_is_not_a_backup() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    let directory = files.path("a-directory.termust-backup");
    fs::create_dir(&directory).expect("the directory was created");

    let error = refused(&vault, &directory, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::NotAFile), "{error:?}");
    assert_eq!(error.to_string(), "Choose a backup file, not a folder.");
}

// --- restore: what a request has to say ------------------------------------

#[test]
fn a_restore_needs_the_replace_mode_and_the_exact_phrase() {
    let (_temp, vault) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let source = files.path("source.termust-backup");
    service::export(&vault, &export_request(&source, BACKUP_PASSWORD)).expect("the export");
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&target);
    let before = database_state(&temp.database());

    for (mode, confirmation, why) in [
        ("merge", REPLACE_CONFIRMATION, "a mode that does not exist"),
        ("", REPLACE_CONFIRMATION, "an empty mode"),
        (
            RESTORE_MODE_REPLACE,
            "restore",
            "the phrase in the wrong case",
        ),
        (RESTORE_MODE_REPLACE, " RESTORE", "the phrase with a space"),
        (RESTORE_MODE_REPLACE, "", "no phrase at all"),
    ] {
        let request = RestoreRequest {
            mode: mode.to_string(),
            confirmation: confirmation.to_string(),
            ..restore_request(&source, BACKUP_PASSWORD)
        };
        let error = service::restore(&target, &request).expect_err(why);
        if mode == RESTORE_MODE_REPLACE {
            assert!(
                matches!(error, BackupError::ConfirmationRequired),
                "{why}: {error:?}"
            );
        } else {
            assert!(
                matches!(error, BackupError::UnsupportedMode),
                "{why}: {error:?}"
            );
        }
        assert!(!error.to_string().contains(BACKUP_PASSWORD));
    }

    assert_eq!(
        database_state(&temp.database()),
        before,
        "a refused restore changed the vault"
    );
}
#[tokio::test]
async fn a_restore_is_refused_while_a_session_is_open_and_allowed_after() {
    let (_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("session.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");
    let request = restore_request(&path, BACKUP_PASSWORD);

    let sessions = SshState::default();
    assert!(!sessions.any_session());

    // A session that is only connecting counts too: it is registered before it
    // reaches the network, and it stops the restore guard from being taken.
    sessions.record_input("session-1");
    assert!(
        sessions.begin_maintenance().is_none(),
        "a restore guard was taken while a session was open"
    );
    assert_eq!(contents(&target), "", "a refused restore wrote something");

    // Nothing disconnects the session automatically; it has to end first.
    crate::ssh::stop(&sessions, "session-1").await;
    assert!(!sessions.any_session());

    let guard = sessions
        .begin_maintenance()
        .expect("a restore is allowed again");
    service::restore(&target, &request).expect("the restore succeeded");
    drop(guard);
    assert_eq!(contents(&target), contents(&source));
}

// --- restore: what a file has to hold --------------------------------------

/// A file that a restore has to refuse, and the error it has to report.
fn refused_restore(vault: &Vault, path: &Path, password: &str, why: &str) -> BackupError {
    service::restore(vault, &restore_request(path, password)).expect_err(why)
}

#[test]
fn a_backup_with_a_record_of_an_unknown_kind_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&target);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let path = files.path("unknown-kind.termust-backup");
    let mut objects = vec![group_object("group-1")];
    objects.push(object(
        "note-1",
        "note",
        json!({ "text": "hi" }).to_string(),
        None,
    ));
    write_sealed(&path, &payload(objects), BACKUP_PASSWORD);

    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "an unknown kind");

    assert!(matches!(error, BackupError::UnknownKind), "{error:?}");
    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_two_records_of_one_id_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let path = files.path("duplicate.termust-backup");
    write_sealed(
        &path,
        &payload(vec![group_object("same"), group_object("same")]),
        BACKUP_PASSWORD,
    );

    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a duplicate id");

    assert!(matches!(error, BackupError::DuplicateId), "{error:?}");
    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_a_record_that_disagrees_with_its_own_id_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let path = files.path("mismatch.termust-backup");
    // The object says it is `outer`, the record inside says it is `inner`.
    let mut record = group_object("inner");
    record.id = "outer".to_string();
    write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);

    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a mismatched id");

    assert!(matches!(error, BackupError::InvalidRecord), "{error:?}");
    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_a_record_that_does_not_parse_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    for (record, why) in [
        (
            object("g", "group", "{not json".to_string(), None),
            "broken JSON",
        ),
        (
            object(
                "g",
                "group",
                json!({ "version": 1, "name": "Work" }).to_string(),
                None,
            ),
            "a record with no id",
        ),
        (
            object(
                "g",
                "host",
                json!({ "version": 1, "id": "g" }).to_string(),
                None,
            ),
            "a host with no address",
        ),
        (
            object("g", "group", json!({}).to_string(), None),
            "no version",
        ),
    ] {
        let path = files.path("broken.termust-backup");
        write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidRecord),
            "{why}: {error:?}"
        );
    }

    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_a_contradictory_timestamp_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    for (created_at, updated_at, deleted_at, why) in [
        (-1, 10, None, "a negative creation time"),
        (10, 9, None, "an update before creation"),
        (10, 10, Some(9), "a delete before creation"),
    ] {
        let mut record = group_object("group-1");
        record.created_at = created_at;
        record.updated_at = updated_at;
        record.deleted_at = deleted_at;
        let path = files.path("timestamps.termust-backup");
        write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidRecord),
            "{why}: {error:?}"
        );
    }

    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_an_unsupported_crypto_version_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let path = files.path("crypto-version.termust-backup");
    let mut record = group_object("group-1");
    record.crypto_version = CRYPTO_VERSION + 1;
    write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);

    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a newer crypto version");

    assert!(
        matches!(
            error,
            BackupError::UnsupportedCryptoVersion {
                found: 2,
                supported: 1
            }
        ),
        "{error:?}"
    );
    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_an_unsupported_payload_version_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    // Inside a record: a payload this build does not know.
    let path = files.path("record-version.termust-backup");
    let record = object(
        "group-1",
        "group",
        json!({ "version": 7, "id": "group-1", "name": "Future", "createdAt": 10, "updatedAt": 10 })
            .to_string(),
        None,
    );
    write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);
    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a newer record version");
    assert!(
        matches!(
            error,
            BackupError::UnsupportedPayloadVersion {
                found: 7,
                supported: 1
            }
        ),
        "{error:?}"
    );

    // The payload as a whole: a file from a format this build does not read.
    let path = files.path("payload-version.termust-backup");
    let mut later = payload(vec![group_object("group-1")]);
    later.payload_version = 2;
    write_sealed(&path, &later, BACKUP_PASSWORD);
    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a newer payload version");
    assert!(
        matches!(
            error,
            BackupError::UnsupportedPayloadVersion {
                found: 2,
                supported: 1
            }
        ),
        "{error:?}"
    );

    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_whose_host_points_at_something_it_does_not_hold_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    for (objects, why) in [
        (
            vec![host_object("host-1", Some("missing"), None)],
            "a missing group",
        ),
        (
            vec![host_object("host-1", None, Some("missing"))],
            "a missing credential",
        ),
        (
            vec![
                group_object("group-1"),
                host_object("host-1", Some("group-2"), None),
            ],
            "another group",
        ),
        (
            vec![
                credential_object("credential-1"),
                host_object("host-1", None, Some("credential-2")),
            ],
            "another credential",
        ),
    ] {
        let path = files.path("relation.termust-backup");
        write_sealed(&path, &payload(objects), BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidRelation),
            "{why}: {error:?}"
        );
        assert!(!error.to_string().contains(ADDRESS_MARKER));
    }

    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_that_points_a_host_at_a_deleted_group_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let mut deleted_group = group_object("group-1");
    deleted_group.deleted_at = Some(20);
    deleted_group.updated_at = 20;
    let path = files.path("deleted-group.termust-backup");
    write_sealed(
        &path,
        &payload(vec![
            deleted_group,
            host_object("host-1", Some("group-1"), None),
        ]),
        BACKUP_PASSWORD,
    );

    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a host on a deleted group");

    assert!(matches!(error, BackupError::InvalidRelation), "{error:?}");
    assert_eq!(database_state(&temp.database()), before);
}

#[test]
fn a_backup_with_two_live_keys_for_one_host_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    let path = files.path("two-keys.termust-backup");
    write_sealed(
        &path,
        &payload(vec![
            known_host_object("known-1", "example.com:22", None),
            known_host_object("known-2", "example.com:22", None),
        ]),
        BACKUP_PASSWORD,
    );
    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a duplicate identity");
    assert!(
        matches!(error, BackupError::DuplicateKnownHost),
        "{error:?}"
    );
    assert_eq!(database_state(&temp.database()), before);

    // The same file with one of them tombstoned is fine: a deleted record keeps
    // its identity in the backup without claiming the host.
    let path = files.path("one-key.termust-backup");
    write_sealed(
        &path,
        &payload(vec![
            known_host_object("known-1", "example.com:22", None),
            known_host_object("known-2", "example.com:22", Some(20)),
        ]),
        BACKUP_PASSWORD,
    );
    let summary = service::restore(&target, &restore_request(&path, BACKUP_PASSWORD))
        .expect("a tombstoned duplicate is allowed");
    assert_eq!(summary.replaced_objects, 2);
    assert_eq!(summary.active_objects, 1);
    assert_eq!(summary.deleted_objects, 1);
}

#[test]
fn a_backup_with_more_records_than_the_limit_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let path = files.path("too-many.termust-backup");

    let objects = (0..=MAX_OBJECTS)
        .map(|index| object(&format!("id-{index}"), "group", "{}".to_string(), None))
        .collect::<Vec<BackupObject>>();
    write_sealed(&path, &payload(objects), BACKUP_PASSWORD);

    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "too many records");

    assert!(matches!(error, BackupError::TooManyObjects), "{error:?}");
    assert_eq!(database_state(&temp.database()), before);
}

// --- restore: the operation itself -----------------------------------------

#[test]
fn a_failed_restore_leaves_the_vault_exactly_as_it_was() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&target);
    let before = database_state(&temp.database());
    let before_contents = contents(&target);
    let files = TempFiles::new();

    // Every failure mode a file can have, one after another, against one vault
    // that must not move.
    let good_path = files.path("good.termust-backup");
    service::export(&target, &export_request(&good_path, BACKUP_PASSWORD)).expect("the export");

    let broken = [
        ("wrong-password.termust-backup", {
            let path = files.path("wrong-password.termust-backup");
            fs::copy(&good_path, &path).expect("the file was copied");
            path
        }),
        ("damaged.termust-backup", {
            let path = files.path("damaged.termust-backup");
            fs::copy(&good_path, &path).expect("the file was copied");
            rewrite_envelope(&path, |envelope| {
                let mut bytes = base64_decode(envelope["ciphertext"].as_str().expect("ciphertext"));
                bytes[0] ^= 0x01;
                envelope["ciphertext"] = json!(base64_encode(&bytes));
            });
            path
        }),
    ];

    for (name, path) in &broken {
        let password = if name.starts_with("wrong") {
            "not-the-password"
        } else {
            BACKUP_PASSWORD
        };
        let error = refused_restore(&target, path, password, name);
        assert!(
            matches!(error, BackupError::CannotOpen),
            "{name}: {error:?}"
        );
    }

    let invalid_path = files.path("invalid-relation.termust-backup");
    write_sealed(
        &invalid_path,
        &payload(vec![host_object("host-1", Some("missing"), None)]),
        BACKUP_PASSWORD,
    );
    refused_restore(
        &target,
        &invalid_path,
        BACKUP_PASSWORD,
        "an invalid relation",
    );

    let duplicate_path = files.path("duplicate-id.termust-backup");
    write_sealed(
        &duplicate_path,
        &payload(vec![group_object("same"), group_object("same")]),
        BACKUP_PASSWORD,
    );
    refused_restore(&target, &duplicate_path, BACKUP_PASSWORD, "a duplicate id");

    let unknown_path = files.path("unknown-kind.termust-backup");
    write_sealed(
        &unknown_path,
        &payload(vec![object("n-1", "note", "{}".to_string(), None)]),
        BACKUP_PASSWORD,
    );
    refused_restore(&target, &unknown_path, BACKUP_PASSWORD, "an unknown kind");

    let missing_path = files.path("not-there.termust-backup");
    refused_restore(&target, &missing_path, BACKUP_PASSWORD, "a missing file");

    assert_eq!(
        database_state(&temp.database()),
        before,
        "a failed restore changed the vault"
    );
    assert_eq!(contents(&target), before_contents, "the records changed");
}

#[test]
fn a_restore_replaces_every_record_and_keeps_the_tombstone() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    let fixture = fill(&source);

    // The destination has records of its own, which the restore has to remove
    // rather than merge with.
    let stale = create_group_record(&target, "Stale").expect("the group was created");
    let stale_host = create_host_record(
        &target,
        &HostInput {
            label: "Stale box".to_string(),
            address: "stale.example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            group_id: Some(stale.id.clone()),
            credential_id: None,
        },
    )
    .expect("the host was created");

    let path = files.path("replace.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    let summary =
        service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");

    assert_eq!(summary.replaced_objects, 6);
    assert_eq!(summary.active_objects, 5);
    assert_eq!(summary.deleted_objects, 1);
    assert_eq!(summary.file_name, "replace.termust-backup");
    assert!(summary.created_at > 0);
    assert_eq!(contents(&target), contents(&source));

    // The destination's own records are gone, not merged.
    assert_eq!(
        crate::inventory::list_group_records(&target)
            .expect("the groups were listed")
            .len(),
        1
    );
    assert_eq!(
        crate::inventory::list_host_records(&target)
            .expect("the hosts were listed")
            .len(),
        1
    );
    assert!(
        crate::inventory::credential_for_host(&target, &stale_host.id).is_err(),
        "a record that was not in the backup survived the restore"
    );

    // The tombstone is a tombstone: the row is there, and the host is not in any
    // active list. Its timestamps are the ones the backup carried.
    let row = Connection::open(target_temp.database())
        .expect("the database opened")
        .query_row(
            "SELECT deleted_at FROM vault_objects WHERE id = ?1",
            rusqlite::params![fixture.deleted_host],
            |row| row.get::<_, Option<i64>>(0),
        )
        .expect("the row exists");
    assert!(row.is_some(), "the tombstone was dropped");
    assert!(
        crate::inventory::credential_for_host(&target, &fixture.deleted_host).is_err(),
        "a deleted host came back"
    );
    assert!(
        crate::inventory::list_host_records(&target)
            .expect("the hosts were listed")
            .iter()
            .all(|host| host.id != fixture.deleted_host)
    );

    // And the backup can be read back out of the restored vault unchanged.
    let again = files.path("again.termust-backup");
    service::export(&target, &export_request(&again, BACKUP_PASSWORD)).expect("the export");
    let first = service::inspect(&source, &inspect_request(&path, BACKUP_PASSWORD))
        .expect("the first inspect");
    let second = service::inspect(&target, &inspect_request(&again, BACKUP_PASSWORD))
        .expect("the second inspect");
    assert_eq!(first.count_by_kind, second.count_by_kind);
}

#[test]
fn a_restored_credential_secret_and_snippet_round_trip() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    let fixture = fill(&source);
    let path = files.path("round-trip.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");
    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");

    // The credential behind the restored host still holds the exact secret.
    let (host, credential) = crate::inventory::credential_for_host(&target, &fixture.host)
        .expect("the restored host resolves");
    assert_eq!(host.address, ADDRESS_MARKER);
    assert_eq!(host.username, USERNAME_MARKER);
    assert_eq!(host.port, 2222);
    assert_eq!(host.group_id.as_deref(), Some(fixture.group.as_str()));
    assert_eq!(
        host.credential_id.as_deref(),
        Some(fixture.credential.as_str())
    );
    assert_eq!(credential.id, fixture.credential);
    assert_eq!(
        credential.secret,
        crate::inventory::CredentialSecret::Password {
            password: Zeroizing::new(SECRET_MARKER.to_string())
        },
        "the restored secret changed"
    );

    // The known host is there under the same identity, with its fingerprint.
    let known = crate::inventory::find_known_host(&target, KNOWN_HOST_ID_MARKER)
        .expect("the known hosts were read")
        .expect("the known host was restored");
    assert_eq!(known.id, fixture.known_host);
    assert_eq!(known.fingerprint_sha256, FINGERPRINT_MARKER);

    // The snippet keeps its name and its content, byte for byte.
    let snippet =
        crate::snippets::get_snippet(&target, &fixture.snippet).expect("the snippet was restored");
    assert_eq!(snippet.name, SNIPPET_NAME_MARKER);
    assert_eq!(snippet.content, SNIPPET_CONTENT_MARKER);
}

#[test]
fn a_restored_snippet_keeps_its_line_breaks() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    let multiline = "set -e\ncd /srv/app\ngit pull --ff-only\n\nsystemctl restart app\n";
    let snippet = crate::snippets::create_snippet_record(
        &source,
        &SnippetInput {
            name: "deploy\n(two lines in the name)".to_string(),
            content: multiline.to_string(),
        },
    )
    .expect("the snippet was created");
    let path = files.path("multiline.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");
    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");

    let restored = crate::snippets::get_snippet(&target, &snippet.id).expect("the snippet");
    assert_eq!(restored.content, multiline);
    assert_eq!(restored.name, "deploy\n(two lines in the name)");
}

#[test]
fn a_restore_into_a_vault_with_another_master_password_works() {
    // The whole point of a portable backup: the source vault was created with one
    // master password, the backup with a second, and the target vault with a
    // third. None of them has to be the same.
    let (source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    let fixture = fill(&source);
    let path = files.path("portable.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    let source_ciphertexts = stored_objects(&source_temp.database(), "snippet");
    let target_meta = {
        let connection = Connection::open(target_temp.database()).expect("the database opened");
        connection
            .query_row(
                "SELECT hex(wrapped_vault_key), hex(kdf_salt), kdf_memory_cost, kdf_time_cost, kdf_parallelism FROM vault_meta",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, u32>(4)?,
                    ))
                },
            )
            .expect("the metadata row")
    };

    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");

    // The target vault is still the target vault: its own key, its own salt and
    // its own parameters, none of them changed by the restore.
    let after_meta = {
        let connection = Connection::open(target_temp.database()).expect("the database opened");
        connection
            .query_row(
                "SELECT hex(wrapped_vault_key), hex(kdf_salt), kdf_memory_cost, kdf_time_cost, kdf_parallelism FROM vault_meta",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, u32>(4)?,
                    ))
                },
            )
            .expect("the metadata row")
    };
    assert_eq!(
        after_meta, target_meta,
        "the restore rewrote vault metadata"
    );

    // The records were re-encrypted with the target's key: the same snippet has
    // different ciphertext in the two databases.
    let target_ciphertexts = stored_objects(&target_temp.database(), "snippet");
    assert_eq!(target_ciphertexts.len(), 1);
    assert_eq!(source_ciphertexts.len(), 1);
    assert_eq!(target_ciphertexts[0].0, fixture.snippet);
    assert_ne!(
        source_ciphertexts[0].1, target_ciphertexts[0].1,
        "the restored record still holds the source vault's ciphertext"
    );

    // The target keeps its master password, and neither the source password nor
    // the backup password opens it.
    let mut reopened = target_temp.vault();
    assert!(
        reopened.unlock(SOURCE_PASSWORD).is_err(),
        "the source password opened the target"
    );
    assert!(
        reopened.unlock(BACKUP_PASSWORD).is_err(),
        "the backup password opened a vault"
    );
    reopened
        .unlock(DESTINATION_PASSWORD)
        .expect("the target password was changed by the restore");

    // And everything is where it was left, read back through a fresh handle: the
    // restored host, its credential, the known host and the snippet.
    let (host, credential) = crate::inventory::credential_for_host(&reopened, &fixture.host)
        .expect("the restored host resolves after a lock and unlock");
    assert_eq!(host.label, "Prod web");
    assert_eq!(
        credential.secret,
        crate::inventory::CredentialSecret::Password {
            password: Zeroizing::new(SECRET_MARKER.to_string())
        }
    );
    assert!(
        crate::inventory::find_known_host(&reopened, KNOWN_HOST_ID_MARKER)
            .expect("the known hosts were read")
            .is_some()
    );
    assert_eq!(
        crate::snippets::get_snippet(&reopened, &fixture.snippet)
            .expect("the snippet")
            .content,
        SNIPPET_CONTENT_MARKER
    );

    // The source vault was not touched by the export.
    assert_eq!(
        stored_objects(&source_temp.database(), "snippet"),
        source_ciphertexts
    );
}

#[test]
fn a_backup_of_one_vault_restores_into_a_vault_that_was_never_touched() {
    let (source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("empty-target.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");

    assert_eq!(contents(&target), contents(&source));
    // One metadata row and the same number of records as the source vault: the
    // ciphertext itself differs, because it was encrypted again.
    assert_eq!(
        database_state(&target_temp.database()).len(),
        database_state(&source_temp.database()).len()
    );
}

#[test]
fn a_replace_that_fails_halfway_rolls_the_whole_table_back() {
    let (temp, vault) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&vault);
    let before = database_state(&temp.database());

    // The file path never reaches this: every duplicate id is caught before the
    // transaction. This drives the storage primitive directly, so the rollback
    // itself is proved rather than assumed: the second insert collides with the
    // first and the table has to come back whole.
    let snapshot = |id: &str, name: &str| SnapshotObject {
        id: id.to_string(),
        kind: "group".to_string(),
        payload: Zeroizing::new(
            json!({ "version": 1, "id": id, "name": name, "createdAt": 1, "updatedAt": 1 })
                .to_string(),
        ),
        crypto_version: CRYPTO_VERSION,
        created_at: 1,
        updated_at: 1,
        deleted_at: None,
    };
    let error = vault
        .replace_objects(&[snapshot("same", "First"), snapshot("same", "Second")])
        .expect_err("a duplicate primary key rolls the transaction back");

    assert!(matches!(error, VaultError::Storage(_)), "{error:?}");
    assert_eq!(
        database_state(&temp.database()),
        before,
        "a failed replace left the table half-written"
    );
}

#[test]
fn a_restore_leaves_no_record_of_the_replaced_vault() {
    // The snapshot a restore writes goes through the vault's own swap, never
    // through the per-record CRUD: `replace_objects` deletes every row and writes
    // the whole snapshot inside one transaction, so a record that fails cannot
    // leave the vault half-replaced. This checks the shape of that operation by
    // restoring into a vault that already holds more records than the backup.
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    for index in 0..20 {
        create_group_record(&target, &format!("Group {index}")).expect("a group was created");
    }
    let path = files.path("fewer.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");

    assert_eq!(
        crate::inventory::list_group_records(&target)
            .expect("the groups were listed")
            .len(),
        1
    );
    let rows: i64 = Connection::open(target_temp.database())
        .expect("the database opened")
        .query_row("SELECT count(*) FROM vault_objects", [], |row| row.get(0))
        .expect("the count");
    assert_eq!(rows, 6);
}

// --- restore: the domain rules a backup has to satisfy ----------------------

/// The record fields a test wants to control: a label, a port and so on. Each
/// is the JSON a real record would hold, so the only thing wrong with it is the
/// one value under test.
fn group_like(id: &str, name: &str) -> BackupObject {
    object(
        id,
        "group",
        json!({ "version": 1, "id": id, "name": name, "createdAt": 10, "updatedAt": 10 })
            .to_string(),
        None,
    )
}

fn host_like(id: &str, label: &str, address: &str, port: u16, username: &str) -> BackupObject {
    object(
        id,
        "host",
        json!({
            "version": 1,
            "id": id,
            "label": label,
            "address": address,
            "port": port,
            "username": username,
            "groupId": null,
            "credentialId": null,
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        None,
    )
}

/// A credential secret as the record really serialises it, so the field names
/// a test writes can never drift from the ones the app reads.
fn secret_json(secret: CredentialSecret) -> serde_json::Value {
    serde_json::to_value(secret).expect("the secret serialises")
}

fn credential_like(id: &str, name: &str, secret: serde_json::Value) -> BackupObject {
    object(
        id,
        "credential",
        json!({
            "version": 1,
            "id": id,
            "name": name,
            "secret": secret,
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        None,
    )
}

fn snippet_like(id: &str, name: &str, content: &str) -> BackupObject {
    object(
        id,
        "snippet",
        json!({
            "version": 1,
            "id": id,
            "name": name,
            "content": content,
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        None,
    )
}

fn known_host_like(id: &str, identity: &str, algorithm: &str, fingerprint: &str) -> BackupObject {
    object(
        id,
        "known_host",
        json!({
            "version": 1,
            "id": id,
            "hostKeyId": identity,
            "keyAlgorithm": algorithm,
            "fingerprintSha256": fingerprint,
            "createdAt": 10,
            "updatedAt": 10,
        })
        .to_string(),
        None,
    )
}

/// A backup whose records parse but break the field rules the commands enforce
/// has to be refused before the vault changes, exactly like a damaged one.
#[test]
fn a_backup_with_a_record_that_breaks_the_domain_rules_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&target);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    let over_name = "a".repeat(101);
    let over_address = "a".repeat(256);
    let over_secret = "s".repeat(16 * 1024 + 1);
    let over_content = "c".repeat(64 * 1024 + 1);
    let nul_content = "echo hi\0there".to_string();

    let cases: Vec<(BackupObject, &str)> = vec![
        (group_like("group-1", ""), "an empty group name"),
        (
            group_like("group-1", &over_name),
            "a group name over the limit",
        ),
        (group_like("group-1", " Work"), "an untrimmed group name"),
        (group_like("group-1", "   "), "a blank group name"),
        (
            host_like("host-1", "", ADDRESS_MARKER, 22, USERNAME_MARKER),
            "an empty host label",
        ),
        (
            host_like("host-1", "Prod", "", 22, USERNAME_MARKER),
            "an empty address",
        ),
        (
            host_like("host-1", "Prod", &over_address, 22, USERNAME_MARKER),
            "an address over the limit",
        ),
        (
            host_like("host-1", "Prod", ADDRESS_MARKER, 0, USERNAME_MARKER),
            "port zero",
        ),
        (
            host_like("host-1", "Prod", ADDRESS_MARKER, 22, " Deploy"),
            "an untrimmed username",
        ),
        (
            credential_like(
                "credential-1",
                "Prod",
                secret_json(CredentialSecret::Password {
                    password: Zeroizing::new(String::new()),
                }),
            ),
            "an empty credential secret",
        ),
        (
            credential_like(
                "credential-1",
                "Prod",
                secret_json(CredentialSecret::Password {
                    password: Zeroizing::new(over_secret),
                }),
            ),
            "a credential secret over the limit",
        ),
        (
            credential_like(
                "credential-1",
                "Prod",
                secret_json(CredentialSecret::PrivateKey {
                    private_key: Zeroizing::new("-----BEGIN OPENSSH PRIVATE KEY-----".to_string()),
                    passphrase: Some(Zeroizing::new(String::new())),
                }),
            ),
            "an empty passphrase",
        ),
        (
            snippet_like("snippet-1", "Deploy", ""),
            "empty snippet content",
        ),
        (
            snippet_like("snippet-1", "Deploy", &over_content),
            "snippet content over the 64 KiB limit",
        ),
        (
            snippet_like("snippet-1", "Deploy", &nul_content),
            "a NUL byte in snippet content",
        ),
        (
            snippet_like("snippet-1", "  Deploy", SNIPPET_CONTENT_MARKER),
            "an untrimmed snippet name",
        ),
        (
            known_host_like("known-1", "example.com:22", "ssh-ed25519", ""),
            "an empty fingerprint",
        ),
        (
            known_host_like(
                "known-1",
                " example.com:22",
                "ssh-ed25519",
                FINGERPRINT_MARKER,
            ),
            "an untrimmed host identity",
        ),
    ];

    for (record, why) in cases {
        let path = files.path("domain-invalid.termust-backup");
        write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidRecord),
            "{why}: {error:?}"
        );
        assert!(
            !error.to_string().contains(SECRET_MARKER),
            "{why}: the error repeated a secret"
        );
    }

    assert_eq!(
        database_state(&temp.database()),
        before,
        "a domain-invalid record was restored"
    );
}

/// A host may point at a group; that relation has to be the exact form the
/// commands store, so an empty string is not "no relation".
#[test]
fn a_backup_with_an_empty_relation_id_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    for (field, why) in [
        ("groupId", "an empty group id"),
        ("credentialId", "an empty credential id"),
    ] {
        let mut record = host_like("host-1", "Prod", ADDRESS_MARKER, 22, USERNAME_MARKER);
        let mut payload_json: serde_json::Value =
            serde_json::from_str(&record.payload).expect("the record parses");
        payload_json[field] = json!("");
        record.payload = Zeroizing::new(payload_json.to_string());
        let path = files.path("empty-relation.termust-backup");
        write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidRecord),
            "{why}: {error:?}"
        );
    }

    assert_eq!(database_state(&temp.database()), before);
}

/// Two live groups whose names differ only in case are impossible in the vault
/// the commands write, so a file that holds them is refused — while a deleted
/// group may keep the same name, because only live groups have to be unique.
#[test]
fn a_backup_with_two_live_groups_of_one_name_in_different_case_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    let path = files.path("group-names.termust-backup");
    write_sealed(
        &path,
        &payload(vec![
            group_like("group-1", "Work"),
            group_like("group-2", "WORK"),
        ]),
        BACKUP_PASSWORD,
    );
    let error = refused_restore(&target, &path, BACKUP_PASSWORD, "a duplicate group name");
    assert!(
        matches!(error, BackupError::DuplicateGroupName),
        "{error:?}"
    );
    assert_eq!(database_state(&temp.database()), before);

    let mut deleted = group_like("group-2", "WORK");
    deleted.deleted_at = Some(20);
    deleted.updated_at = 20;
    let path = files.path("group-names-tombstone.termust-backup");
    write_sealed(
        &path,
        &payload(vec![group_like("group-1", "Work"), deleted]),
        BACKUP_PASSWORD,
    );
    let summary = service::restore(&target, &restore_request(&path, BACKUP_PASSWORD))
        .expect("a tombstoned duplicate is allowed");
    assert_eq!(summary.replaced_objects, 2);
    assert_eq!(summary.active_objects, 1);
}

/// The payload's own metadata is checked too, and it has an explicit size
/// limit rather than whatever a hostile file feels like writing.
#[test]
fn a_backup_with_invalid_payload_metadata_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    for (change, why) in [
        (
            (|payload: &mut BackupPayload| payload.created_at = -1) as fn(&mut BackupPayload),
            "a negative creation time",
        ),
        (
            (|payload: &mut BackupPayload| payload.source_app_version = String::new())
                as fn(&mut BackupPayload),
            "no source version",
        ),
        (
            (|payload: &mut BackupPayload| payload.source_app_version = "v".repeat(65))
                as fn(&mut BackupPayload),
            "an oversized source version",
        ),
        (
            (|payload: &mut BackupPayload| payload.source_app_version = " 0.1.0".to_string())
                as fn(&mut BackupPayload),
            "an untrimmed source version",
        ),
    ] {
        let mut later = payload(vec![group_like("group-1", "Work")]);
        change(&mut later);
        let path = files.path("metadata.termust-backup");
        write_sealed(&path, &later, BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidPayload),
            "{why}: {error:?}"
        );
    }

    assert_eq!(database_state(&temp.database()), before);
}

/// The record inside an object carries its own timestamps, and they have to be
/// real and to agree with the row that holds it. A record that predates the
/// epoch, one that was updated before it was created, and one that claims a
/// newer history than its own metadata are all impossible to write through the
/// commands, so a backup cannot introduce them either.
#[test]
fn a_backup_whose_record_timestamps_do_not_match_its_object_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();

    for (inner_created, inner_updated, outer_created, outer_updated, why) in [
        (-1, 10, 10, 20, "a record created before the epoch"),
        (10, -1, 10, 20, "a record updated before the epoch"),
        (20, 10, 30, 30, "a record updated before it was created"),
        (20, 20, 10, 20, "a record created after its row"),
        (10, 30, 10, 20, "a record updated after its row"),
        (11, 11, 10, 10, "a record newer than its row on both ends"),
    ] {
        let mut record = group_like("group-1", "Work");
        let mut record_json: serde_json::Value =
            serde_json::from_str(&record.payload).expect("the record parses");
        record_json["createdAt"] = json!(inner_created);
        record_json["updatedAt"] = json!(inner_updated);
        record.payload = Zeroizing::new(record_json.to_string());
        record.created_at = outer_created;
        record.updated_at = outer_updated;
        let path = files.path("inner-timestamps.termust-backup");
        write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);
        let error = refused_restore(&target, &path, BACKUP_PASSWORD, why);
        assert!(
            matches!(error, BackupError::InvalidRecord),
            "{why}: {error:?}"
        );
    }

    assert_eq!(
        database_state(&temp.database()),
        before,
        "a record with mismatched timestamps was restored"
    );

    // Older inner timestamps are what a tombstone really looks like: the row
    // moved forward when the record was deleted, and the payload did not. Both
    // ends of the record are older than the row that carries it, which is the
    // case the validator has to keep accepting.
    let mut deleted = group_like("group-1", "Work");
    deleted.deleted_at = Some(30);
    deleted.updated_at = 30;
    let path = files.path("tombstoned-timestamps.termust-backup");
    write_sealed(&path, &payload(vec![deleted]), BACKUP_PASSWORD);
    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD))
        .expect("a tombstone with older inner timestamps is fine");

    let row: (i64, i64, Option<i64>) = Connection::open(temp.database())
        .expect("the database opened")
        .query_row(
            "SELECT created_at, updated_at, deleted_at FROM vault_objects WHERE id = ?1",
            ["group-1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("the tombstoned row was read");
    assert_eq!(
        row,
        (10, 30, Some(30)),
        "the row did not keep the timestamps the backup carried"
    );
}

/// A tombstone cannot be older than the update that preceded it: the deletion
/// timestamp is real and follows the record's own history.
#[test]
fn a_backup_with_a_tombstone_older_than_its_last_update_is_refused() {
    let (temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let before = database_state(&temp.database());
    let files = TempFiles::new();
    let mut record = group_like("group-1", "Work");
    record.updated_at = 10;
    record.deleted_at = Some(9);
    let path = files.path("tombstone-order.termust-backup");
    write_sealed(&path, &payload(vec![record]), BACKUP_PASSWORD);

    let error = refused_restore(
        &target,
        &path,
        BACKUP_PASSWORD,
        "a tombstone before the update",
    );

    assert!(matches!(error, BackupError::InvalidRecord), "{error:?}");
    assert_eq!(database_state(&temp.database()), before);
}

// --- the backup format's own identity --------------------------------------

/// The AAD names the format version explicitly, so this pins the two together:
/// a later format that changes one has to change the other too.
#[test]
fn the_backup_aad_names_the_format_version() {
    let expected = format!("termust:backup:v{FORMAT_VERSION}:payload");
    assert_eq!(model::BACKUP_AAD, expected.as_bytes());
}

/// A ciphertext bound to another format version cannot be opened as v1, even
/// with the right password, the right KDF parameters and the same nonce.
#[test]
fn a_payload_sealed_for_another_format_version_is_refused() {
    let (_temp, vault) = unlocked_vault(SOURCE_PASSWORD);
    let files = TempFiles::new();
    fill(&vault);
    let destination = files.path("aad.termust-backup");
    service::export(&vault, &export_request(&destination, BACKUP_PASSWORD)).expect("the export");

    // Re-seal the same plaintext under a v2 AAD, leaving everything else — the
    // password, the salt, the nonce, the format version in the head — alone.
    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&destination).expect("the file was read"))
            .expect("the head parsed");
    let salt = base64_decode(envelope["kdf"]["salt"].as_str().expect("a salt"));
    let nonce = base64_decode(envelope["nonce"].as_str().expect("a nonce"));
    let ciphertext = base64_decode(envelope["ciphertext"].as_str().expect("ciphertext"));
    let key = derive_kek(BACKUP_PASSWORD, &salt, TEST).expect("the key derives");
    let plaintext = open_backup(&key, &nonce, &ciphertext, BACKUP_AAD).expect("v1 opens it");
    let resealed = seal_backup(&key, &nonce, &plaintext, b"termust:backup:v2:payload")
        .expect("the payload was sealed again");
    rewrite_envelope(&destination, |envelope| {
        envelope["ciphertext"] = json!(base64_encode(&resealed));
    });

    let error = refused(&vault, &destination, BACKUP_PASSWORD);
    assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
    assert!(
        !error.to_string().contains(BACKUP_PASSWORD),
        "the error repeated the password"
    );
}

// --- restore: the file the preview named -----------------------------------

/// A file that changed after the preview is refused, even when the change keeps
/// the file readable: without the digest a restore would happily use the new
/// bytes, which are not what the user read.
#[test]
fn a_file_that_changed_after_the_preview_is_refused() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    fill(&target);
    let before = database_state(&target_temp.database());
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("changed.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    let previewed = service::inspect(&source, &inspect_request(&path, BACKUP_PASSWORD))
        .expect("the preview succeeded");
    let request = restore_request(&path, BACKUP_PASSWORD);
    assert_eq!(
        request.file_digest, previewed.file_digest,
        "the preview digest is not the one a restore sends"
    );

    // The same file, one readable byte longer: the JSON still parses, so only
    // the digest can tell that it is not the file the user previewed.
    {
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("the file was opened");
        file.write_all(b"\n").expect("the file was changed");
    }

    let error = service::restore(&target, &request).expect_err("a changed file is refused");
    assert!(matches!(error, BackupError::FileChanged), "{error:?}");
    assert_eq!(
        database_state(&target_temp.database()),
        before,
        "a changed file was restored"
    );

    // Reading the file again is enough: the restore then carries the digest of
    // what is really there.
    let again = restore_request(&path, BACKUP_PASSWORD);
    assert_ne!(again.file_digest, request.file_digest);
    service::restore(&target, &again).expect("the restore succeeds after a fresh read");
    assert_eq!(contents(&target), contents(&source));
}

#[test]
fn a_restore_that_carries_no_previewed_digest_is_refused() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("no-digest.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");
    let request = RestoreRequest {
        file_digest: String::new(),
        ..restore_request(&path, BACKUP_PASSWORD)
    };

    let error = service::restore(&target, &request).expect_err("a restore needs a digest");

    assert!(matches!(error, BackupError::FileChanged), "{error:?}");
    assert_eq!(contents(&target), "", "a refused restore wrote something");
}

// --- the restore guard -----------------------------------------------------

/// A session that is already registered stops the guard from being taken, in
/// the same critical section that a later one would be refused in.
#[test]
fn a_restore_guard_cannot_be_taken_while_a_session_is_open() {
    let sessions = SshState::default();
    sessions.record_input("session-1");

    assert!(
        sessions.begin_maintenance().is_none(),
        "the guard was taken while a session was open"
    );
}

/// The other direction, with no sleeps: while one thread holds the guard, a
/// session cannot register; once it drops the guard, a session registers again.
/// The two threads hand the guard over through channels, so the test proves the
/// lock — not the timing — is what keeps them apart.
#[test]
fn a_restore_guard_and_a_new_session_are_mutually_exclusive_across_threads() {
    use std::sync::Arc as Shared;
    use std::sync::mpsc;

    let sessions = Shared::new(SshState::default());
    let sessions_for_thread = Shared::clone(&sessions);
    let (taken_tx, taken_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let guard = sessions_for_thread
            .begin_maintenance()
            .expect("nothing is connected, so the guard is free");
        taken_tx.send(()).expect("the main thread is waiting");
        release_rx.recv().expect("the main thread is waiting");
        drop(guard);
    });

    taken_rx.recv().expect("the guard was taken");
    assert!(
        sessions.try_record_input("session-2").is_err(),
        "a session registered while a restore held the guard"
    );
    assert!(
        sessions.begin_maintenance().is_none(),
        "a second restore guard was taken"
    );

    release_tx.send(()).expect("the worker is waiting");
    worker.join().expect("the worker finished");

    // The guard is gone: another one can be taken, and once that one is dropped
    // a session registers again.
    drop(
        sessions
            .begin_maintenance()
            .expect("the guard is free again once the restore is over"),
    );
    sessions
        .try_record_input("session-2")
        .expect("a session can start again after the restore");
}

#[test]
fn the_restore_guard_is_released_after_a_successful_restore() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("guard-success.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    let sessions = SshState::default();
    let guard = sessions.begin_maintenance().expect("no session is open");
    service::restore(&target, &restore_request(&path, BACKUP_PASSWORD)).expect("the restore");
    drop(guard);

    drop(
        sessions
            .begin_maintenance()
            .expect("the guard was not released"),
    );
    sessions
        .try_record_input("after-restore")
        .expect("a session can start after a successful restore");
}

#[test]
fn the_restore_guard_is_released_after_a_failed_restore() {
    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("guard-failure.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");

    let sessions = SshState::default();
    let guard = sessions.begin_maintenance().expect("no session is open");
    let error = service::restore(&target, &restore_request(&path, "not-the-password"))
        .expect_err("the wrong password fails the restore");
    assert!(matches!(error, BackupError::CannotOpen), "{error:?}");
    drop(guard);

    drop(
        sessions
            .begin_maintenance()
            .expect("the guard was not released"),
    );
    sessions
        .try_record_input("after-failure")
        .expect("a session can start after a failed restore");
}

/// Dropping the guard during unwinding is the same `Drop`, so a panic in the
/// restore cannot leave the app unable to open a session ever again.
#[test]
fn the_restore_guard_is_released_when_the_work_panics() {
    let sessions = SshState::default();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = sessions
            .begin_maintenance()
            .expect("nothing is connected, so the guard is free");
        panic!("the restore blew up");
    }));

    assert!(panicked.is_err(), "the panic did not unwind");
    sessions
        .try_record_input("after-panic")
        .expect("the guard survived a panic");
}

/// Waits for a step the test itself is driving, giving up instead of hanging: a
/// step that never happens fails the test with a message. The condition always
/// means "the work this test started has reached a state it can observe", so no
/// step is timed against another thread with a sleep.
fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !condition() {
        assert!(
            std::time::Instant::now() < until,
            "timed out waiting for {what}"
        );
        std::thread::yield_now();
    }
}

/// The guard has to belong to the blocking work, not to the command's future.
///
/// The command is polled once — enough to take the guard and start the blocking
/// restore — and then dropped, which is what a cancelled command does. The
/// blocking task it started keeps running, and this test holds the vault lock to
/// keep it still, so nothing about the outcome depends on how fast a thread is.
///
/// A session has to stay refused for as long as that work is unfinished, and the
/// work really is unfinished when the refusal happens: the vault rows it writes
/// only appear once it is let through. Once it has run, a session is accepted
/// again.
#[test]
fn a_cancelled_restore_command_holds_the_guard_until_the_blocking_work_ends() {
    use std::sync::mpsc;
    use std::task::{Context, Poll, Waker};

    use tauri::Manager as _;
    use tauri::test::{mock_builder, mock_context, noop_assets};

    let (_source_temp, source) = unlocked_vault(SOURCE_PASSWORD);
    let (_target_temp, target) = unlocked_vault(DESTINATION_PASSWORD);
    let files = TempFiles::new();
    fill(&source);
    let path = files.path("cancelled.termust-backup");
    service::export(&source, &export_request(&path, BACKUP_PASSWORD)).expect("the export");
    let request = restore_request(&path, BACKUP_PASSWORD);
    let expected = contents(&source);

    let state = VaultState::new(target);
    // A handle on the same vault: how the test holds the blocking work still and
    // how it checks that the work did run in the end.
    let vault = state.handle();
    let app = mock_builder()
        .manage(state)
        .manage(SshState::default())
        .build(mock_context(noop_assets()))
        .expect("the app built");

    // The restore takes the vault lock, so holding it here stalls the blocking
    // work after its guard has been taken.
    let (holding_tx, holding_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let held = vault.clone();
    let holder = std::thread::spawn(move || {
        held.with(|_| {
            holding_tx.send(()).expect("the main thread is waiting");
            release_rx.recv().expect("the main thread is waiting");
        });
    });
    holding_rx.recv().expect("the vault lock was taken");

    let mut command = Box::pin(super::restore_encrypted_backup(
        app.handle().clone(),
        app.state::<SshState>(),
        request,
    ));
    assert!(
        matches!(
            command
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ),
        "the restore finished while the vault was locked"
    );

    // Cancelled: the future is gone, and the work it started is not.
    drop(command);

    let error = app
        .state::<SshState>()
        .try_record_input("while-the-work-is-stalled")
        .map(|_| ())
        .expect_err("a session started while the cancelled restore was still running");
    assert!(
        error.contains("restore is in progress"),
        "the refusal did not name the restore: {error}"
    );

    // Let the work through. Reading the restored rows needs the vault lock, which
    // the blocking work holds until it has returned — and it returns after the
    // guard is dropped — so once they are there, that work is really over.
    release_tx.send(()).expect("the holder is waiting");
    holder.join().expect("the holder finished");
    wait_until("the restore to write the restored rows", || {
        vault.with(contents) == expected
    });

    app.state::<SshState>()
        .try_record_input("after-the-work-ended")
        .expect("a session can start again once the restore is really over");
}

// --- the atomic file write -------------------------------------------------

/// The overwrite policy is decided by the publish step, not by a look at the
/// destination before it: a file that is already there when the publish runs is
/// never replaced, which is exactly the race a check-then-rename would lose.
#[test]
fn the_publish_step_never_replaces_a_file_it_was_not_told_to() {
    let files = TempFiles::new();
    let destination = files.path("arrived-first.termust-backup");
    let existing = b"a file that was already here";
    fs::write(&destination, existing).expect("the file was written");

    let error = service::write_atomically(&destination, b"a whole backup", false)
        .expect_err("an existing file stops a no-clobber write");

    assert!(matches!(error, BackupError::DestinationExists), "{error:?}");
    assert_eq!(
        fs::read(&destination).expect("the file was read"),
        existing,
        "the existing file was replaced"
    );
    assert_eq!(
        files.names(),
        vec!["arrived-first.termust-backup".to_string()],
        "a temporary file was left behind"
    );
}

/// Many writers race for one destination, and the filesystem picks exactly one
/// winner: no reader can ever see a half-written file, and no lost write can
/// happen without an error.
#[test]
fn only_one_of_many_no_clobber_writes_can_win() {
    use std::sync::Barrier;

    let files = TempFiles::new();
    let destination = files.path("race.termust-backup");
    let writers = 8;
    let barrier = Barrier::new(writers);

    let outcomes: Vec<Result<(), BackupError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..writers)
            .map(|index| {
                let barrier = &barrier;
                let destination = destination.as_path();
                scope.spawn(move || {
                    // Every writer reaches the publish step at the same moment.
                    barrier.wait();
                    let bytes = format!("writer-{index}").into_bytes();
                    service::write_atomically(destination, &bytes, false)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a writer finished"))
            .collect()
    });

    let winners = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    assert_eq!(
        winners, 1,
        "the no-clobber move let more than one writer win"
    );
    for outcome in &outcomes {
        if let Err(error) = outcome {
            assert!(
                matches!(error, BackupError::DestinationExists),
                "a losing writer reported {error:?}"
            );
        }
    }

    // The destination holds one whole writer's bytes, and nothing else is left.
    let written = fs::read(&destination).expect("the destination exists");
    let text = String::from_utf8(written).expect("the bytes are a writer's");
    assert!(
        (0..writers).any(|index| text == format!("writer-{index}")),
        "the destination holds {text:?}"
    );
    assert_eq!(
        files.names(),
        vec!["race.termust-backup".to_string()],
        "a temporary file was left behind"
    );
}

/// Overwriting is the one path that must really replace an existing file, and
/// it must do it as a whole file: the destination is never a mix of the old and
/// the new bytes.
#[test]
fn a_publish_that_may_overwrite_replaces_the_whole_file() {
    let files = TempFiles::new();
    let destination = files.path("replaced.termust-backup");
    fs::write(&destination, b"the old backup").expect("the file was written");

    service::write_atomically(&destination, b"the new backup", true).expect("the write");

    assert_eq!(
        fs::read(&destination).expect("the file was read"),
        b"the new backup"
    );
    assert_eq!(
        files.names(),
        vec!["replaced.termust-backup".to_string()],
        "a temporary file was left behind"
    );
}

// --- the module itself -----------------------------------------------------

/// Backups read the vault, read and write one file, and know about an open
/// session. The modules' own source is pinned here, so a backup path that grows a
/// process, a socket or another network client fails a test instead of a review.
#[test]
fn nothing_here_runs_a_process_or_opens_a_socket() {
    let sources = [
        include_str!("mod.rs"),
        include_str!("model.rs"),
        include_str!("format.rs"),
        include_str!("service.rs"),
    ];
    let forbidden = [
        "std::process",
        "Command::new",
        "TcpStream",
        "TcpListener",
        "std::net",
        "russh",
        "reqwest",
        "Url::parse",
    ];

    for source in sources {
        // Only the part of the file before its tests is the code that runs.
        let code = source
            .split("#[cfg(test)]")
            .next()
            .expect("every source has a first part");
        for needle in forbidden {
            assert!(!code.contains(needle), "a backup must not use {needle}");
        }
    }
}

// --- small helpers ---------------------------------------------------------

fn base64_decode(value: &str) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .expect("the value is base64")
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
