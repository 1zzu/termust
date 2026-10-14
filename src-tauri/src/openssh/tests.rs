//! Behaviour tests for the OpenSSH config import.
//!
//! Every test runs against its own temporary vault database, created with the
//! cheap Argon2id parameters, and removes it afterwards. No test touches a real
//! vault, the app data directory, a file named by a config, or a network.
//!
//! What is checked here is the part behind the two commands: the parser's own
//! rules are pinned in `parser.rs`, the wire contract in `lib.rs`, and the real
//! connection an imported host can make in `ssh::live`.

use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::inventory::{
    HostInput, create_group_record, create_host_record, create_password_record, update_host_record,
};
use crate::vault::test_support::TempVault;
use crate::vault::{Vault, VaultError};

use super::model::{EntryStatus, ImportEntry, ImportOutcome, MAX_CONFIG_BYTES};
use super::{OpenSshError, import, parse, preview, same_target};

/// The master password every test creates its vault with.
const PASSWORD: &str = "a-strong-master-password";

/// Distinctive values for the plaintext scan. They must not appear anywhere in
/// the database file, encrypted or not.
const ALIAS_MARKER: &str = "marker-alias-7e4a";
const ADDRESS_MARKER: &str = "marker-address-2f8c.example.com";
const USERNAME_MARKER: &str = "marker-user-5b1d";
const PORT: u16 = 2222;

/// A temporary vault that has been created and is left unlocked.
fn unlocked_vault() -> (TempVault, Vault) {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).expect("the vault was created");
    (temp, vault)
}

/// One importable block whose every value is a marker, so a plaintext scan has
/// something to look for.
fn marker_config() -> String {
    format!(
        "Host {ALIAS_MARKER}\n  HostName {ADDRESS_MARKER}\n  User {USERNAME_MARKER}\n  Port {PORT}\n"
    )
}

fn input(label: &str, address: &str, port: u16, username: &str) -> HostInput {
    HostInput {
        label: label.to_string(),
        address: address.to_string(),
        port: i64::from(port),
        username: username.to_string(),
        group_id: None,
        credential_id: None,
    }
}

/// Every object row exactly as it is stored: its id, its kind, whether it is a
/// tombstone, and its ciphertext. Comparing this before and after an operation
/// is how a test says "the database was not touched".
fn database_state(path: &Path) -> Vec<(String, String, Option<i64>, Vec<u8>)> {
    let connection = Connection::open(path).expect("the database opened");
    let mut statement = connection
        .prepare("SELECT id, kind, deleted_at, encrypted_payload FROM vault_objects ORDER BY id")
        .expect("the query prepared");
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .expect("the query ran");
    rows.map(|row| row.expect("a row")).collect()
}

