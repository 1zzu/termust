//! Behaviour tests for groups, hosts and credentials.
//!
//! Every test runs against its own temporary vault database, created with the
//! cheap Argon2id parameters, and removes it afterwards. No test touches a real
//! vault or the app data directory.

use std::fs;
use std::path::Path;

use rusqlite::{Connection, params};

use crate::vault::test_support::TempVault;
use crate::vault::{Vault, VaultError, VaultStatus};

use super::InventoryError;
use super::model::{
    Credential, CredentialKind, CredentialSecret, HostInput, KIND_CREDENTIAL, KIND_GROUP,
    PAYLOAD_VERSION,
};
use super::service;

/// The master password every test creates its vault with.
const PASSWORD: &str = "a-strong-master-password";

/// Distinctive values for the plaintext scans. They must not appear anywhere in
/// the database file.
const PASSWORD_MARKER: &str = "marker-password-8f2a";
const KEY_MARKER: &str = "marker-private-key-3c1b";
const PASSPHRASE_MARKER: &str = "marker-passphrase-7d4e";
const KEY_MATERIAL: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nnot-a-real-key\n";

/// A temporary vault that has been created and is left unlocked.
fn unlocked_vault() -> (TempVault, Vault) {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).expect("the vault was created");
    (temp, vault)
}

fn host_input(label: &str, group_id: Option<&str>, credential_id: Option<&str>) -> HostInput {
    HostInput {
        label: label.to_string(),
        address: format!("{label}.example.com"),
        port: 22,
        username: "root".to_string(),
        group_id: group_id.map(str::to_string),
        credential_id: credential_id.map(str::to_string),
    }
}

/// The stored secret of a credential, read the way the service reads records.
fn stored_secret(vault: &Vault, id: &str) -> CredentialSecret {
    service::load::<Credential>(vault, id, KIND_CREDENTIAL)
        .expect("the credential is stored")
        .secret
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Flips one bit of an object's ciphertext, as a damaged file would.
fn flip_payload_byte(path: &Path, id: &str) {
    let connection = Connection::open(path).unwrap();
    let mut payload: Vec<u8> = connection
        .query_row(
            "SELECT encrypted_payload FROM vault_objects WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap();
    payload[0] ^= 0x01;
    connection
        .execute(
            "UPDATE vault_objects SET encrypted_payload = ?1 WHERE id = ?2",
            params![payload, id],
        )
        .unwrap();
}

// --- groups ---------------------------------------------------------------

#[test]
fn groups_can_be_created_listed_renamed_and_deleted() {
    let (_temp, vault) = unlocked_vault();

    let work = service::create_group(&vault, "  Work  ").unwrap();
    assert_eq!(work.name, "Work", "the name was not trimmed");
    let lab = service::create_group(&vault, "Lab").unwrap();

    let names: Vec<String> = service::list_groups(&vault)
        .unwrap()
        .into_iter()
        .map(|group| group.name)
        .collect();
    assert_eq!(names, vec!["Lab".to_string(), "Work".to_string()]);

    let renamed = service::rename_group(&vault, &work.id, "Office").unwrap();
    assert_eq!(renamed.name, "Office");

    service::delete_group(&vault, &lab.id).unwrap();
    let remaining = service::list_groups(&vault).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, work.id);
}

#[test]
fn a_blank_group_name_is_refused() {
    let (_temp, vault) = unlocked_vault();

    for blank in ["", "   ", "\n\t"] {
        let error = service::create_group(&vault, blank).unwrap_err();
        assert!(
            matches!(error, InventoryError::Invalid(_)),
            "blank name {blank:?} produced {error:?}"
        );
    }
    assert!(service::list_groups(&vault).unwrap().is_empty());
}

#[test]
fn a_group_name_must_be_unique_regardless_of_case() {
    let (_temp, vault) = unlocked_vault();
    let work = service::create_group(&vault, "Work").unwrap();

    let error = service::create_group(&vault, "  work ").unwrap_err();
    assert!(matches!(error, InventoryError::DuplicateGroupName(_)));

    // Renaming a group to its own name in another case is not a duplicate.
    service::rename_group(&vault, &work.id, "WORK").unwrap();

    let other = service::create_group(&vault, "Lab").unwrap();
    let error = service::rename_group(&vault, &other.id, "work").unwrap_err();
    assert!(matches!(error, InventoryError::DuplicateGroupName(_)));
}

#[test]
fn a_group_a_host_uses_cannot_be_deleted() {
    let (_temp, vault) = unlocked_vault();
    let group = service::create_group(&vault, "Work").unwrap();
    let host = service::create_host(&vault, &host_input("web-1", Some(&group.id), None)).unwrap();

    let error = service::delete_group(&vault, &group.id).unwrap_err();
    assert!(matches!(error, InventoryError::GroupInUse));

    service::delete_host(&vault, &host.id).unwrap();
    service::delete_group(&vault, &group.id).unwrap();
}

#[test]
fn group_operations_are_refused_while_locked() {
    let (_temp, mut vault) = unlocked_vault();
    let group = service::create_group(&vault, "Work").unwrap();
    vault.lock();

    let results = [
        service::list_groups(&vault).map(|_| ()),
        service::create_group(&vault, "New").map(|_| ()),
        service::rename_group(&vault, &group.id, "Other").map(|_| ()),
        service::delete_group(&vault, &group.id),
        service::list_hosts(&vault).map(|_| ()),
        service::list_credentials(&vault).map(|_| ()),
    ];
    for result in results {
        assert!(
            matches!(result, Err(InventoryError::Vault(VaultError::Locked))),
            "a locked vault answered with {result:?}"
        );
    }
}

// --- credentials ----------------------------------------------------------

#[test]
fn a_password_credential_can_be_created_listed_and_deleted() {
    let (_temp, vault) = unlocked_vault();

    let summary = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    assert_eq!(summary.kind, CredentialKind::Password);
    assert_eq!(summary.name, "Prod");

    let all = service::list_credentials(&vault).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, summary.id);

    service::delete_credential(&vault, &summary.id).unwrap();
    assert!(service::list_credentials(&vault).unwrap().is_empty());
}

