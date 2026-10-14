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

use crate::vault::{Vault, VaultError, VaultObject, new_id, now};

use super::InventoryError;
use super::model::{
    Credential, CredentialSecret, CredentialSummary, Group, Host, HostInput, KIND_CREDENTIAL,
    KIND_GROUP, KIND_HOST, KIND_KNOWN_HOST, KnownHost, KnownKey, MAX_ADDRESS_LEN,
    MAX_ALGORITHM_LEN, MAX_FINGERPRINT_LEN, MAX_LABEL_LEN, MAX_NAME_LEN, MAX_USERNAME_LEN,
    PAYLOAD_VERSION, check_required, check_secret, optional_id, optional_secret, required, secret,
    valid_port,
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
    from_payload(&object.payload)
}

/// Parses one payload, checking its version before it is decoded into a record.
///
/// A payload with a version this build does not know is refused here rather than
/// half-read, which is what lets a portable backup fail closed instead of
/// silently losing an object it does not understand.
fn from_payload<T: DeserializeOwned>(payload: &str) -> Result<T, InventoryError> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| InventoryError::Corrupt)?;
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

/// One decoded inventory record.
///
/// The portable backup walks a snapshot and needs to look *inside* a record — to
/// check its id and the ids it points at — so it needs the record, not just the
/// answer "this parsed".
pub(crate) enum Record {
    Group(Group),
    Host(Host),
    Credential(Credential),
    KnownHost(KnownHost),
}

/// Decodes one domain payload from its kind and its plaintext payload, using the
/// same checks a command reads with.
///
/// This is the single place that says what a group, a host, a credential and a
/// known host look like inside the ciphertext, so a backup written here can only
/// contain records this build understands, and a backup read here can only
/// contain records it would read anyway.
pub(crate) fn decode_record(kind: &str, payload: &str) -> Result<Record, InventoryError> {
    match kind {
        KIND_GROUP => from_payload(payload).map(Record::Group),
        KIND_HOST => from_payload(payload).map(Record::Host),
        KIND_CREDENTIAL => from_payload(payload).map(Record::Credential),
        KIND_KNOWN_HOST => from_payload(payload).map(Record::KnownHost),
        _ => Err(InventoryError::UnknownKind),
    }
}

/// Re-checks one decoded record against the same field rules the commands apply
/// to the input they are given.
///
/// A command validates what the user typed and then builds the record; a record
/// read out of a backup was built elsewhere, so this checks the built shape
/// instead. Both paths end at the same rules: the length limits, the non-empty
/// fields, the usable port, the secret limits and the relation ids all come from
/// the same helpers in `model.rs`. A value the commands would have trimmed has
/// to already be trimmed here, because a restore writes records exactly as it
/// read them and never silently rewrites user data.
pub(crate) fn check_record(record: &Record) -> Result<(), InventoryError> {
    match record {
        Record::Group(group) => {
            unchanged(
                &group.name,
                check_required(&group.name, "Group name", MAX_NAME_LEN)?,
            )?;
        }
        Record::Host(host) => {
            unchanged(
                &host.label,
                check_required(&host.label, "Label", MAX_LABEL_LEN)?,
            )?;
            unchanged(
                &host.address,
                check_required(&host.address, "Address", MAX_ADDRESS_LEN)?,
            )?;
            unchanged(
                &host.username,
                check_required(&host.username, "Username", MAX_USERNAME_LEN)?,
            )?;
            valid_port(i64::from(host.port))?;
            // Absent and empty both mean "no relation" in the commands, so a
            // stored relation has to be the same form they write.
            unchanged_option(&host.group_id, optional_id(&host.group_id))?;
            unchanged_option(&host.credential_id, optional_id(&host.credential_id))?;
        }
        Record::Credential(credential) => {
            unchanged(
                &credential.name,
                check_required(&credential.name, "Credential name", MAX_NAME_LEN)?,
            )?;
            match &credential.secret {
                CredentialSecret::Password { password } => check_secret(password, "Password")?,
                CredentialSecret::PrivateKey {
                    private_key,
                    passphrase,
                } => {
                    check_secret(private_key, "Private key")?;
                    // The commands store an empty passphrase as none, so a record
                    // that carries an empty one was not written by this build.
                    if let Some(passphrase) = passphrase {
                        check_secret(passphrase, "Passphrase")?;
                    }
                }
            }
        }
        Record::KnownHost(known) => {
            unchanged(
                &known.host_key_id,
                check_required(&known.host_key_id, "Host identity", MAX_ADDRESS_LEN)?,
            )?;
            unchanged(
                &known.key_algorithm,
                check_required(&known.key_algorithm, "Key algorithm", MAX_ALGORITHM_LEN)?,
            )?;
            unchanged(
                &known.fingerprint_sha256,
                check_required(
                    &known.fingerprint_sha256,
                    "Fingerprint",
                    MAX_FINGERPRINT_LEN,
                )?,
            )?;
        }
    }
    Ok(())
}

