//! What a portable backup is, on disk and over IPC.
//!
//! A backup file is one JSON object with a small plaintext head — enough to
//! recognise the file and to derive the key that opens it — and one base64
//! ciphertext holding everything else. Nothing about the user's data is in the
//! head: not a count, not a name, not an address, not an id. That is the whole
//! point of the split, and it is why the counts the UI shows come from the
//! decrypted half and never from the file's size or its head.
//!
//! The types here are also the wire contract with the frontend, so every request
//! refuses unknown fields and no response ever carries a secret.
//!
//! - `format.rs` turns a payload into those bytes and back.
//! - `service.rs` reads the vault, inspects a file and restores one.
//! - `mod.rs` holds the Tauri commands and the error mapping.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// The first field of every backup file. It is plaintext, and it is checked
/// before anything is derived or decrypted, so a file that is not a backup is
/// refused without doing any work on it.
pub const MAGIC: &str = "TERMUST_BACKUP";

/// The format version this build writes and the only one it reads.
pub const FORMAT_VERSION: u8 = 1;

/// The key derivation a backup envelope may name.
pub const BACKUP_KDF: &str = "argon2id";

/// The authenticated cipher a backup envelope may name.
pub const BACKUP_CIPHER: &str = "xchacha20-poly1305";

/// The version of the encrypted payload.
pub const PAYLOAD_VERSION: u8 = 1;

/// The Additional Authenticated Data of a v1 backup payload.
///
/// This is the backup format's own identity, and it names the format version on
/// purpose: the crypto version that `vault::crypto` builds its other AADs from
/// describes how a vault object is encrypted, not which file format is being
/// written. Binding the format version here means a v1 file can never be opened
/// by a later format that expects different bytes, a v1 payload can never be
/// read as a vault object, and the other way around. The literal version has to
/// match [`FORMAT_VERSION`], which a test pins.
pub const BACKUP_AAD: &[u8] = b"termust:backup:v1:payload";

/// The longest `sourceAppVersion` a backup may carry.
///
/// The value is informational — a human-readable label the preview shows — and
/// it is validated when a file is read, so a damaged or hostile file cannot put
/// an unbounded string on that path.
pub const MAX_SOURCE_APP_VERSION_LEN: usize = 64;

/// The largest backup file this build will read or write, in bytes.
///
/// The vault is encrypted text, so a backup of a very large vault is still small;
/// the limit exists so a hostile file cannot make the app allocate without bound.
pub const MAX_BACKUP_BYTES: usize = 64 * 1024 * 1024;

/// The largest decrypted payload this build accepts.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

/// The largest number of records a snapshot may hold.
pub const MAX_OBJECTS: usize = 50_000;

/// The shortest backup password this build accepts.
pub const MIN_PASSWORD_LEN: usize = 12;

/// The only restore mode this build has.
pub const RESTORE_MODE_REPLACE: &str = "replace";

/// The exact phrase a replace has to be sent with.
///
/// The frontend asks the user to type it, and the backend checks it too: a
/// disabled button is a convenience, never the guard.
pub const REPLACE_CONFIRMATION: &str = "RESTORE";

/// The plaintext head of a backup file.
///
/// `deny_unknown_fields` is deliberate: a file with a field this build does not
/// know is refused as malformed rather than read halfway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupEnvelope {
    pub magic: String,
    pub format_version: u8,
    pub kdf: EnvelopeKdf,
    pub cipher: String,
    /// Base64, because a backup has to be one text file that can be moved by
    /// hand. The nonce is bound to the ciphertext by the authentication tag.
    pub nonce: String,
    /// Base64 ciphertext of the payload below.
    pub ciphertext: String,
}

/// The key derivation a backup file names, in the parameters the file carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvelopeKdf {
    pub algorithm: String,
    /// Base64 random salt. A fresh one is drawn for every export, so the same
    /// vault exported twice never produces the same key or the same bytes.
    pub salt: String,
    pub memory_cost: u32,
    pub time_cost: u32,
    pub parallelism: u32,
}