#[test]
fn a_private_key_credential_can_be_created_with_and_without_a_passphrase() {
    let (_temp, vault) = unlocked_vault();

    let plain =
        service::create_private_key_credential(&vault, "Plain", KEY_MATERIAL, None).unwrap();
    let locked =
        service::create_private_key_credential(&vault, "Locked", KEY_MATERIAL, Some("unlock"))
            .unwrap();
    let blank =
        service::create_private_key_credential(&vault, "Blank", KEY_MATERIAL, Some("")).unwrap();

    assert_eq!(plain.kind, CredentialKind::PrivateKey);
    assert_eq!(locked.kind, CredentialKind::PrivateKey);

    match stored_secret(&vault, &plain.id) {
        CredentialSecret::PrivateKey {
            private_key,
            passphrase,
        } => {
            assert_eq!(private_key.as_str(), KEY_MATERIAL);
            assert!(passphrase.is_none());
        }
        other => panic!("expected a private key, got {other:?}"),
    }

    match stored_secret(&vault, &locked.id) {
        CredentialSecret::PrivateKey { passphrase, .. } => {
            assert_eq!(
                passphrase.as_ref().map(|value| value.as_str()),
                Some("unlock")
            );
        }
        other => panic!("expected a private key, got {other:?}"),
    }

    // An empty passphrase means "no passphrase", not an empty one.
    match stored_secret(&vault, &blank.id) {
        CredentialSecret::PrivateKey { passphrase, .. } => assert!(passphrase.is_none()),
        other => panic!("expected a private key, got {other:?}"),
    }
}

#[test]
fn a_blank_credential_name_or_secret_is_refused() {
    let (_temp, vault) = unlocked_vault();

    assert!(matches!(
        service::create_password_credential(&vault, "   ", PASSWORD_MARKER),
        Err(InventoryError::Invalid(_))
    ));
    assert!(matches!(
        service::create_password_credential(&vault, "Prod", ""),
        Err(InventoryError::Invalid(_))
    ));
    assert!(matches!(
        service::create_private_key_credential(&vault, "Key", "", None),
        Err(InventoryError::Invalid(_))
    ));
}