/// A text field has to be exactly what a command would have stored for it.
fn unchanged(value: &str, checked: &str) -> Result<(), InventoryError> {
    if checked == value {
        return Ok(());
    }
    Err(InventoryError::Invalid(
        "A stored field is not in the form this build writes.".to_string(),
    ))
}

/// The same, for an optional relation id.
fn unchanged_option(
    stored: &Option<String>,
    checked: Option<String>,
) -> Result<(), InventoryError> {
    if checked.as_deref() == stored.as_deref() {
        return Ok(());
    }
    Err(InventoryError::Invalid(
        "A stored field is not in the form this build writes.".to_string(),
    ))
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

/// Reads a saved host and the credential it points at, for the SSH layer.
///
/// This is the only way a saved host becomes something a connection can use, and
/// it is where `start_saved_ssh` gets its address, username and secret from. The
/// credential it answers with holds a secret, so it is handed straight to the
/// SSH request and never to the frontend.
///
/// The host's own fields are checked again here: a record written by an older
/// build, or one that lost a field, has to fail with a clear message instead of
/// opening a connection to something it does not describe.
pub fn credential_for_host(
    vault: &Vault,
    host_id: &str,
) -> Result<(Host, Credential), InventoryError> {
    let host = match get_host(vault, host_id) {
        Ok(host) => host,
        // An unknown id and a deleted host mean the same thing to a caller:
        // there is nothing left to connect to.
        Err(InventoryError::Vault(VaultError::NotFound)) => {
            return Err(InventoryError::HostNotFound);
        }
        Err(error) => return Err(error),
    };

    if host.address.trim().is_empty() || host.username.trim().is_empty() || host.port == 0 {
        return Err(InventoryError::IncompleteHost);
    }

    let Some(credential_id) = host.credential_id.as_deref() else {
        return Err(InventoryError::HostHasNoCredential);
    };

    match load::<Credential>(vault, credential_id, KIND_CREDENTIAL) {
        Ok(credential) => Ok((host, credential)),
        Err(InventoryError::Vault(VaultError::NotFound)) => Err(InventoryError::CredentialNotFound),
        Err(error) => Err(error),
    }
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

// --- known hosts ----------------------------------------------------------

pub fn list_known_hosts(vault: &Vault) -> Result<Vec<KnownHost>, InventoryError> {
    let mut known: Vec<KnownHost> = load_all(vault, KIND_KNOWN_HOST)?;
    known.sort_by(|left, right| left.host_key_id.cmp(&right.host_key_id));
    Ok(known)
}

/// The active record for one canonical host identity, if there is one.
pub fn find_known_host(
    vault: &Vault,
    host_key_id: &str,
) -> Result<Option<KnownHost>, InventoryError> {
    Ok(list_known_hosts(vault)?
        .into_iter()
        .find(|known| known.host_key_id == host_key_id))
}

/// Records that this device trusts one server's key.
///
/// Approving the same host twice updates the record it already has instead of
/// adding a second one, so a repeated decision can never leave two active rows
/// for one identity.
pub fn remember_known_host(
    vault: &Vault,
    host_key_id: &str,
    key: &KnownKey,
) -> Result<KnownHost, InventoryError> {
    let host_key_id = required(host_key_id, "Host identity", MAX_ADDRESS_LEN)?;
    let algorithm = required(&key.algorithm, "Key algorithm", MAX_ALGORITHM_LEN)?;
    let fingerprint = required(&key.fingerprint, "Fingerprint", MAX_FINGERPRINT_LEN)?;

    let timestamp = now();
    let existing = find_known_host(vault, &host_key_id)?;
    let id = match &existing {
        Some(record) => record.id.clone(),
        None => new_id()?,
    };
    let known = KnownHost {
        version: PAYLOAD_VERSION,
        id,
        host_key_id,
        key_algorithm: algorithm,
        fingerprint_sha256: fingerprint,
        created_at: existing
            .as_ref()
            .map(|record| record.created_at)
            .unwrap_or(timestamp),
        updated_at: timestamp,
    };

    if existing.is_some() {
        overwrite(vault, &known.id, &known)?;
    } else {
        insert(vault, &known.id, KIND_KNOWN_HOST, &known)?;
    }
    Ok(known)
}

/// Forgets one approved key. The next connection to that host asks again.
pub fn forget_known_host(vault: &Vault, id: &str) -> Result<(), InventoryError> {
    let _: KnownHost = load(vault, id, KIND_KNOWN_HOST)?;
    vault.delete_object(id)?;
    Ok(())
}