fn hosts(vault: &Vault) -> Vec<crate::inventory::Host> {
    crate::inventory::list_host_records(vault).expect("the hosts were listed")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Every note a preview carries, file-level and per entry.
fn notes(preview: &super::ImportPreview) -> Vec<String> {
    preview
        .warnings
        .iter()
        .chain(
            preview
                .entries
                .iter()
                .flat_map(|entry| entry.warnings.iter()),
        )
        .map(|warning| warning.message.clone())
        .collect()
}

// --- preview --------------------------------------------------------------

#[test]
fn a_preview_reports_the_host_it_could_import() {
    let (_temp, vault) = unlocked_vault();

    let previewed = preview(&vault, &marker_config()).expect("the config parsed");

    assert_eq!(previewed.entries.len(), 1);
    let entry = &previewed.entries[0];
    assert_eq!(entry.index, 0);
    assert_eq!(entry.label, ALIAS_MARKER);
    assert_eq!(entry.address, ADDRESS_MARKER);
    assert_eq!(entry.port, Some(PORT));
    assert_eq!(entry.username, USERNAME_MARKER);
    assert_eq!(entry.status, EntryStatus::Ready);
    assert_eq!(previewed.warnings, vec![]);
    assert_eq!(entry.warnings, vec![]);
}

#[test]
fn a_preview_writes_nothing_at_all() {
    let (temp, vault) = unlocked_vault();
    let before = database_state(&temp.database());

    let previewed = preview(&vault, &marker_config()).expect("the config parsed");
    assert_eq!(previewed.entries[0].status, EntryStatus::Ready);

    // No object of any kind was written, and no existing row moved: a preview is
    // a read, tombstones included.
    assert_eq!(database_state(&temp.database()), before);
    assert!(
        vault.list_objects(None).unwrap().is_empty(),
        "a preview stored something"
    );
    assert!(hosts(&vault).is_empty());
}

#[test]
fn a_preview_of_a_config_with_nothing_importable_is_still_a_preview() {
    let (temp, vault) = unlocked_vault();
    let before = database_state(&temp.database());

    let previewed = preview(
        &vault,
        "User root\nMatch host *.internal\n  User admin\nHost *\n  Port 2222\n",
    )
    .expect("the config parsed");

    assert_eq!(
        previewed
            .entries
            .iter()
            .map(|entry| entry.status)
            .collect::<Vec<_>>(),
        vec![EntryStatus::Unsupported]
    );
    assert_eq!(
        previewed.warnings.len(),
        2,
        "a global default and a Match section"
    );
    assert_eq!(database_state(&temp.database()), before);
}

// --- import ---------------------------------------------------------------

#[test]
fn an_import_saves_the_selected_entry_as_an_encrypted_host() {
    let (temp, vault) = unlocked_vault();

    let summary = import(&vault, &marker_config(), &[0]).expect("the import ran");

    assert_eq!(
        (summary.imported, summary.skipped, summary.failed),
        (1, 0, 0)
    );
    assert_eq!(summary.results[0].index, 0);
    assert_eq!(summary.results[0].label, ALIAS_MARKER);
    assert_eq!(summary.results[0].outcome, ImportOutcome::Imported);
    assert_eq!(summary.results[0].reason, None);

    let saved = hosts(&vault);
    assert_eq!(saved.len(), 1);
    assert_eq!(Some(&saved[0].id), summary.results[0].host_id.as_ref());
    assert_eq!(saved[0].label, ALIAS_MARKER);
    assert_eq!(saved[0].address, ADDRESS_MARKER);
    assert_eq!(saved[0].port, PORT);
    assert_eq!(saved[0].username, USERNAME_MARKER);
    assert_eq!(saved[0].version, 1);
    assert_eq!(saved[0].group_id, None, "an import invented a group");
    assert_eq!(
        saved[0].credential_id, None,
        "an import invented a credential"
    );

    // The host is one encrypted object of the ordinary kind, and nothing the
    // config said is readable in the database file.
    assert_eq!(vault.list_objects(Some("host")).unwrap().len(), 1);
    let bytes = fs::read(temp.database()).unwrap();
    for marker in [ALIAS_MARKER, ADDRESS_MARKER, USERNAME_MARKER] {
        assert!(
            !contains(&bytes, marker.as_bytes()),
            "{marker} was stored in the clear"
        );
    }
}

#[test]
fn two_selected_entries_become_two_hosts() {
    let (_temp, vault) = unlocked_vault();
    let config = format!(
        "{}Host staging\n  HostName staging.example.com\n  User deploy\n",
        marker_config()
    );

    let summary = import(&vault, &config, &[0, 1]).expect("the import ran");

    assert_eq!(
        (summary.imported, summary.skipped, summary.failed),
        (2, 0, 0)
    );
    assert_eq!(summary.results.len(), 2);
    let saved = hosts(&vault);
    assert_eq!(
        saved
            .iter()
            .map(|host| host.label.as_str())
            .collect::<Vec<_>>(),
        vec![ALIAS_MARKER, "staging"],
        "the listing is ordered by label"
    );
    assert_eq!(saved[0].port, PORT);
    assert_eq!(saved[1].port, 22);
}

#[test]
fn the_selected_index_is_the_entry_the_preview_showed() {
    let (_temp, vault) = unlocked_vault();
    let config = "Host one\n  User u\n\nHost two\n  HostName two.example.com\n  User u\n  Port 2200\n\nHost three\n  User u\n";

    let previewed = preview(&vault, config).expect("the config parsed");
    assert_eq!(
        previewed
            .entries
            .iter()
            .map(|entry| (entry.index, entry.label.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "one"), (1, "two"), (2, "three")]
    );

    // The middle one, so a shifted index would save the wrong host.
    let summary = import(&vault, config, &[1]).expect("the import ran");

    assert_eq!(summary.imported, 1);
    assert_eq!(summary.results[0].index, 1);
    assert_eq!(summary.results[0].label, "two");
    let saved = hosts(&vault);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].label, "two");
    assert_eq!(saved[0].address, "two.example.com");
    assert_eq!(saved[0].port, 2200);
}

