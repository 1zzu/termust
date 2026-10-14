//! CRUD for snippets.
//!
//! Every read and write goes through the vault's object API, so the encrypted
//! format lives in exactly one place and this module never touches SQLite or a
//! key. There are no relations to check: a snippet points at nothing.
//!
//! All of these functions return [`SnippetError::Vault`]`(Locked)` when the
//! vault is locked, because the vault refuses the very first operation.

use crate::vault::{Vault, VaultError, VaultObject, new_id, now};

use super::SnippetError;
use super::model::{
    KIND_SNIPPET, PAYLOAD_VERSION, Snippet, SnippetInput, check_content, check_name, content, name,
};

/// One snippet, read by id. An unknown id and a deleted one mean the same thing
/// to a caller: there is nothing left to read.
pub fn get_snippet(vault: &Vault, id: &str) -> Result<Snippet, SnippetError> {
    load(vault, id)
}

/// Every active snippet, ordered by name and then by id.
///
/// The name is compared without regard to case, so the list does not reshuffle
/// when a label changes case, and the id breaks ties so the order is stable and
/// the same on every read.
pub fn list_snippets(vault: &Vault) -> Result<Vec<Snippet>, SnippetError> {
    let mut snippets: Vec<Snippet> = vault
        .list_objects(Some(KIND_SNIPPET))?
        .iter()
        .map(decode)
        .collect::<Result<_, _>>()?;
    snippets.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(snippets)
}

/// Writes a new snippet. The id and both timestamps are chosen here, so two
/// snippets may share a name: the id is the identity.
pub fn create_snippet(vault: &Vault, input: &SnippetInput) -> Result<Snippet, SnippetError> {
    let id = new_id()?;
    let timestamp = now();
    let snippet = Snippet {
        version: PAYLOAD_VERSION,
        id: id.clone(),
        name: name(&input.name)?,
        content: content(&input.content)?,
        created_at: timestamp,
        updated_at: timestamp,
    };
    vault.put_object_with_id(&id, KIND_SNIPPET, &to_json(&snippet)?)?;
    Ok(snippet)
}

/// Rewrites a snippet, keeping its id and its creation time.
pub fn update_snippet(
    vault: &Vault,
    id: &str,
    input: &SnippetInput,
) -> Result<Snippet, SnippetError> {
    let existing: Snippet = load(vault, id)?;
    let snippet = Snippet {
        version: PAYLOAD_VERSION,
        id: id.to_string(),
        name: name(&input.name)?,
        content: content(&input.content)?,
        created_at: existing.created_at,
        updated_at: now(),
    };
    vault.update_object(id, &to_json(&snippet)?)?;
    Ok(snippet)
}

/// Soft-deletes a snippet: the row stays and `deleted_at` is set, exactly as
/// every other vault object is removed.
pub fn delete_snippet(vault: &Vault, id: &str) -> Result<(), SnippetError> {
    let _: Snippet = load(vault, id)?;
    vault.delete_object(id)?;
    Ok(())
}

fn load(vault: &Vault, id: &str) -> Result<Snippet, SnippetError> {
    let object = match vault.get_object(id) {
        Ok(object) => object,
        Err(VaultError::NotFound) => return Err(SnippetError::NotFound),
        Err(error) => return Err(SnippetError::Vault(error)),
    };
    decode(&object)
}

/// Reads one object as a snippet.
///
/// The kind is checked before anything else: an object's kind is part of its
/// encryption's Additional Authenticated Data, so an object written as a host
/// can never be read as a snippet.
fn decode(object: &VaultObject) -> Result<Snippet, SnippetError> {
    if object.kind != KIND_SNIPPET {
        return Err(SnippetError::WrongKind {
            expected: KIND_SNIPPET,
        });
    }
    decode_payload(&object.payload)
}

/// Parses one payload, checking its version before it is decoded into a record.
///
/// A payload with a version this build does not know is refused here rather than
/// half-read, which is what lets a portable backup fail closed instead of
/// silently losing an object it does not understand.
fn decode_payload(payload: &str) -> Result<Snippet, SnippetError> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| SnippetError::Corrupt)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(SnippetError::Corrupt)?;
    if version != u64::from(PAYLOAD_VERSION) {
        return Err(SnippetError::UnsupportedPayloadVersion {
            found: version,
            supported: PAYLOAD_VERSION,
        });
    }

    serde_json::from_value(value).map_err(|_| SnippetError::Corrupt)
}

/// Decodes one snippet from its plaintext payload, using the same checks a
/// command reads with.
///
/// The portable backup calls this, so a file can only ever hold the snippets this
/// build would read back.
pub(crate) fn decode_record(payload: &str) -> Result<Snippet, SnippetError> {
    decode_payload(payload)
}

/// Re-checks one decoded snippet against the same field rules `create_snippet`
/// and `update_snippet` apply to their input.
///
/// The rules themselves live in `model.rs` and are the very ones the commands
/// call; this only adds the one thing a decoded record can break that an input
/// cannot: a value the commands would have trimmed has to already be trimmed,
/// because a restore writes records exactly as it read them. The content is
/// never trimmed, so its rule is the whole check, NUL bytes and the 64 KiB limit
/// included.
pub(crate) fn check_snippet(snippet: &Snippet) -> Result<(), SnippetError> {
    if check_name(&snippet.name)? != snippet.name
        || check_content(&snippet.content)? != snippet.content
    {
        return Err(SnippetError::Corrupt);
    }
    Ok(())
}

/// Serialising a record made of strings and numbers cannot fail in practice, so
/// the error here only guards an impossible case rather than a user mistake.
fn to_json(snippet: &Snippet) -> Result<String, SnippetError> {
    serde_json::to_string(snippet).map_err(|_| SnippetError::Internal)
}