#[test]
fn a_credential_summary_never_carries_the_secret() {
    let (_temp, vault) = unlocked_vault();
    service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    service::create_private_key_credential(&vault, "Key", KEY_MARKER, Some(PASSPHRASE_MARKER))
        .unwrap();

    let summaries = serde_json::to_value(service::list_credentials(&vault).unwrap()).unwrap();
    let text = summaries.to_string();

    for marker in [PASSWORD_MARKER, KEY_MARKER, PASSPHRASE_MARKER] {
        assert!(!text.contains(marker), "a summary leaked {marker}");
    }

    let items = summaries.as_array().expect("a list of summaries");
    assert_eq!(items.len(), 2);
    for summary in items {
        for field in ["secret", "password", "privateKey", "passphrase"] {
            assert!(
                summary.get(field).is_none(),
                "a summary has a {field} field: {summary}"
            );
        }
    }
}

#[test]
fn credential_debug_output_never_contains_the_secret() {
    let (_temp, vault) = unlocked_vault();
    let password = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    let key =
        service::create_private_key_credential(&vault, "Key", KEY_MARKER, Some(PASSPHRASE_MARKER))
            .unwrap();

    let password_debug = format!(
        "{:?}",
        service::load::<Credential>(&vault, &password.id, KIND_CREDENTIAL).unwrap()
    );
    let key_debug = format!("{:?}", stored_secret(&vault, &key.id));

    assert!(!password_debug.contains(PASSWORD_MARKER));
    assert!(password_debug.contains("***"));
    assert!(!key_debug.contains(KEY_MARKER));
    assert!(!key_debug.contains(PASSPHRASE_MARKER));
}

#[test]
fn renaming_a_credential_does_not_change_its_secret() {
    let (_temp, vault) = unlocked_vault();
    let summary = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();

    let renamed = service::rename_credential(&vault, &summary.id, "Production").unwrap();

    assert_eq!(renamed.name, "Production");
    match stored_secret(&vault, &summary.id) {
        CredentialSecret::Password { password } => assert_eq!(password.as_str(), PASSWORD_MARKER),
        other => panic!("expected a password, got {other:?}"),
    }
}

#[test]
fn replacing_a_password_changes_only_the_secret() {
    let (_temp, vault) = unlocked_vault();
    let summary = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();

    service::replace_password(&vault, &summary.id, "a-brand-new-password").unwrap();

    match stored_secret(&vault, &summary.id) {
        CredentialSecret::Password { password } => {
            assert_eq!(password.as_str(), "a-brand-new-password");
        }
        other => panic!("expected a password, got {other:?}"),
    }
    assert_eq!(service::list_credentials(&vault).unwrap()[0].name, "Prod");
}

#[test]
fn replacing_a_private_key_changes_the_key_and_the_passphrase() {
    let (_temp, vault) = unlocked_vault();
    let summary =
        service::create_private_key_credential(&vault, "Key", KEY_MARKER, Some(PASSPHRASE_MARKER))
            .unwrap();

    service::replace_private_key(&vault, &summary.id, "new-key-material", None).unwrap();

    match stored_secret(&vault, &summary.id) {
        CredentialSecret::PrivateKey {
            private_key,
            passphrase,
        } => {
            assert_eq!(private_key.as_str(), "new-key-material");
            assert!(passphrase.is_none(), "the old passphrase survived");
        }
        other => panic!("expected a private key, got {other:?}"),
    }
}

#[test]
fn a_credential_a_host_uses_cannot_be_deleted() {
    let (_temp, vault) = unlocked_vault();
    let credential = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    let host =
        service::create_host(&vault, &host_input("web-1", None, Some(&credential.id))).unwrap();

    let error = service::delete_credential(&vault, &credential.id).unwrap_err();
    assert!(matches!(error, InventoryError::CredentialInUse));

    service::delete_host(&vault, &host.id).unwrap();
    service::delete_credential(&vault, &credential.id).unwrap();
}

