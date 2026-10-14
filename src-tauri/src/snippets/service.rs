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
use super::model::{KIND_SNIPPET, PAYLOAD_VERSION, Snippet, SnippetInput, content, name};

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

    let value: serde_json::Value =
        serde_json::from_str(&object.payload).map_err(|_| SnippetError::Corrupt)?;
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

/// Serialising a record made of strings and numbers cannot fail in practice, so
/// the error here only guards an impossible case rather than a user mistake.
fn to_json(snippet: &Snippet) -> Result<String, SnippetError> {
    serde_json::to_string(snippet).map_err(|_| SnippetError::Internal)
}