#[test]
fn importing_the_same_config_again_adds_nothing() {
    let (temp, vault) = unlocked_vault();

    assert_eq!(import(&vault, &marker_config(), &[0]).unwrap().imported, 1);
    let after_first = database_state(&temp.database());

    let again = preview(&vault, &marker_config()).expect("the config parsed");
    assert_eq!(again.entries[0].status, EntryStatus::AlreadyExists);
    assert!(
        notes(&again)
            .iter()
            .any(|note| note.contains("already has this address")),
        "a duplicate was not explained: {:?}",
        notes(&again)
    );

    let summary = import(&vault, &marker_config(), &[0]).expect("the import ran");
    assert_eq!(
        (summary.imported, summary.skipped, summary.failed),
        (0, 1, 0)
    );
    assert_eq!(summary.results[0].outcome, ImportOutcome::Skipped);
    assert_eq!(summary.results[0].host_id, None);

    assert_eq!(hosts(&vault).len(), 1, "the same host was saved twice");
    assert_eq!(
        database_state(&temp.database()),
        after_first,
        "a second import of the same config wrote to the database"
    );
}

#[test]
fn a_host_that_was_deleted_is_not_an_existing_host() {
    let (temp, vault) = unlocked_vault();
    let deleted = create_host_record(
        &vault,
        &input("Old prod", ADDRESS_MARKER, PORT, USERNAME_MARKER),
    )
    .unwrap();
    vault.delete_object(&deleted.id).unwrap();
    let with_tombstone = database_state(&temp.database());

    let previewed = preview(&vault, &marker_config()).expect("the config parsed");
    assert_eq!(
        previewed.entries[0].status,
        EntryStatus::Ready,
        "a tombstone is not an existing host"
    );

    assert_eq!(import(&vault, &marker_config(), &[0]).unwrap().imported, 1);

    // The import added one host and removed nothing: the tombstone is untouched.
    let after = database_state(&temp.database());
    assert_eq!(after.len(), with_tombstone.len() + 1);
    assert!(
        after
            .iter()
            .any(|(id, _, deleted_at, _)| id == &deleted.id && deleted_at.is_some()),
        "the import touched a tombstone"
    );
}

#[test]
fn an_existing_host_is_never_overwritten() {
    let (_temp, vault) = unlocked_vault();
    let credential = create_password_record(&vault, "Prod key", "marker-secret-6d3f").unwrap();
    let group = create_group_record(&vault, "Work").unwrap();
    let saved = create_host_record(
        &vault,
        &HostInput {
            label: "My own label".to_string(),
            address: ADDRESS_MARKER.to_string(),
            port: i64::from(PORT),
            username: USERNAME_MARKER.to_string(),
            group_id: Some(group.id),
            credential_id: Some(credential.id),
        },
    )
    .unwrap();

    let previewed = preview(&vault, &marker_config()).expect("the config parsed");
    assert_eq!(previewed.entries[0].status, EntryStatus::AlreadyExists);

    let summary = import(&vault, &marker_config(), &[0]).expect("the import ran");
    assert_eq!(
        (summary.imported, summary.skipped, summary.failed),
        (0, 1, 0)
    );

    // Exactly the host that was already there: its label, its group and its
    // credential are all as they were.
    assert_eq!(hosts(&vault), vec![saved]);
}

#[test]
fn a_shared_label_adds_a_second_host_and_overwrites_nothing() {
    let (_temp, vault) = unlocked_vault();
    let saved = create_host_record(
        &vault,
        &input(ALIAS_MARKER, "other.example.com", 22, "someone"),
    )
    .unwrap();

    let previewed = preview(&vault, &marker_config()).expect("the config parsed");
    assert_eq!(
        previewed.entries[0].status,
        EntryStatus::Ready,
        "a different target is not a duplicate"
    );
    assert!(
        notes(&previewed)
            .iter()
            .any(|note| note.contains("already uses this label")),
        "a reused label was not mentioned: {:?}",
        notes(&previewed)
    );

    assert_eq!(import(&vault, &marker_config(), &[0]).unwrap().imported, 1);

    let all = hosts(&vault);
    assert_eq!(all.len(), 2);
    let mut addresses: Vec<&str> = all.iter().map(|host| host.address.as_str()).collect();
    addresses.sort_unstable();
    assert_eq!(addresses, vec![ADDRESS_MARKER, "other.example.com"]);
    assert_eq!(
        all.iter().find(|host| host.id == saved.id),
        Some(&saved),
        "the saved host was changed"
    );
}

