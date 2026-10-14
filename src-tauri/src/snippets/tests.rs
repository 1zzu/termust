//! Behaviour tests for snippets.
//!
//! Every test runs against its own temporary vault database, created with the
//! cheap Argon2id parameters, and removes it afterwards. No test touches a real
//! vault, the app data directory, or a real SSH session.

use std::fs;
use std::path::Path;

use rusqlite::{Connection, params};

use crate::ssh::SshState;
use crate::vault::test_support::TempVault;
use crate::vault::{Vault, VaultError, VaultStatus};

use super::model::{KIND_SNIPPET, MAX_CONTENT_LEN, MAX_NAME_LEN, SnippetInput};
use super::{SnippetError, send_content, service};

/// The master password every test creates its vault with.
const PASSWORD: &str = "a-strong-master-password";

/// Distinctive values for the plaintext scan. They must not appear anywhere in
/// the database file.
const NAME_MARKER: &str = "marker-snippet-name-4e7a";
const CONTENT_MARKER: &str = "marker-snippet-content-9c2b";

/// A temporary vault that has been created and is left unlocked.
fn unlocked_vault() -> (TempVault, Vault) {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).expect("the vault was created");
    (temp, vault)
}

fn input(name: &str, content: &str) -> SnippetInput {
    SnippetInput {
        name: name.to_string(),
        content: content.to_string(),
    }
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

// --- CRUD -----------------------------------------------------------------

#[test]
fn snippets_can_be_created_listed_read_updated_and_deleted() {
    let (_temp, vault) = unlocked_vault();

    let mut created = service::create_snippet(&vault, &input("Deploy", "git pull")).unwrap();
    assert_eq!(created.name, "Deploy");
    assert_eq!(created.content, "git pull");
    assert_eq!(created.version, 1);
    assert_eq!(created.created_at, created.updated_at);

    let listed = service::list_snippets(&vault).unwrap();
    assert_eq!(listed, vec![created.clone()]);
    assert_eq!(service::get_snippet(&vault, &created.id).unwrap(), created);

    created.updated_at = 0;
    let updated =
        service::update_snippet(&vault, &created.id, &input("Deploy", "git pull && make")).unwrap();
    assert_eq!(updated.id, created.id, "the id changed");
    assert_eq!(
        updated.created_at, created.created_at,
        "the creation time changed"
    );
    assert_eq!(updated.content, "git pull && make");
    assert_eq!(updated.name, "Deploy");

    service::delete_snippet(&vault, &updated.id).unwrap();
    assert!(service::list_snippets(&vault).unwrap().is_empty());
}

#[test]
fn a_snippet_name_is_trimmed_and_the_content_is_left_exactly_as_written() {
    let (_temp, vault) = unlocked_vault();
    let content = "  echo '  spaced  '  \n\tquoted \"text\"\\\n";

    let created = service::create_snippet(&vault, &input("  Deploy  ", content)).unwrap();

    assert_eq!(created.name, "Deploy", "the name was not trimmed");
    assert_eq!(created.content, content, "the content was changed");

    // The same has to hold across an update.
    let updated = service::update_snippet(&vault, &created.id, &input("Deploy", content)).unwrap();
    assert_eq!(updated.content, content);
}

#[test]
fn listing_is_ordered_by_name_case_insensitively_and_then_by_id() {
    let (_temp, vault) = unlocked_vault();

    service::create_snippet(&vault, &input("deploy", "a")).unwrap();
    service::create_snippet(&vault, &input("Backup", "b")).unwrap();
    // Two snippets may share a name: the id decides between them.
    let first = service::create_snippet(&vault, &input("cleanup", "c")).unwrap();
    let second = service::create_snippet(&vault, &input("cleanup", "d")).unwrap();

    let listed = service::list_snippets(&vault).unwrap();
    let names: Vec<&str> = listed.iter().map(|snippet| snippet.name.as_str()).collect();
    assert_eq!(names, vec!["Backup", "cleanup", "cleanup", "deploy"]);

    let mut expected = [first.id.as_str(), second.id.as_str()];
    expected.sort();
    assert_eq!(
        vec![listed[1].id.as_str(), listed[2].id.as_str()],
        expected.to_vec()
    );

    // And the order is the same on a second read.
    let again = service::list_snippets(&vault).unwrap();
    assert_eq!(
        again
            .iter()
            .map(|snippet| snippet.id.clone())
            .collect::<Vec<_>>(),
        listed
            .iter()
            .map(|snippet| snippet.id.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_deleted_snippet_leaves_a_tombstone_and_disappears_from_the_list() {
    let (temp, vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input("Deploy", "git pull")).unwrap();

    service::delete_snippet(&vault, &snippet.id).unwrap();

    let deleted_at: Option<i64> = Connection::open(temp.database())
        .unwrap()
        .query_row(
            "SELECT deleted_at FROM vault_objects WHERE id = ?1",
            params![snippet.id],
            |row| row.get(0),
        )
        .expect("the row is still there");
    assert!(
        deleted_at.is_some(),
        "the row was removed instead of marked deleted"
    );

    assert!(service::list_snippets(&vault).unwrap().is_empty());
    assert!(
        matches!(
            service::get_snippet(&vault, &snippet.id),
            Err(SnippetError::NotFound)
        ),
        "a deleted snippet must not be readable"
    );
    assert!(matches!(
        service::delete_snippet(&vault, &snippet.id),
        Err(SnippetError::NotFound)
    ));
}

#[test]
fn snippets_survive_a_restart_and_unlock() {
    let (temp, vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input(NAME_MARKER, CONTENT_MARKER)).unwrap();
    drop(vault);

    let mut reopened = temp.vault();
    assert_eq!(reopened.status().unwrap(), VaultStatus::Locked);
    reopened.unlock(PASSWORD).unwrap();

    let kept = service::list_snippets(&reopened).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].id, snippet.id);
    assert_eq!(kept[0].name, NAME_MARKER);
    assert_eq!(kept[0].content, CONTENT_MARKER);
}

#[test]
fn snippet_operations_are_refused_while_locked() {
    let (_temp, mut vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input("Deploy", "git pull")).unwrap();
    vault.lock();

    let results = [
        service::list_snippets(&vault).map(|_| ()),
        service::get_snippet(&vault, &snippet.id).map(|_| ()),
        service::create_snippet(&vault, &input("Other", "ls")).map(|_| ()),
        service::update_snippet(&vault, &snippet.id, &input("Other", "ls")).map(|_| ()),
        service::delete_snippet(&vault, &snippet.id),
    ];
    for result in results {
        assert!(
            matches!(result, Err(SnippetError::Vault(VaultError::Locked))),
            "a locked vault answered with {result:?}"
        );
    }
}

// --- validation -----------------------------------------------------------

#[test]
fn a_blank_name_or_empty_content_is_refused() {
    let (_temp, vault) = unlocked_vault();

    for blank in ["", "   ", "\n\t"] {
        let error = service::create_snippet(&vault, &input(blank, "git pull")).unwrap_err();
        assert!(
            matches!(error, SnippetError::Invalid(_)),
            "blank name {blank:?} produced {error:?}"
        );
    }

    let error = service::create_snippet(&vault, &input("Deploy", "")).unwrap_err();
    assert!(matches!(error, SnippetError::Invalid(_)));
    assert_eq!(error.to_string(), "Snippet content cannot be empty.");

    assert!(service::list_snippets(&vault).unwrap().is_empty());
}

#[test]
fn a_name_longer_than_the_limit_is_refused() {
    let (_temp, vault) = unlocked_vault();

    let too_long = "a".repeat(MAX_NAME_LEN + 1);
    assert!(matches!(
        service::create_snippet(&vault, &input(&too_long, "ls")),
        Err(SnippetError::Invalid(_))
    ));

    let exactly = "a".repeat(MAX_NAME_LEN);
    assert!(service::create_snippet(&vault, &input(&exactly, "ls")).is_ok());
}

#[test]
fn content_over_the_limit_is_refused() {
    let (_temp, vault) = unlocked_vault();

    let too_long = "a".repeat(MAX_CONTENT_LEN + 1);
    let error = service::create_snippet(&vault, &input("Deploy", &too_long)).unwrap_err();
    assert!(
        matches!(error, SnippetError::Invalid(_)),
        "expected a validation error, got {error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!("Snippet content is longer than {MAX_CONTENT_LEN} bytes.")
    );

    // The exact limit is still accepted, and it is a byte limit: 64 KiB of
    // multi-byte characters is refused even though it is fewer characters.
    let exactly = "a".repeat(MAX_CONTENT_LEN);
    assert!(service::create_snippet(&vault, &input("Deploy", &exactly)).is_ok());

    let multi_byte = "é".repeat(MAX_CONTENT_LEN / 2 + 1);
    assert!(matches!(
        service::create_snippet(&vault, &input("Deploy", &multi_byte)),
        Err(SnippetError::Invalid(_))
    ));
}

#[test]
fn a_nul_byte_in_the_content_is_refused() {
    let (_temp, vault) = unlocked_vault();

    let error = service::create_snippet(&vault, &input("Deploy", "echo\0ls")).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Snippet content cannot contain a NUL character."
    );
}

#[test]
fn validation_happens_before_anything_is_stored() {
    let (_temp, vault) = unlocked_vault();

    assert!(service::create_snippet(&vault, &input("   ", "ls")).is_err());
    assert!(service::list_snippets(&vault).unwrap().is_empty());
    assert_eq!(
        vault.list_objects(Some(KIND_SNIPPET)).unwrap().len(),
        0,
        "a refused snippet left an object behind"
    );
}

#[test]
fn an_update_that_fails_leaves_the_stored_snippet_alone() {
    let (_temp, vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input("Deploy", "git pull")).unwrap();

    assert!(service::update_snippet(&vault, &snippet.id, &input("Deploy", "")).is_err());
    assert!(service::update_snippet(&vault, &snippet.id, &input("   ", "ls")).is_err());

    assert_eq!(service::get_snippet(&vault, &snippet.id).unwrap(), snippet);
}

// --- payload and storage --------------------------------------------------

#[test]
fn an_unsupported_payload_version_is_reported_clearly() {
    let (_temp, vault) = unlocked_vault();
    vault
        .put_object(
            KIND_SNIPPET,
            r#"{"version":99,"id":"x","name":"Future","content":"ls","createdAt":1,"updatedAt":1}"#,
        )
        .unwrap();

    match service::list_snippets(&vault).unwrap_err() {
        SnippetError::UnsupportedPayloadVersion { found, supported } => {
            assert_eq!(found, 99);
            assert_eq!(supported, 1);
        }
        other => panic!("expected an unsupported version error, got {other:?}"),
    }
}

#[test]
fn an_object_without_a_version_is_reported_as_corrupted() {
    let (_temp, vault) = unlocked_vault();
    vault
        .put_object(KIND_SNIPPET, r#"{"name":"no version here"}"#)
        .unwrap();

    assert!(matches!(
        service::list_snippets(&vault).unwrap_err(),
        SnippetError::Corrupt
    ));
}

#[test]
fn a_snippet_cannot_be_read_as_another_kind() {
    let (_temp, vault) = unlocked_vault();
    let group = vault
        .put_object(
            "group",
            r#"{"version":1,"id":"x","name":"Work","createdAt":1,"updatedAt":1}"#,
        )
        .unwrap();

    assert!(matches!(
        service::get_snippet(&vault, &group),
        Err(SnippetError::WrongKind {
            expected: "snippet"
        })
    ));
}

#[test]
fn a_tampered_snippet_is_reported_as_corrupted_without_its_plaintext() {
    let (temp, vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input(NAME_MARKER, CONTENT_MARKER)).unwrap();

    flip_payload_byte(&temp.database(), &snippet.id);

    let error = service::get_snippet(&vault, &snippet.id).unwrap_err();
    assert!(
        matches!(error, SnippetError::Vault(VaultError::Corrupt(_))),
        "unexpected error: {error:?}"
    );
    assert!(!error.to_string().contains(CONTENT_MARKER));
    assert!(!error.to_string().contains(NAME_MARKER));
}

#[test]
fn the_database_holds_no_plaintext_snippet_data() {
    let (temp, vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input(NAME_MARKER, CONTENT_MARKER)).unwrap();
    let deleted =
        service::create_snippet(&vault, &input("old", "old-marker-content-1f4d")).unwrap();
    service::delete_snippet(&vault, &deleted.id).unwrap();

    // The record really is there after decryption...
    assert_eq!(
        service::get_snippet(&vault, &snippet.id).unwrap().content,
        CONTENT_MARKER
    );

    // ...but nothing readable is left in the database file, including in the
    // tombstone that a delete leaves behind.
    let bytes = fs::read(temp.database()).unwrap();
    for marker in [NAME_MARKER, CONTENT_MARKER, "old-marker-content-1f4d"] {
        assert!(
            !contains(&bytes, marker.as_bytes()),
            "{marker} was stored in the clear"
        );
    }
}

#[test]
fn snippet_errors_never_repeat_the_name_or_the_content() {
    let (_temp, vault) = unlocked_vault();
    let snippet = service::create_snippet(&vault, &input(NAME_MARKER, CONTENT_MARKER)).unwrap();

    // Every failing operation is a different one, and the content is a valid
    // string containing the marker, so nothing else can be blamed for it.
    let failures = [
        service::create_snippet(&vault, &input(NAME_MARKER, &format!("{CONTENT_MARKER}\0")))
            .unwrap_err(),
        service::create_snippet(&vault, &input(NAME_MARKER, &CONTENT_MARKER.repeat(8_000)))
            .unwrap_err(),
        service::create_snippet(&vault, &input(NAME_MARKER, "")).unwrap_err(),
        service::create_snippet(&vault, &input("   ", CONTENT_MARKER)).unwrap_err(),
        service::update_snippet(&vault, "not-a-real-id", &input(NAME_MARKER, CONTENT_MARKER))
            .unwrap_err(),
        service::delete_snippet(&vault, "not-a-real-id").unwrap_err(),
    ];
    assert_eq!(failures.len(), 6, "every operation really failed");

    for error in failures {
        let message = error.to_string();
        assert!(
            !message.contains(CONTENT_MARKER),
            "an error repeated the content: {message}"
        );
    }

    // The stored snippet is untouched by all of that.
    assert_eq!(service::get_snippet(&vault, &snippet.id).unwrap(), snippet);
}

// --- running one ----------------------------------------------------------

/// The send half is synchronous and takes no vault handle, so the vault lock
/// cannot be held while a snippet is being sent: `run` finishes its one read
/// before this function is called. Driving it here with a session state that has
/// no vault anywhere near it pins that down.
#[test]
fn sending_a_snippet_writes_the_content_and_one_submit_keystroke() {
    let ssh = SshState::default();
    let recorded = ssh.record_input("session-1");

    send_content(&ssh, "session-1", "git pull").expect("the session accepted the snippet");

    assert_eq!(
        recorded.take(),
        vec!["git pull".to_string(), "\r".to_string()],
        "the content and exactly one submit keystroke were expected"
    );
}

#[test]
fn content_that_already_ends_with_a_line_break_gets_no_second_submit() {
    for content in [
        "git pull\n",
        "git pull\r",
        "git pull\r\n",
        "echo one\necho two\n",
    ] {
        let ssh = SshState::default();
        let recorded = ssh.record_input("session-1");

        send_content(&ssh, "session-1", content).expect("the session accepted the snippet");

        assert_eq!(
            recorded.take(),
            vec![content.to_string()],
            "{content:?} was given a second submit"
        );
    }
}

#[test]
fn a_multiline_snippet_is_sent_as_one_piece_plus_one_submit() {
    let ssh = SshState::default();
    let recorded = ssh.record_input("session-1");
    let content = "cd /srv\nsystemctl restart app";

    send_content(&ssh, "session-1", content).expect("the session accepted the snippet");

    assert_eq!(recorded.take(), vec![content.to_string(), "\r".to_string()]);
}

#[test]
fn a_snippet_only_reaches_the_session_it_was_given() {
    let ssh = SshState::default();
    let target = ssh.record_input("session-1");
    let other = ssh.record_input("session-2");

    send_content(&ssh, "session-1", "whoami").expect("the session accepted the snippet");

    assert_eq!(target.take().len(), 2);
    assert!(
        other.take().is_empty(),
        "a snippet reached the wrong session"
    );
}

#[test]
fn sending_a_snippet_without_a_session_is_refused() {
    let ssh = SshState::default();
    let recorded = ssh.record_input("session-1");

    let error =
        send_content(&ssh, "session-2", "whoami").expect_err("no session can accept a snippet");

    assert_eq!(error, "This session is not connected.");
    assert!(
        recorded.take().is_empty(),
        "a refused snippet still reached a session"
    );
}

/// A snippet is only ever sent by `send_content`, which nothing but `run_snippet`
/// calls. Creating, editing, listing, reading and unlocking therefore cannot run
/// one, which is what this test holds the CRUD to.
#[test]
fn creating_editing_listing_and_unlocking_never_send_anything() {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(PASSWORD).expect("the vault was created");
    let ssh = SshState::default();
    let recorded = ssh.record_input("session-1");

    let created = service::create_snippet(&vault, &input("Deploy", "echo marker")).unwrap();
    service::update_snippet(&vault, &created.id, &input("Deploy", "echo changed")).unwrap();
    assert_eq!(service::list_snippets(&vault).unwrap().len(), 1);
    let _ = service::get_snippet(&vault, &created.id).unwrap();
    service::delete_snippet(&vault, &created.id).unwrap();

    drop(vault);
    let mut reopened = temp.vault();
    reopened.unlock(PASSWORD).expect("the vault unlocked");
    assert!(service::list_snippets(&reopened).unwrap().is_empty());

    assert!(
        recorded.take().is_empty(),
        "something sent a snippet without being asked to"
    );
}

/// The keyboard path is the one a snippet borrows: a session that accepts typed
/// input still receives it unchanged.
#[test]
fn terminal_keyboard_input_still_reaches_the_session() {
    let ssh = SshState::default();
    let recorded = ssh.record_input("session-1");

    crate::ssh::write(&ssh, "session-1", "ls -la\r").expect("the session accepted typed input");

    assert_eq!(recorded.take(), vec!["ls -la\r".to_string()]);
}