#[test]
fn credential_errors_never_echo_a_secret() {
    let (_temp, vault) = unlocked_vault();
    let credential = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    service::create_host(&vault, &host_input("web-1", None, Some(&credential.id))).unwrap();

    let messages = [
        service::delete_credential(&vault, &credential.id)
            .unwrap_err()
            .to_string(),
        service::create_password_credential(&vault, "   ", PASSWORD_MARKER)
            .unwrap_err()
            .to_string(),
        service::create_private_key_credential(&vault, "  ", KEY_MARKER, Some(PASSPHRASE_MARKER))
            .unwrap_err()
            .to_string(),
        service::replace_password(&vault, "not-a-real-id", PASSWORD_MARKER)
            .unwrap_err()
            .to_string(),
    ];

    for message in messages {
        for marker in [PASSWORD_MARKER, KEY_MARKER, PASSPHRASE_MARKER] {
            assert!(
                !message.contains(marker),
                "an error leaked {marker}: {message}"
            );
        }
    }
}

#[test]
fn credential_operations_are_refused_while_locked() {
    let (_temp, mut vault) = unlocked_vault();
    let credential = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    vault.lock();

    let results = [
        service::list_credentials(&vault).map(|_| ()),
        service::create_password_credential(&vault, "Other", PASSWORD_MARKER).map(|_| ()),
        service::create_private_key_credential(&vault, "Key", KEY_MARKER, None).map(|_| ()),
        service::rename_credential(&vault, &credential.id, "Other").map(|_| ()),
        service::replace_password(&vault, &credential.id, "new"),
        service::replace_private_key(&vault, &credential.id, "new", None),
        service::delete_credential(&vault, &credential.id),
    ];
    for result in results {
        assert!(
            matches!(result, Err(InventoryError::Vault(VaultError::Locked))),
            "a locked vault answered with {result:?}"
        );
    }
}

// --- hosts ----------------------------------------------------------------

#[test]
fn hosts_can_be_created_listed_read_updated_and_deleted() {
    let (_temp, vault) = unlocked_vault();
    let group = service::create_group(&vault, "Work").unwrap();
    let credential = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();

    let mut input = host_input("web-1", Some(&group.id), Some(&credential.id));
    let host = service::create_host(&vault, &input).unwrap();

    assert_eq!(host.label, "web-1");
    assert_eq!(host.port, 22);
    assert_eq!(host.group_id.as_deref(), Some(group.id.as_str()));
    assert_eq!(host.credential_id.as_deref(), Some(credential.id.as_str()));
    assert_eq!(service::get_host(&vault, &host.id).unwrap(), host);
    assert_eq!(service::list_hosts(&vault).unwrap().len(), 1);

    input.label = "web-2".to_string();
    input.port = 2222;
    input.group_id = None;
    let updated = service::update_host(&vault, &host.id, &input).unwrap();

    assert_eq!(updated.label, "web-2");
    assert_eq!(updated.port, 2222);
    assert!(updated.group_id.is_none());
    assert_eq!(updated.created_at, host.created_at, "created_at changed");
    assert_eq!(updated.id, host.id);

    service::delete_host(&vault, &host.id).unwrap();
    assert!(service::list_hosts(&vault).unwrap().is_empty());
    assert!(matches!(
        service::get_host(&vault, &host.id),
        Err(InventoryError::Vault(VaultError::NotFound))
    ));
}

#[test]
fn an_out_of_range_port_is_refused() {
    let (_temp, vault) = unlocked_vault();

    for port in [0, -1, 65536, 100_000] {
        let mut input = host_input("web", None, None);
        input.port = port;
        let error = service::create_host(&vault, &input).unwrap_err();
        assert!(
            matches!(error, InventoryError::Invalid(_)),
            "port {port} produced {error:?}"
        );
    }
    assert!(service::list_hosts(&vault).unwrap().is_empty());
}

#[test]
fn blank_host_fields_are_refused() {
    let (_temp, vault) = unlocked_vault();

    let mut blank_label = host_input("web", None, None);
    blank_label.label = "   ".to_string();
    let mut blank_address = host_input("web", None, None);
    blank_address.address = "\t".to_string();
    let mut blank_user = host_input("web", None, None);
    blank_user.username = String::new();

    for input in [blank_label, blank_address, blank_user] {
        let error = service::create_host(&vault, &input).unwrap_err();
        assert!(matches!(error, InventoryError::Invalid(_)), "got {error:?}");
    }
    assert!(service::list_hosts(&vault).unwrap().is_empty());
}