#[test]
fn a_selection_that_cannot_be_honoured_writes_nothing() {
    let (temp, vault) = unlocked_vault();
    let before = database_state(&temp.database());
    let config = marker_config();

    for (selected, expected) in [
        (vec![1], "The config has no entry at position 1."),
        (vec![0, 0], "Entry 0 was selected twice."),
        (vec![], "Select at least one host to import."),
    ] {
        let error = import(&vault, &config, &selected)
            .expect_err("the selection cannot be honoured by any import");
        assert_eq!(error.to_string(), expected);
    }

    assert_eq!(
        database_state(&temp.database()),
        before,
        "a refused import still wrote something"
    );
}

#[test]
fn an_entry_that_cannot_be_imported_is_skipped_and_writes_nothing() {
    let (temp, vault) = unlocked_vault();
    let before = database_state(&temp.database());
    let config = "Host incomplete\n  HostName incomplete.example.com\n\nHost *\n  User root\n\nHost good\n  HostName good.example.com\n  User deploy\n";

    let previewed = preview(&vault, config).expect("the config parsed");
    assert_eq!(
        previewed
            .entries
            .iter()
            .map(|entry| entry.status)
            .collect::<Vec<_>>(),
        vec![
            EntryStatus::Invalid,
            EntryStatus::Unsupported,
            EntryStatus::Ready
        ]
    );

    let refused = import(&vault, config, &[0, 1]).expect("the import ran");
    assert_eq!(
        (refused.imported, refused.skipped, refused.failed),
        (0, 2, 0)
    );
    assert!(
        refused
            .results
            .iter()
            .all(|entry| entry.reason.is_some() && entry.host_id.is_none())
    );
    assert_eq!(
        database_state(&temp.database()),
        before,
        "an entry that cannot be imported was written"
    );

    // The same config, now with the good entry selected as well.
    let accepted = import(&vault, config, &[0, 1, 2]).expect("the import ran");
    assert_eq!(
        (accepted.imported, accepted.skipped, accepted.failed),
        (1, 2, 0)
    );
    let saved = hosts(&vault);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].label, "good");
}

#[test]
fn a_locked_vault_refuses_a_preview_and_an_import() {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).expect("the vault was created");
    vault.lock();

    for error in [
        preview(&vault, &marker_config()).expect_err("the vault is locked"),
        import(&vault, &marker_config(), &[0]).expect_err("the vault is locked"),
    ] {
        assert!(
            matches!(error, OpenSshError::Vault(VaultError::Locked)),
            "unexpected error: {error:?}"
        );
    }
}

#[test]
fn an_empty_or_oversized_config_is_refused_before_anything_else() {
    let (_temp, vault) = unlocked_vault();

    for content in ["", "   \n\t\n"] {
        let error = preview(&vault, content).expect_err("an empty config cannot be parsed");
        assert_eq!(error.to_string(), "Paste a config or choose a file first.");
        assert!(matches!(
            import(&vault, content, &[0]),
            Err(OpenSshError::Invalid(_))
        ));
    }

    let oversized = format!("{}{}", marker_config(), "#".repeat(MAX_CONFIG_BYTES));
    assert!(oversized.len() > MAX_CONFIG_BYTES);
    let error = preview(&vault, &oversized).expect_err("an oversized config is refused");
    assert_eq!(
        error.to_string(),
        format!("The config is larger than {MAX_CONFIG_BYTES} bytes.")
    );
    assert!(matches!(
        import(&vault, &oversized, &[0]),
        Err(OpenSshError::Invalid(_))
    ));
}

