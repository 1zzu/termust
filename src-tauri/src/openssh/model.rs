//! What an OpenSSH config import looks like on the wire and in a preview.
//!
//! Nothing here is stored: a preview exists in memory for as long as the dialog
//! needs it, and an import writes ordinary hosts. There is no import table, no
//! import history and no marker on the host that says where it came from.

use serde::{Deserialize, Serialize};

/// The port an OpenSSH host uses when the config does not say.
pub const DEFAULT_PORT: u16 = 22;

/// The largest config this build accepts. The frontend checks the same limit so
/// the user is told before a large paste is sent, and the backend checks it
/// again because the frontend's check is not a guarantee.
pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;

/// How one entry of a preview ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    /// Everything it needs is there, and nothing in the vault matches it yet.
    Ready,
    /// A saved host already has the same address, port and username.
    AlreadyExists,
    /// A pattern or a directive this build does not read.
    Unsupported,
    /// A block that cannot become a host: no user, or a value that is unusable.
    Invalid,
}

/// A note about one line of the config, or about the config as a whole.
///
/// The message names what was skipped and why. It never repeats a value from the
/// config, and it never repeats the config itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWarning {
    /// The 1-based line the note is about, when it is about a line.
    pub line: Option<u32>,
    pub message: String,
}

/// One `Host` block, as the preview shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntry {
    /// The position of this block in the config, counted from zero. It is what
    /// the import sends back, so it is stable for one content.
    pub index: usize,
    /// The alias from the `Host` line.
    pub label: String,
    /// The `HostName`, or the alias when there is none.
    pub address: String,
    /// The parsed port, or `None` when the config's port is unusable.
    pub port: Option<u16>,
    /// The `User`. Empty when the block does not have one.
    pub username: String,
    pub status: EntryStatus,
    pub warnings: Vec<ConfigWarning>,
}

/// The whole preview: what could be imported, and what was skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub entries: Vec<ImportEntry>,
    /// Notes that belong to no single block: a global default, a `Match`
    /// section, a continued line before the first `Host`.
    pub warnings: Vec<ConfigWarning>,
}

/// What happened to one selected entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOutcome {
    Imported,
    Skipped,
    Failed,
}

/// The result of one selected entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultEntry {
    pub index: usize,
    pub label: String,
    pub outcome: ImportOutcome,
    /// Why it was skipped or could not be created. Never a config value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The host that was created, when one was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
}

/// The answer `import_openssh_hosts` gives.
///
/// The import is not atomic: it writes one host at a time so an entry that
/// cannot be written does not lose the ones already written. Every entry is
/// reported, and running the same import again skips what is already saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub results: Vec<ImportResultEntry>,
}

impl ImportSummary {
    pub fn new(results: Vec<ImportResultEntry>) -> Self {
        let count = |outcome: ImportOutcome| {
            results
                .iter()
                .filter(|entry| entry.outcome == outcome)
                .count()
        };
        Self {
            imported: count(ImportOutcome::Imported),
            skipped: count(ImportOutcome::Skipped),
            failed: count(ImportOutcome::Failed),
            results,
        }
    }
}

/// The body of `preview_openssh_config`.
///
/// There is no path anywhere in this request: the frontend reads the file the
/// user chose and sends its text, so the backend never opens a file named by a
/// config or by the frontend.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewRequest {
    pub content: String,
}

/// The body of `import_openssh_hosts`: the same content, re-parsed, plus the
/// entries the user ticked.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportRequest {
    pub content: String,
    pub selected_entry_indexes: Vec<usize>,
}
