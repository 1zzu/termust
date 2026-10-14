//! CRUD for groups, hosts and credentials.
//!
//! Every read and write goes through the vault's object API, so the encrypted
//! format lives in exactly one place and this module never touches SQLite or a
//! key. Relations are checked here, after decrypting, because the vault itself
//! cannot see inside an object.
//!
//! All of these functions return [`InventoryError::Vault`]`(Locked)` when the
//! vault is locked, because the vault refuses the very first operation.

use serde::{Serialize, de::DeserializeOwned};

use crate::vault::{Vault, VaultObject, new_id, now};

use super::InventoryError;
use super::model::{
    Credential, CredentialSecret, CredentialSummary, Group, Host, HostInput, KIND_CREDENTIAL,
    KIND_GROUP, KIND_HOST, MAX_ADDRESS_LEN, MAX_LABEL_LEN, MAX_NAME_LEN, MAX_USERNAME_LEN,
    PAYLOAD_VERSION, optional_id, optional_secret, required, secret, valid_port,
};

// --- shared plumbing ------------------------------------------------------

/// Reads and checks one object, returning its typed payload.
///
/// The kind is checked before anything else: an object's kind is part of its
/// encryption's Additional Authenticated Data, so an object written as a group
/// can never be read as a host.
pub(super) fn load<T: DeserializeOwned>(
    vault: &Vault,
    id: &str,
    kind: &'static str,
) -> Result<T, InventoryError> {
    decode(&vault.get_object(id)?, kind)
}

fn decode<T: DeserializeOwned>(
    object: &VaultObject,
    kind: &'static str,
) -> Result<T, InventoryError> {
    if object.kind != kind {
        return Err(InventoryError::WrongKind { expected: kind });
    }

    let value: serde_json::Value =
        serde_json::from_str(&object.payload).map_err(|_| InventoryError::Corrupt)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(InventoryError::Corrupt)?;
    if version != u64::from(PAYLOAD_VERSION) {
        return Err(InventoryError::UnsupportedPayloadVersion {
            found: version,
            supported: PAYLOAD_VERSION,
        });
    }

    serde_json::from_value(value).map_err(|_| InventoryError::Corrupt)
}

fn load_all<T: DeserializeOwned>(
    vault: &Vault,
    kind: &'static str,
) -> Result<Vec<T>, InventoryError> {
    vault
        .list_objects(Some(kind))?
        .iter()
        .map(|object| decode(object, kind))
        .collect()
}

fn insert<T: Serialize>(
    vault: &Vault,
    id: &str,
    kind: &str,
    record: &T,
) -> Result<(), InventoryError> {
    vault.put_object_with_id(id, kind, &to_json(record)?)?;
    Ok(())
}

fn overwrite<T: Serialize>(vault: &Vault, id: &str, record: &T) -> Result<(), InventoryError> {
    vault.update_object(id, &to_json(record)?)?;
    Ok(())
}

/// Serialising a record made of strings and numbers cannot fail in practice, so
/// the error here only guards a impossible case rather than a user mistake.
fn to_json<T: Serialize>(record: &T) -> Result<String, InventoryError> {
    serde_json::to_string(record).map_err(|_| InventoryError::Internal)
}

// --- groups ---------------------------------------------------------------

pub fn list_groups(vault: &Vault) -> Result<Vec<Group>, InventoryError> {
    let mut groups: Vec<Group> = load_all(vault, KIND_GROUP)?;
    groups.sort_by_key(|group| group.name.to_lowercase());
    Ok(groups)
}

pub fn create_group(vault: &Vault, name: &str) -> Result<Group, InventoryError> {
    let name = required(name, "Group name", MAX_NAME_LEN)?;
    if group_named(vault, &name)?.is_some() {
        return Err(InventoryError::DuplicateGroupName(name));
    }

    let id = new_id()?;
    let timestamp = now();
    let group = Group {
        version: PAYLOAD_VERSION,
        id: id.clone(),
        name,
        created_at: timestamp,
        updated_at: timestamp,
    };
    insert(vault, &id, KIND_GROUP, &group)?;
    Ok(group)
}