#[test]
fn no_error_and_no_note_repeats_a_value_from_the_config() {
    let (_temp, vault) = unlocked_vault();
    let content = format!(
        "Host {ALIAS_MARKER}\n  HostName {ADDRESS_MARKER}\n  User {USERNAME_MARKER}\n\
         \x20 Port 70000\n  IdentityFile ~/.ssh/{ALIAS_MARKER}.key\n\
         \x20 FancyDirective {ALIAS_MARKER}\n  ProxyCommand scp {ALIAS_MARKER}\n\
         Include {ALIAS_MARKER}.conf\n"
    );

    let previewed = preview(&vault, &content).expect("the config parsed");
    let notes = notes(&previewed);

    // The preview shows the values as the entry's own fields; the notes that
    // explain what was skipped must not repeat them.
    assert!(
        notes.len() >= 4,
        "the config was expected to produce notes: {notes:?}"
    );
    for note in &notes {
        for marker in [ALIAS_MARKER, ADDRESS_MARKER, USERNAME_MARKER] {
            assert!(
                !note.contains(marker),
                "{marker} was repeated in a note: {note}"
            );
        }
    }

    let failures = [
        preview(&vault, "").unwrap_err(),
        preview(
            &vault,
            &format!("{content}{}", "x".repeat(MAX_CONFIG_BYTES)),
        )
        .unwrap_err(),
        import(&vault, &content, &[9]).unwrap_err(),
        import(&vault, &content, &[0, 0]).unwrap_err(),
        import(&vault, &content, &[]).unwrap_err(),
    ];
    for error in failures {
        let message = error.to_string();
        for marker in [ALIAS_MARKER, ADDRESS_MARKER, USERNAME_MARKER] {
            assert!(
                !message.contains(marker),
                "{marker} was repeated in an error: {message}"
            );
        }
    }
}

#[test]
fn an_import_creates_hosts_and_nothing_else() {
    let (_temp, vault) = unlocked_vault();
    let content = format!(
        "Host {ALIAS_MARKER}\n  HostName {ADDRESS_MARKER}\n  User {USERNAME_MARKER}\n\
         \x20 Port {PORT}\n  IdentityFile ~/.ssh/id_ed25519\n\
         \x20 CertificateFile ~/.ssh/id_ed25519-cert.pub\n\
         \x20 IdentityAgent SSH_AUTH_SOCK\n\
         \x20 PreferredAuthentications publickey\n"
    );

    let previewed = preview(&vault, &content).expect("the config parsed");
    assert_eq!(
        previewed.entries[0].status,
        EntryStatus::Ready,
        "a skipped directive does not block the host"
    );
    let notes = notes(&previewed);
    for name in [
        "IdentityFile",
        "CertificateFile",
        "IdentityAgent",
        "PreferredAuthentications",
    ] {
        assert!(
            notes.iter().any(|note| note.contains(name)),
            "{name} was not reported: {notes:?}"
        );
    }

    assert_eq!(import(&vault, &content, &[0]).unwrap().imported, 1);

    // A host, and no other kind of record at all.
    assert_eq!(vault.list_objects(Some("host")).unwrap().len(), 1);
    for kind in ["credential", "group", "known_host"] {
        assert!(
            vault.list_objects(Some(kind)).unwrap().is_empty(),
            "an import created a {kind}"
        );
    }
    assert_eq!(
        hosts(&vault)[0].credential_id,
        None,
        "an import linked a credential"
    );
}

#[test]
fn an_imported_host_is_an_ordinary_host_that_can_be_edited_and_linked() {
    let (_temp, vault) = unlocked_vault();
    assert_eq!(import(&vault, &marker_config(), &[0]).unwrap().imported, 1);
    let imported = hosts(&vault).remove(0);

    // The stored payload has exactly the fields of a host: nothing on the record
    // says where it came from, and none of the config's syntax is kept.
    let stored = vault.get_object(&imported.id).expect("the node was read");
    assert_eq!(stored.kind, "host");
    let value: serde_json::Value = serde_json::from_str(&stored.payload).expect("a payload");
    let mut fields: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        vec![
            "address",
            "createdAt",
            "credentialId",
            "groupId",
            "id",
            "label",
            "port",
            "updatedAt",
            "username",
            "version"
        ],
        "the host payload is not the ordinary one"
    );

    // It can be edited and linked to a credential like any other host.
    let credential = create_password_record(&vault, "Prod", "marker-secret-6d3f").unwrap();
    let group = create_group_record(&vault, "Work").unwrap();
    let edited = update_host_record(
        &vault,
        &imported.id,
        &HostInput {
            label: "Prod web".to_string(),
            address: imported.address.clone(),
            port: i64::from(imported.port),
            username: imported.username.clone(),
            group_id: Some(group.id),
            credential_id: Some(credential.id.clone()),
        },
    )
    .expect("the imported host was updated");

    assert_eq!(edited.id, imported.id);
    assert_eq!(edited.credential_id, Some(credential.id));
    assert_eq!(
        edited.created_at, imported.created_at,
        "the edit moved the creation time"
    );
}