#[test]
fn a_host_cannot_point_at_a_missing_group_or_credential() {
    let (_temp, vault) = unlocked_vault();
    let missing = "11111111-1111-4111-8111-111111111111";

    let error = service::create_host(&vault, &host_input("web", Some(missing), None)).unwrap_err();
    assert!(matches!(error, InventoryError::Vault(VaultError::NotFound)));

    let error = service::create_host(&vault, &host_input("web", None, Some(missing))).unwrap_err();
    assert!(matches!(error, InventoryError::Vault(VaultError::NotFound)));
}

#[test]
fn updating_a_host_validates_the_new_relations() {
    let (_temp, vault) = unlocked_vault();
    let kept = service::create_group(&vault, "Kept").unwrap();
    let scratch = service::create_group(&vault, "Scratch").unwrap();
    let host = service::create_host(&vault, &host_input("web", None, None)).unwrap();

    // Pointing at a live group works.
    let mut input = host_input("web", Some(&kept.id), None);
    let updated = service::update_host(&vault, &host.id, &input).unwrap();
    assert_eq!(updated.group_id.as_deref(), Some(kept.id.as_str()));

    // A group in use cannot be deleted...
    assert!(matches!(
        service::delete_group(&vault, &kept.id),
        Err(InventoryError::GroupInUse)
    ));

    // ...and a deleted group cannot be linked.
    service::delete_group(&vault, &scratch.id).unwrap();
    input.group_id = Some(scratch.id.clone());
    let error = service::update_host(&vault, &host.id, &input).unwrap_err();
    assert!(matches!(error, InventoryError::Vault(VaultError::NotFound)));
}

#[test]
fn deleting_a_host_leaves_a_tombstone() {
    let (temp, vault) = unlocked_vault();
    let host = service::create_host(&vault, &host_input("web", None, None)).unwrap();

    service::delete_host(&vault, &host.id).unwrap();

    let deleted_at: Option<i64> = Connection::open(temp.database())
        .unwrap()
        .query_row(
            "SELECT deleted_at FROM vault_objects WHERE id = ?1",
            params![host.id],
            |row| row.get(0),
        )
        .expect("the row is still there");
    assert!(
        deleted_at.is_some(),
        "the row was removed instead of marked deleted"
    );
}

#[test]
fn data_survives_a_restart_and_unlock() {
    let (temp, vault) = unlocked_vault();
    let group = service::create_group(&vault, "Work").unwrap();
    let credential = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();
    let host = service::create_host(
        &vault,
        &host_input("web-1", Some(&group.id), Some(&credential.id)),
    )
    .unwrap();
    drop(vault);

    let mut reopened = temp.vault();
    assert_eq!(reopened.status().unwrap(), VaultStatus::Locked);
    reopened.unlock(PASSWORD).unwrap();

    assert_eq!(service::list_groups(&reopened).unwrap()[0].name, "Work");
    assert_eq!(
        service::list_credentials(&reopened).unwrap()[0].name,
        "Prod"
    );
    let restored = service::get_host(&reopened, &host.id).unwrap();
    assert_eq!(restored.label, "web-1");
    assert_eq!(restored.group_id.as_deref(), Some(group.id.as_str()));
    assert_eq!(
        restored.credential_id.as_deref(),
        Some(credential.id.as_str())
    );
}

#[test]
fn host_operations_are_refused_while_locked() {
    let (_temp, mut vault) = unlocked_vault();
    let host = service::create_host(&vault, &host_input("web", None, None)).unwrap();
    vault.lock();

    let input = host_input("web", None, None);
    let results = [
        service::list_hosts(&vault).map(|_| ()),
        service::get_host(&vault, &host.id).map(|_| ()),
        service::create_host(&vault, &input).map(|_| ()),
        service::update_host(&vault, &host.id, &input).map(|_| ()),
        service::delete_host(&vault, &host.id),
    ];
    for result in results {
        assert!(
            matches!(result, Err(InventoryError::Vault(VaultError::Locked))),
            "a locked vault answered with {result:?}"
        );
    }
}

// --- encryption and storage -----------------------------------------------