pub fn rename_group(vault: &Vault, id: &str, name: &str) -> Result<Group, InventoryError> {
    let name = required(name, "Group name", MAX_NAME_LEN)?;
    let mut group: Group = load(vault, id, KIND_GROUP)?;

    if let Some(existing) = group_named(vault, &name)?
        && existing.id != id
    {
        return Err(InventoryError::DuplicateGroupName(name));
    }

    group.name = name;
    group.updated_at = now();
    overwrite(vault, id, &group)?;
    Ok(group)
}

pub fn delete_group(vault: &Vault, id: &str) -> Result<(), InventoryError> {
    // Resolve the group first, so a wrong id is "not found" rather than
    // "in use".
    let _: Group = load(vault, id, KIND_GROUP)?;
    if list_hosts(vault)?
        .iter()
        .any(|host| host.group_id.as_deref() == Some(id))
    {
        return Err(InventoryError::GroupInUse);
    }
    vault.delete_object(id)?;
    Ok(())
}

/// The group with this name, compared without regard to case so the UI cannot
/// end up with two groups that look identical.
fn group_named(vault: &Vault, name: &str) -> Result<Option<Group>, InventoryError> {
    Ok(list_groups(vault)?
        .into_iter()
        .find(|group| group.name.eq_ignore_ascii_case(name)))
}

// --- hosts ----------------------------------------------------------------

pub fn list_hosts(vault: &Vault) -> Result<Vec<Host>, InventoryError> {
    let mut hosts: Vec<Host> = load_all(vault, KIND_HOST)?;
    hosts.sort_by_key(|host| host.label.to_lowercase());
    Ok(hosts)
}

pub fn get_host(vault: &Vault, id: &str) -> Result<Host, InventoryError> {
    load(vault, id, KIND_HOST)
}

pub fn create_host(vault: &Vault, input: &HostInput) -> Result<Host, InventoryError> {
    let host = build_host(vault, new_id()?, None, input)?;
    insert(vault, &host.id, KIND_HOST, &host)?;
    Ok(host)
}

pub fn update_host(vault: &Vault, id: &str, input: &HostInput) -> Result<Host, InventoryError> {
    let existing: Host = load(vault, id, KIND_HOST)?;
    let host = build_host(vault, id.to_string(), Some(existing.created_at), input)?;
    overwrite(vault, id, &host)?;
    Ok(host)
}

pub fn delete_host(vault: &Vault, id: &str) -> Result<(), InventoryError> {
    let _: Host = load(vault, id, KIND_HOST)?;
    vault.delete_object(id)?;
    Ok(())
}

/// Validates the fields and both relationships, then builds the record.
///
/// A relationship is checked by trying to read the other record: that fails for
/// an unknown id and for a deleted one alike, which is exactly what a link must
/// not point at.
fn build_host(
    vault: &Vault,
    id: String,
    created_at: Option<i64>,
    input: &HostInput,
) -> Result<Host, InventoryError> {
    let label = required(&input.label, "Label", MAX_LABEL_LEN)?;
    let address = required(&input.address, "Address", MAX_ADDRESS_LEN)?;
    let port = valid_port(input.port)?;
    let username = required(&input.username, "Username", MAX_USERNAME_LEN)?;

    let group_id = optional_id(&input.group_id);
    if let Some(group_id) = &group_id {
        let _: Group = load(vault, group_id, KIND_GROUP)?;
    }

    let credential_id = optional_id(&input.credential_id);
    if let Some(credential_id) = &credential_id {
        let _: Credential = load(vault, credential_id, KIND_CREDENTIAL)?;
    }

    let timestamp = now();
    Ok(Host {
        version: PAYLOAD_VERSION,
        id,
        label,
        address,
        port,
        username,
        group_id,
        credential_id,
        created_at: created_at.unwrap_or(timestamp),
        updated_at: timestamp,
    })
}