// --- the duplicate check --------------------------------------------------

fn entry(address: &str, port: Option<u16>, username: &str) -> ImportEntry {
    ImportEntry {
        index: 0,
        label: "label".to_string(),
        address: address.to_string(),
        port,
        username: username.to_string(),
        status: EntryStatus::Ready,
        warnings: vec![],
    }
}

#[test]
fn the_duplicate_check_folds_names_trims_values_and_leaves_literals_alone() {
    let (_temp, vault) = unlocked_vault();
    create_host_record(&vault, &input("Prod", "Prod.Example.COM", 22, "Deploy")).unwrap();
    create_host_record(&vault, &input("Ipv4", "192.0.2.10", 22, "root")).unwrap();
    create_host_record(&vault, &input("Ipv6", "[2001:db8::1]", 22, "root")).unwrap();
    let saved = hosts(&vault);

    // A name is not case-sensitive, and whitespace around a value is not part of
    // it.
    assert!(same_target(
        &entry("  prod.example.com  ", Some(22), " Deploy "),
        &saved
    ));
    assert!(same_target(
        &entry("PROD.EXAMPLE.COM", Some(22), "Deploy"),
        &saved
    ));

    // A different port, a different username or a different name is a different
    // host. A username is case-sensitive.
    assert!(!same_target(
        &entry("prod.example.com", Some(2222), "Deploy"),
        &saved
    ));
    assert!(!same_target(
        &entry("prod.example.com", Some(22), "deploy"),
        &saved
    ));
    assert!(!same_target(
        &entry("other.example.com", Some(22), "Deploy"),
        &saved
    ));
    assert!(
        !same_target(&entry("prod.example.com", None, "Deploy"), &saved),
        "an entry without a usable port cannot match anything"
    );

    // An IP literal is compared as it is written, in both spellings OpenSSH
    // allows for an IPv6 address.
    assert!(same_target(&entry("192.0.2.10", Some(22), "root"), &saved));
    assert!(same_target(
        &entry("[2001:db8::1]", Some(22), "root"),
        &saved
    ));
    assert!(
        !same_target(&entry("2001:db8::1", Some(22), "root"), &saved),
        "the two spellings of one address are not folded together"
    );
    assert!(!same_target(&entry("192.0.2.11", Some(22), "root"), &saved));
}

// --- what this module is not allowed to do --------------------------------

/// The parser and the import are pure over the text they are given: they read no
/// file, run no process, expand no environment, resolve no name and reach no
/// session. The modules' own source is pinned here, so an import path that grows
/// a `File::open`, a `Command::new` or a session handle fails a test instead of a
/// review.
#[test]
fn nothing_here_opens_a_file_runs_a_command_or_opens_a_session() {
    let sources = [
        include_str!("mod.rs"),
        include_str!("model.rs"),
        include_str!("parser.rs"),
        include_str!("service.rs"),
    ];
    let forbidden = [
        "std::fs",
        "File::open",
        "File::create",
        "fs::read",
        "std::process",
        "Command::new",
        "std::env",
        "TcpStream",
        "TcpListener",
        "SshState",
        "crate::ssh",
        "reqwest",
    ];

    for source in sources {
        // Only the part of the file before its tests is the code that runs.
        let code = source
            .split("#[cfg(test)]")
            .next()
            .expect("every source has a first part");
        for needle in forbidden {
            assert!(
                !code.contains(needle),
                "the OpenSSH import must not use {needle}"
            );
        }
    }
}

/// A preview costs one read of the saved hosts, and the parser itself is a
/// single pass over the text. This is the shape of a config at the size limit.
#[test]
fn a_config_at_the_size_limit_is_handled_in_one_pass() {
    let (_temp, vault) = unlocked_vault();
    let mut content = String::new();
    let mut index = 0;
    while content.len() + 64 < MAX_CONFIG_BYTES {
        content.push_str(&format!("Host host-{index}\n  User deploy\n"));
        index += 1;
    }

    let previewed = preview(&vault, &content).expect("the config parsed");
    assert!(previewed.entries.len() > 10_000);
    assert_eq!(
        previewed.entries.last().unwrap().index,
        previewed.entries.len() - 1
    );
    assert!(previewed.warnings.is_empty());

    // Deterministic: the same content gives the same entries and the same
    // indexes as the parser used by the import.
    assert_eq!(parse(&content), previewed);
}