#[test]
fn the_database_holds_no_plaintext_domain_data() {
    let (temp, vault) = unlocked_vault();
    service::create_group(&vault, "marker-group-4e7a").unwrap();
    let credential =
        service::create_password_credential(&vault, "marker-cred-5b2c", PASSWORD_MARKER).unwrap();
    service::create_private_key_credential(
        &vault,
        "marker-key-9d1f",
        KEY_MARKER,
        Some(PASSPHRASE_MARKER),
    )
    .unwrap();
    service::create_host(
        &vault,
        &HostInput {
            label: "marker-label-2a6b".to_string(),
            address: "marker-address-7c3e.example.com".to_string(),
            port: 2200,
            username: "marker-user-8f5d".to_string(),
            group_id: None,
            credential_id: Some(credential.id.clone()),
        },
    )
    .unwrap();

    // The records really are there after decryption...
    assert!(service::list_groups(&vault).unwrap()[0].name == "marker-group-4e7a");
    assert_eq!(
        service::list_hosts(&vault).unwrap()[0].label,
        "marker-label-2a6b"
    );

    // ...but nothing readable is left in the database file.
    let bytes = fs::read(temp.database()).unwrap();
    for marker in [
        "marker-group-4e7a",
        "marker-cred-5b2c",
        "marker-key-9d1f",
        "marker-label-2a6b",
        "marker-address-7c3e",
        "marker-user-8f5d",
        PASSWORD_MARKER,
        KEY_MARKER,
        PASSPHRASE_MARKER,
    ] {
        assert!(
            !contains(&bytes, marker.as_bytes()),
            "{marker} was stored in the clear"
        );
    }
}

#[test]
fn a_tampered_object_is_reported_as_corrupted() {
    let (temp, vault) = unlocked_vault();
    let group = service::create_group(&vault, "Work").unwrap();
    flip_payload_byte(&temp.database(), &group.id);

    let error = service::list_groups(&vault).unwrap_err();
    assert!(
        matches!(error, InventoryError::Vault(VaultError::Corrupt(_))),
        "expected a typed corruption error, got {error:?}"
    );
}

#[test]
fn an_unsupported_payload_version_is_reported_clearly() {
    let (_temp, vault) = unlocked_vault();
    vault
        .put_object(
            KIND_GROUP,
            r#"{"version":99,"id":"x","name":"Future","createdAt":1,"updatedAt":1}"#,
        )
        .unwrap();

    match service::list_groups(&vault).unwrap_err() {
        InventoryError::UnsupportedPayloadVersion { found, supported } => {
            assert_eq!(found, 99);
            assert_eq!(supported, PAYLOAD_VERSION);
        }
        other => panic!("expected an unsupported version error, got {other:?}"),
    }
}

#[test]
fn an_object_without_a_version_is_reported_as_corrupted() {
    let (_temp, vault) = unlocked_vault();
    vault
        .put_object(KIND_GROUP, r#"{"name":"no version here"}"#)
        .unwrap();

    assert!(matches!(
        service::list_groups(&vault).unwrap_err(),
        InventoryError::Corrupt
    ));
}

#[test]
fn an_object_cannot_be_read_as_the_wrong_kind() {
    let (_temp, vault) = unlocked_vault();
    let group = service::create_group(&vault, "Work").unwrap();
    let credential = service::create_password_credential(&vault, "Prod", PASSWORD_MARKER).unwrap();

    assert!(matches!(
        service::get_host(&vault, &group.id).unwrap_err(),
        InventoryError::WrongKind { expected: "host" }
    ));
    assert!(matches!(
        service::delete_group(&vault, &credential.id).unwrap_err(),
        InventoryError::WrongKind { expected: "group" }
    ));
    assert!(matches!(
        service::rename_credential(&vault, &group.id, "Other").unwrap_err(),
        InventoryError::WrongKind {
            expected: "credential"
        }
    ));
}

#[test]
fn a_missing_object_is_reported_as_not_found() {
    let (_temp, vault) = unlocked_vault();

    assert!(matches!(
        service::get_host(&vault, "not-a-real-id"),
        Err(InventoryError::Vault(VaultError::NotFound))
    ));
    assert!(matches!(
        service::delete_group(&vault, "not-a-real-id"),
        Err(InventoryError::Vault(VaultError::NotFound))
    ));
    assert!(matches!(
        service::delete_credential(&vault, "not-a-real-id"),
        Err(InventoryError::Vault(VaultError::NotFound))
    ));
}