// --- credentials ----------------------------------------------------------

/// Every credential, as summaries only. The secrets are decrypted in order to
/// list them, then dropped and zeroized on the way out.
pub fn list_credentials(vault: &Vault) -> Result<Vec<CredentialSummary>, InventoryError> {
    let mut summaries: Vec<CredentialSummary> = load_all::<Credential>(vault, KIND_CREDENTIAL)?
        .into_iter()
        .map(CredentialSummary::from)
        .collect();
    summaries.sort_by_key(|summary| summary.name.to_lowercase());
    Ok(summaries)
}

pub fn create_password_credential(
    vault: &Vault,
    name: &str,
    password: &str,
) -> Result<CredentialSummary, InventoryError> {
    let name = required(name, "Credential name", MAX_NAME_LEN)?;
    let credential = build_credential(
        new_id()?,
        None,
        name,
        CredentialSecret::Password {
            password: secret(password, "Password")?,
        },
    );
    insert(vault, &credential.id, KIND_CREDENTIAL, &credential)?;
    Ok(credential.into())
}

pub fn create_private_key_credential(
    vault: &Vault,
    name: &str,
    private_key: &str,
    passphrase: Option<&str>,
) -> Result<CredentialSummary, InventoryError> {
    let name = required(name, "Credential name", MAX_NAME_LEN)?;
    let credential = build_credential(
        new_id()?,
        None,
        name,
        CredentialSecret::PrivateKey {
            private_key: secret(private_key, "Private key")?,
            passphrase: optional_secret(passphrase)?,
        },
    );
    insert(vault, &credential.id, KIND_CREDENTIAL, &credential)?;
    Ok(credential.into())
}

/// Changes the name and nothing else: the stored secret is written back
/// unchanged.
pub fn rename_credential(
    vault: &Vault,
    id: &str,
    name: &str,
) -> Result<CredentialSummary, InventoryError> {
    let name = required(name, "Credential name", MAX_NAME_LEN)?;
    let mut credential: Credential = load(vault, id, KIND_CREDENTIAL)?;
    credential.name = name;
    credential.updated_at = now();
    overwrite(vault, id, &credential)?;
    Ok(credential.into())
}

pub fn replace_password(vault: &Vault, id: &str, password: &str) -> Result<(), InventoryError> {
    let mut credential: Credential = load(vault, id, KIND_CREDENTIAL)?;
    credential.secret = CredentialSecret::Password {
        password: secret(password, "Password")?,
    };
    credential.updated_at = now();
    overwrite(vault, id, &credential)
}

pub fn replace_private_key(
    vault: &Vault,
    id: &str,
    private_key: &str,
    passphrase: Option<&str>,
) -> Result<(), InventoryError> {
    let mut credential: Credential = load(vault, id, KIND_CREDENTIAL)?;
    credential.secret = CredentialSecret::PrivateKey {
        private_key: secret(private_key, "Private key")?,
        passphrase: optional_secret(passphrase)?,
    };
    credential.updated_at = now();
    overwrite(vault, id, &credential)
}

pub fn delete_credential(vault: &Vault, id: &str) -> Result<(), InventoryError> {
    let _: Credential = load(vault, id, KIND_CREDENTIAL)?;
    if list_hosts(vault)?
        .iter()
        .any(|host| host.credential_id.as_deref() == Some(id))
    {
        return Err(InventoryError::CredentialInUse);
    }
    vault.delete_object(id)?;
    Ok(())
}

fn build_credential(
    id: String,
    created_at: Option<i64>,
    name: String,
    secret: CredentialSecret,
) -> Credential {
    let timestamp = now();
    Credential {
        version: PAYLOAD_VERSION,
        id,
        name,
        secret,
        created_at: created_at.unwrap_or(timestamp),
        updated_at: timestamp,
    }
}