/// The encrypted half of a backup: the whole snapshot, as it was when the file
/// was written.
///
/// Everything the user could learn from is here — the timestamps, the app that
/// wrote it, the records themselves. It travels only as ciphertext.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupPayload {
    pub payload_version: u8,
    pub created_at: i64,
    pub source_app_version: String,
    pub objects: Vec<BackupObject>,
}

/// One record inside a backup, exactly as the vault holds it.
///
/// `payload` is the domain record — a group, host, credential, known host or
/// snippet — as JSON. It is re-encrypted when it is restored, so a backup makes
/// no assumption about the key of the vault it will land in. A credential's
/// record holds its secret, so this field wipes its bytes when it is dropped and
/// its `Debug` never prints them.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupObject {
    pub id: String,
    pub kind: String,
    pub payload: Zeroizing<String>,
    pub crypto_version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// Set on a tombstone; absent on a live record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
}

/// Redacts the record body: a decoded credential holds its secret.
impl std::fmt::Debug for BackupObject {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BackupObject")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("payload", &"***")
            .field("crypto_version", &self.crypto_version)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("deleted_at", &self.deleted_at)
            .finish()
    }
}

/// The body of `export_encrypted_backup`.
///
/// There is no `Debug` here on purpose, and there never will be: this struct
/// carries the backup password, and a derived one would print it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportRequest {
    /// The file to write, chosen by the user in the native save dialog.
    pub destination_path: String,
    pub backup_password: Zeroizing<String>,
    /// Whether the user already agreed to replace an existing file.
    ///
    /// The backend never replaces one on its own: without this flag a file that
    /// is already there stops the export, so a caller that did not ask cannot
    /// destroy a backup by accident.
    #[serde(default)]
    pub overwrite: bool,
}

/// The body of `inspect_encrypted_backup`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectRequest {
    pub source_path: String,
    pub backup_password: Zeroizing<String>,
}

/// The body of `restore_encrypted_backup`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreRequest {
    pub source_path: String,
    pub backup_password: Zeroizing<String>,
    /// The only value this build accepts is `replace`.
    pub mode: String,
    /// The phrase the user typed. Checked here as well as in the UI.
    pub confirmation: String,
    /// The digest the inspect step reported for the file the user previewed.
    ///
    /// A restore names the exact bytes it replaces the vault with, so a file
    /// that changed after the preview — rewritten, half-copied or swapped for
    /// another one with a different password — is refused instead of silently
    /// restored. The digest travels from preview to restore and never decides
    /// anything else.
    pub file_digest: String,
}

/// How many records a snapshot holds, by kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCount {
    pub kind: String,
    pub active: usize,
    pub deleted: usize,
}

/// What `inspect_encrypted_backup` answers with.
///
/// Counts only, plus the digest of the exact file that was read. There is
/// deliberately no field for a secret, a name, an address or a whole payload: a
/// preview shows how much data there is, never what it is.
///
/// The digest is not a secret either — it is a SHA-256 over the file's bytes —
/// and it exists so a restore can name the file it is replacing the vault with:
/// see [`RestoreRequest::file_digest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub created_at: i64,
    pub source_app_version: String,
    pub total_objects: usize,
    pub active_objects: usize,
    pub deleted_objects: usize,
    pub count_by_kind: Vec<KindCount>,
    /// `sha256:<base64>` of the file this summary was read from.
    pub file_digest: String,
}

/// What `export_encrypted_backup` answers with.
///
/// The file name, not the whole path: the path is the user's directory layout
/// and the UI has no use for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub file_name: String,
    pub bytes: u64,
    pub created_at: i64,
    pub total_objects: usize,
    pub active_objects: usize,
    pub deleted_objects: usize,
    pub count_by_kind: Vec<KindCount>,
}

/// What `restore_encrypted_backup` answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub file_name: String,
    pub created_at: i64,
    /// Every record the snapshot held, which is every record the vault holds
    /// afterwards.
    pub replaced_objects: usize,
    pub active_objects: usize,
    pub deleted_objects: usize,
    pub count_by_kind: Vec<KindCount>,
}
