//! Termust desktop shell.
//!
//! The frontend owns the screens; this layer only exposes the SSH session
//! commands it needs and keeps the session state alive between calls.

mod inventory;
mod snippets;
mod ssh;
// The vault is a self-contained core the app builds on; keeping it public lets
// its API be used and tested directly, without going through the frontend.
pub mod vault;

use tauri::{AppHandle, Manager, Runtime, State};

use ssh::{ConnectRequest, SavedConnectRequest, SshState};
use vault::{Vault, VaultState};

#[tauri::command]
fn connect_ssh<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SshState>,
    request: ConnectRequest,
) -> Result<(), String> {
    ssh::start(&app, &state, request)
}

/// Opens a session for a saved host, using the credential stored beside it.
///
/// The frontend sends which host to use and how big the terminal is. The
/// address, the port, the username and the secret all come from the vault, so
/// nothing here can point the connection somewhere else, and no secret ever
/// crosses the IPC boundary in either direction.
#[tauri::command]
async fn start_saved_ssh<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, SshState>,
    request: SavedConnectRequest,
) -> Result<(), String> {
    ssh::start_saved(&app, &state, request).await
}

#[tauri::command]
fn write_ssh(state: State<'_, SshState>, session_id: String, data: String) -> Result<(), String> {
    ssh::write(&state, &session_id, &data)
}

#[tauri::command]
fn resize_ssh(
    state: State<'_, SshState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    ssh::resize(&state, &session_id, cols, rows)
}

#[tauri::command]
fn respond_host_key(
    state: State<'_, SshState>,
    session_id: String,
    accepted: bool,
) -> Result<(), String> {
    ssh::respond_host_key(&state, &session_id, accepted)
}

#[tauri::command]
async fn disconnect_ssh(state: State<'_, SshState>, session_id: String) -> Result<(), String> {
    ssh::stop(&state, &session_id).await;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .manage(SshState::default())
        .setup(|app| {
            // The vault database lives in the app data directory the operating
            // system defines for this app; the frontend never chooses a path.
            let directory = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("Could not resolve the app data directory: {error}"))?;
            app.manage(VaultState::new(Vault::new(directory.join(vault::DB_FILE))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect_ssh,
            start_saved_ssh,
            write_ssh,
            resize_ssh,
            respond_host_key,
            disconnect_ssh,
            vault::vault_status,
            vault::create_vault,
            vault::unlock_vault,
            vault::lock_vault,
            inventory::list_groups,
            inventory::create_group,
            inventory::rename_group,
            inventory::delete_group,
            inventory::list_hosts,
            inventory::get_host,
            inventory::create_host,
            inventory::update_host,
            inventory::delete_host,
            inventory::list_credentials,
            inventory::create_password_credential,
            inventory::create_private_key_credential,
            inventory::rename_credential,
            inventory::replace_password,
            inventory::replace_private_key,
            inventory::delete_credential,
            inventory::list_known_hosts,
            inventory::forget_known_host,
            snippets::list_snippets,
            snippets::create_snippet,
            snippets::update_snippet,
            snippets::delete_snippet,
            snippets::run_snippet
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        // Nothing is left to recover: report it on stderr and stop.
        eprintln!("Termust could not start: {error}");
        std::process::exit(1);
    }
}

/// The IPC contract with the frontend: the command names in `src/lib/ssh.ts` and
/// the camelCase argument names it sends are checked against the real handlers,
/// over Tauri's own command dispatch.
#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};

    use super::{
        connect_ssh, disconnect_ssh, resize_ssh, respond_host_key, start_saved_ssh, write_ssh,
    };
    use crate::inventory::{
        HostInput, KnownKey, create_host_record, create_password_record, remember_known_host,
    };
    use crate::ssh::SshState;
    use crate::vault::{VaultState, test_support::TempVault};

    /// A window over the session commands, wired exactly as the app wires them.
    ///
    /// `connect_ssh` is missing on purpose: it needs the session state and a
    /// concrete `AppHandle`, which a mock runtime cannot hand to a command. Its
    /// only argument is the nested request, whose field names are covered by the
    /// serde test in `ssh::tests`. The vault and inventory commands are missing
    /// for the same reason; their behaviour is covered against `vault::Vault`
    /// and the inventory service. The known-host commands are the exception:
    /// they are generic over the runtime exactly so this harness can drive them.
    /// Keep this list in step with `run`.
    fn mock_window() -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
        let app = mock_builder()
            .manage(SshState::default())
            .invoke_handler(tauri::generate_handler![
                write_ssh,
                resize_ssh,
                respond_host_key,
                disconnect_ssh
            ])
            .build(mock_context(noop_assets()))
            .expect("the app built");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("the window built");

        (app, webview)
    }

    /// The origin the webview sends its invocations from. Tauri serves the app
    /// from `tauri://localhost` on macOS and Linux and from
    /// `http://tauri.localhost` on Windows and Android, and it only trusts the
    /// commands of its own origin.
    fn app_origin() -> &'static str {
        if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
    }

    /// Invokes a command the way the webview does, and returns either its value
    /// or the error it reported.
    fn invoke(
        webview: &WebviewWindow<MockRuntime>,
        cmd: &str,
        args: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(
            webview,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: app_origin().parse().expect("a valid origin"),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().expect("a JSON response"))
    }

    #[test]
    fn terminal_input_for_an_unknown_session_is_reported() {
        let (_app, webview) = mock_window();

        let error = invoke(
            &webview,
            "write_ssh",
            json!({ "sessionId": "missing", "data": "ls\n" }),
        )
        .expect_err("no session can accept input");

        assert_eq!(error, json!("This session is not connected."));
    }

    #[test]
    fn a_resize_for_an_unknown_session_is_reported() {
        let (_app, webview) = mock_window();

        let error = invoke(
            &webview,
            "resize_ssh",
            json!({ "sessionId": "missing", "cols": 120, "rows": 40 }),
        )
        .expect_err("no session can be resized");

        assert_eq!(error, json!("This session is not connected."));
    }

    #[test]
    fn disconnecting_an_unknown_session_is_not_an_error() {
        let (_app, webview) = mock_window();

        let response = invoke(
            &webview,
            "disconnect_ssh",
            json!({ "sessionId": "missing" }),
        );

        assert!(response.is_ok(), "disconnect reported {response:?}");
    }

    #[test]
    fn answering_an_unknown_host_key_prompt_is_not_an_error() {
        let (_app, webview) = mock_window();

        let response = invoke(
            &webview,
            "respond_host_key",
            json!({ "sessionId": "missing", "accepted": false }),
        );

        assert!(response.is_ok(), "answering reported {response:?}");
    }

    const VAULT_PASSWORD: &str = "a-strong-master-password";

    /// A window over the known-host commands, with a real vault of its own on a
    /// temporary database, either left open or locked.
    fn known_host_window(open: bool) -> (App<MockRuntime>, WebviewWindow<MockRuntime>, TempVault) {
        let temp = TempVault::new();
        let mut vault = temp.vault();
        vault.create(VAULT_PASSWORD).expect("the vault was created");
        if !open {
            vault.lock();
        }

        let app = mock_builder()
            .manage(VaultState::new(vault))
            // Qualified on purpose: naming the handlers here means a rename
            // breaks this test at compile time, not at run time.
            .invoke_handler(tauri::generate_handler![
                crate::inventory::list_known_hosts,
                crate::inventory::forget_known_host
            ])
            .build(mock_context(noop_assets()))
            .expect("the app built");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("the window built");

        (app, webview, temp)
    }

    /// The exact payload the frontend receives, over Tauri's own dispatch. Both
    /// directions of this contract are pinned here: the command names, the
    /// argument names, and the camelCase field names of the response.
    #[test]
    fn the_known_host_commands_answer_over_ipc() {
        let (app, webview, _temp) = known_host_window(true);

        // Nothing is trusted before anything was approved.
        assert_eq!(
            invoke(&webview, "list_known_hosts", json!({})).expect("the list answered"),
            json!([])
        );

        // A record can only come from the SSH prompt: there is no command that
        // writes one, so the test writes it the way a session does.
        let key = KnownKey {
            algorithm: "ssh-ed25519".to_string(),
            fingerprint: "SHA256:abc".to_string(),
        };
        let record = app.state::<VaultState>().handle().with(|vault| {
            remember_known_host(vault, "example.com:22", &key).expect("the key was remembered")
        });

        let listed = invoke(&webview, "list_known_hosts", json!({})).expect("the list answered");
        assert_eq!(
            listed,
            json!([{
                "version": 1,
                "id": record.id,
                "hostKeyId": "example.com:22",
                "keyAlgorithm": "ssh-ed25519",
                "fingerprintSha256": "SHA256:abc",
                "createdAt": record.created_at,
                "updatedAt": record.updated_at,
            }]),
            "the frontend contract changed"
        );

        assert!(
            invoke(&webview, "forget_known_host", json!({ "id": record.id })).is_ok(),
            "forgetting a known host failed"
        );
        assert_eq!(
            invoke(&webview, "list_known_hosts", json!({})).expect("the list answered"),
            json!([]),
            "a forgotten host must not be listed"
        );

        // The argument name is part of the contract too: a renamed one is a
        // rejected call rather than a silent no-op.
        assert!(
            invoke(
                &webview,
                "forget_known_host",
                json!({ "knownHostId": "example.com:22" })
            )
            .is_err(),
            "an unknown argument name was accepted"
        );
    }

    #[test]
    fn the_known_host_commands_refuse_a_locked_vault() {
        let (_app, webview, _temp) = known_host_window(false);

        let error = invoke(&webview, "list_known_hosts", json!({})).expect_err("a locked vault");

        assert_eq!(error, json!("The vault is locked. Unlock it first."));
    }

    /// A secret that must never appear in any answer or payload. It is stored in
    /// the credential the saved host points at.
    const SAVED_PASSWORD: &str = "marker-password-8f2a";

    /// A window wired like the app for the saved-host command, over a vault of
    /// its own holding one saved host.
    ///
    /// `credential` decides whether that host points at a credential, so a test
    /// can ask for the "this host cannot connect yet" answer.
    fn saved_host_window(
        open: bool,
        credential: bool,
    ) -> (
        App<MockRuntime>,
        WebviewWindow<MockRuntime>,
        TempVault,
        String,
    ) {
        let temp = TempVault::new();
        let mut vault = temp.vault();
        vault.create(VAULT_PASSWORD).expect("the vault was created");

        // The records are written the way the commands write them, before the
        // vault is handed to the app, so a locked window is locked for real.
        let credential_id = credential.then(|| {
            create_password_record(&vault, "Prod", SAVED_PASSWORD)
                .expect("the credential was created")
                .id
        });
        let host = create_host_record(
            &vault,
            &HostInput {
                label: "Prod web".to_string(),
                address: "web.example.com".to_string(),
                port: 2222,
                username: "deploy".to_string(),
                group_id: None,
                credential_id,
            },
        )
        .expect("the host was created");

        if !open {
            vault.lock();
        }

        let app = mock_builder()
            .manage(SshState::default())
            .manage(VaultState::new(vault))
            .invoke_handler(tauri::generate_handler![
                connect_ssh,
                start_saved_ssh,
                write_ssh,
                disconnect_ssh
            ])
            .build(mock_context(noop_assets()))
            .expect("the app built");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("the window built");

        (app, webview, temp, host.id)
    }

    fn saved_request(host_id: &str) -> Value {
        json!({ "request": { "sessionId": "saved-1", "hostId": host_id, "cols": 100, "rows": 30 } })
    }

    /// Connecting a saved host carries an id and a terminal size, and nothing
    /// else: a payload that tries to bring a secret, an address or a credential
    /// is refused rather than quietly ignored.
    #[test]
    fn the_saved_host_payload_carries_no_secret_over_ipc() {
        let (_app, webview, _temp, host_id) = saved_host_window(true, true);

        for (extra, why) in [
            (json!({ "password": "hunter2" }), "a password"),
            (json!({ "privateKey": "key-material" }), "a private key"),
            (json!({ "passphrase": "unlock" }), "a passphrase"),
            (json!({ "address": "evil.example.com" }), "an address"),
            (json!({ "port": 22 }), "a port"),
            (json!({ "username": "root" }), "a username"),
            (json!({ "credentialId": "other" }), "a credential id"),
            (json!({ "auth": "password" }), "an auth method"),
        ] {
            let mut request = json!({
                "sessionId": "saved-1",
                "hostId": host_id,
                "cols": 100,
                "rows": 30
            });
            request
                .as_object_mut()
                .expect("the request is an object")
                .extend(
                    extra
                        .as_object()
                        .expect("the extra field is an object")
                        .clone(),
                );

            let error =
                invoke(&webview, "start_saved_ssh", json!({ "request": request })).expect_err(why);
            assert!(
                !error.to_string().contains(SAVED_PASSWORD),
                "{why} leaked into {error}"
            );
        }

        // The argument name is part of the contract too.
        assert!(
            invoke(
                &webview,
                "start_saved_ssh",
                json!({ "savedHost": { "sessionId": "saved-1" } })
            )
            .is_err(),
            "a renamed argument was accepted"
        );
        assert!(
            invoke(
                &webview,
                "start_saved_ssh",
                json!({ "request": { "sessionId": "saved-1", "cols": 100, "rows": 30 } })
            )
            .is_err(),
            "a request without a host id was accepted"
        );
    }

    #[test]
    fn the_saved_host_command_refuses_a_locked_vault() {
        let (_app, webview, _temp, host_id) = saved_host_window(false, true);

        let error = invoke(&webview, "start_saved_ssh", saved_request(&host_id))
            .expect_err("a locked vault cannot open a saved host");

        assert_eq!(error, json!("The vault is locked. Unlock it first."));
    }

    #[test]
    fn the_saved_host_command_refuses_an_unknown_or_deleted_host() {
        let (_app, webview, _temp, _host_id) = saved_host_window(true, true);

        let error = invoke(&webview, "start_saved_ssh", saved_request("no-such-host"))
            .expect_err("an unknown host cannot be connected");

        assert_eq!(error, json!("That saved host no longer exists."));
        assert!(!error.to_string().contains(SAVED_PASSWORD));
    }

    #[test]
    fn the_saved_host_command_refuses_a_host_without_a_credential() {
        let (_app, webview, _temp, host_id) = saved_host_window(true, false);

        let error = invoke(&webview, "start_saved_ssh", saved_request(&host_id))
            .expect_err("a host without a credential cannot be connected");

        assert_eq!(
            error,
            json!("This saved host has no credential yet. Edit the host and choose one.")
        );
    }

    /// A marker that must never appear in an answer or a payload. It is the
    /// content of the snippet the snippet commands are tested with.
    const SNIPPET_CONTENT: &str = "marker-snippet-content-5e1c";
    /// The name of that snippet, checked the same way.
    const SNIPPET_NAME: &str = "marker-snippet-name-9b3d";

    /// A window wired like the app for the snippet commands, over a vault of its
    /// own holding one snippet, either left open or locked.
    ///
    /// The existing snippet is written the way the command writes one, before
    /// the vault is handed to the app, so a locked window is locked for real.
    fn snippet_window(
        open: bool,
    ) -> (
        App<MockRuntime>,
        WebviewWindow<MockRuntime>,
        TempVault,
        String,
    ) {
        let temp = TempVault::new();
        let mut vault = temp.vault();
        vault.create(VAULT_PASSWORD).expect("the vault was created");

        let snippet = crate::snippets::create_snippet_record(
            &vault,
            &crate::snippets::SnippetInput {
                name: SNIPPET_NAME.to_string(),
                content: SNIPPET_CONTENT.to_string(),
            },
        )
        .expect("the snippet was created");

        if !open {
            vault.lock();
        }

        let app = mock_builder()
            .manage(SshState::default())
            .manage(VaultState::new(vault))
            .invoke_handler(tauri::generate_handler![
                crate::snippets::list_snippets,
                crate::snippets::create_snippet,
                crate::snippets::update_snippet,
                crate::snippets::delete_snippet,
                crate::snippets::run_snippet
            ])
            .build(mock_context(noop_assets()))
            .expect("the app built");
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("the window built");

        (app, webview, temp, snippet.id)
    }

    fn snippet_input(name: &str, content: &str) -> Value {
        json!({ "name": name, "content": content })
    }

    /// The CRUD contract: the command names, the argument names and the field
    /// names of both directions. A snippet reaches the frontend with its
    /// content, because the UI has to show it before anyone can run it.
    #[test]
    fn the_snippet_commands_answer_over_ipc() {
        let (_app, webview, _temp, snippet_id) = snippet_window(true);
        let id = snippet_id.as_str();

        let listed = invoke(&webview, "list_snippets", json!({})).expect("the list answered");
        assert_eq!(
            listed,
            json!([{
                "version": 1,
                "id": id,
                "name": SNIPPET_NAME,
                "content": SNIPPET_CONTENT,
                "createdAt": listed[0]["createdAt"],
                "updatedAt": listed[0]["updatedAt"],
            }]),
            "the frontend contract changed"
        );

        let created = invoke(
            &webview,
            "create_snippet",
            json!({ "snippet": snippet_input("Deploy", "git pull\nmake") }),
        )
        .expect("the snippet was created");
        assert_eq!(created["name"], "Deploy");
        assert_eq!(created["content"], "git pull\nmake");

        let updated = invoke(
            &webview,
            "update_snippet",
            json!({ "id": created["id"], "snippet": snippet_input("Deploy", "git pull") }),
        )
        .expect("the snippet was updated");
        assert_eq!(updated["id"], created["id"]);
        assert_eq!(
            updated["createdAt"], created["createdAt"],
            "createdAt changed"
        );
        assert_eq!(updated["content"], "git pull");

        assert!(
            invoke(&webview, "delete_snippet", json!({ "id": created["id"] })).is_ok(),
            "deleting a snippet failed"
        );
        let remaining = invoke(&webview, "list_snippets", json!({})).expect("the list answered");
        assert_eq!(remaining.as_array().map(Vec::len), Some(1));

        // A renamed argument is a rejected call rather than a silent no-op.
        assert!(
            invoke(
                &webview,
                "delete_snippet",
                json!({ "snippetId": created["id"] })
            )
            .is_err(),
            "an unknown argument name was accepted"
        );
    }

    /// Create and update take only the two fields they need: an id, a timestamp
    /// or an object kind in the payload is refused instead of ignored.
    #[test]
    fn a_snippet_payload_carries_no_field_of_its_own() {
        let (_app, webview, _temp, snippet_id) = snippet_window(true);
        let id = snippet_id.as_str();

        for extra in [
            (json!({ "id": "other" }), "an id"),
            (json!({ "version": 99 }), "a version"),
            (json!({ "createdAt": 1 }), "a created time"),
            (json!({ "sessionId": "s-1" }), "a session id"),
        ] {
            let mut snippet = snippet_input("Deploy", "ls");
            snippet
                .as_object_mut()
                .expect("the snippet is an object")
                .extend(extra.0.as_object().expect("an object").clone());

            let error = invoke(&webview, "create_snippet", json!({ "snippet": snippet }))
                .expect_err(extra.1);
            assert!(!error.to_string().contains(SNIPPET_CONTENT));
        }

        assert!(
            invoke(&webview, "update_snippet", json!({ "id": id })).is_err(),
            "a payload without a snippet was accepted"
        );
    }

    /// `run_snippet` names a session and a snippet and nothing else. In
    /// particular it takes no content: the text a session receives can only come
    /// from the vault.
    #[test]
    fn the_run_snippet_payload_carries_only_two_ids() {
        let (app, webview, _temp, snippet_id) = snippet_window(true);
        let id = snippet_id.as_str();
        let recorded = app.state::<SshState>().record_input("session-1");

        for (extra, why) in [
            (json!({ "content": "whoami" }), "content"),
            (json!({ "command": "whoami" }), "a command"),
            (json!({ "data": "whoami\n" }), "terminal data"),
            (json!({ "password": "hunter2" }), "a password"),
            (json!({ "name": SNIPPET_NAME }), "a name"),
        ] {
            let mut request = json!({ "sessionId": "session-1", "snippetId": id });
            request
                .as_object_mut()
                .expect("the request is an object")
                .extend(extra.as_object().expect("an object").clone());

            let error =
                invoke(&webview, "run_snippet", json!({ "request": request })).expect_err(why);
            assert!(
                !error.to_string().contains(SNIPPET_CONTENT),
                "{why} leaked into {error}"
            );
        }

        // Nothing was sent to the session while those payloads were refused.
        assert!(recorded.take().is_empty(), "a refused request sent input");

        // The argument name is part of the contract too.
        assert!(
            invoke(
                &webview,
                "run_snippet",
                json!({ "snippet": { "sessionId": "session-1", "snippetId": id } })
            )
            .is_err(),
            "a renamed argument was accepted"
        );
    }

    /// The happy path over the real dispatch: the content stored in the vault is
    /// what the session receives, followed by exactly one submit keystroke.
    #[test]
    fn running_a_snippet_sends_the_stored_content_and_one_submit() {
        let (app, webview, _temp, snippet_id) = snippet_window(true);
        let id = snippet_id.as_str();
        let recorded = app.state::<SshState>().record_input("session-1");

        let response = invoke(
            &webview,
            "run_snippet",
            json!({ "request": { "sessionId": "session-1", "snippetId": id } }),
        );
        assert!(response.is_ok(), "running the snippet failed: {response:?}");

        assert_eq!(
            recorded.take(),
            vec![SNIPPET_CONTENT.to_string(), "\r".to_string()]
        );
    }

    #[test]
    fn the_snippet_commands_refuse_a_locked_vault() {
        let (_app, webview, _temp, snippet_id) = snippet_window(false);
        let id = snippet_id.as_str();

        let error = invoke(&webview, "list_snippets", json!({})).expect_err("a locked vault");
        assert_eq!(error, json!("The vault is locked. Unlock it first."));

        for call in [
            (
                "create_snippet",
                json!({ "snippet": snippet_input("Deploy", "ls") }),
            ),
            (
                "update_snippet",
                json!({ "id": id, "snippet": snippet_input("Deploy", "ls") }),
            ),
            ("delete_snippet", json!({ "id": id })),
            (
                "run_snippet",
                json!({ "request": { "sessionId": "session-1", "snippetId": id } }),
            ),
        ] {
            let error = match invoke(&webview, call.0, call.1) {
                Ok(_) => panic!("{} answered with a locked vault", call.0),
                Err(error) => error,
            };
            assert_eq!(error, json!("The vault is locked. Unlock it first."));
        }
    }

    #[test]
    fn the_snippet_commands_refuse_an_unknown_or_deleted_snippet() {
        let (app, webview, _temp, snippet_id) = snippet_window(true);
        let id = snippet_id.as_str();
        app.state::<SshState>().record_input("session-1");

        let missing = invoke(
            &webview,
            "run_snippet",
            json!({ "request": { "sessionId": "session-1", "snippetId": "no-such-snippet" } }),
        )
        .expect_err("an unknown snippet cannot run");
        assert_eq!(missing, json!("That snippet no longer exists."));
        assert!(!missing.to_string().contains(SNIPPET_CONTENT));

        assert!(
            invoke(&webview, "delete_snippet", json!({ "id": id })).is_ok(),
            "deleting a snippet failed"
        );
        let deleted = invoke(
            &webview,
            "run_snippet",
            json!({ "request": { "sessionId": "session-1", "snippetId": id } }),
        )
        .expect_err("a deleted snippet cannot run");
        assert_eq!(deleted, json!("That snippet no longer exists."));
        assert!(
            !deleted.to_string().contains(SNIPPET_CONTENT),
            "a deleted snippet answered with its content"
        );
    }

    #[test]
    fn running_a_snippet_without_a_session_is_refused() {
        let (_app, webview, _temp, snippet_id) = snippet_window(true);
        let id = snippet_id.as_str();

        let error = invoke(
            &webview,
            "run_snippet",
            json!({ "request": { "sessionId": "session-1", "snippetId": id } }),
        )
        .expect_err("no session can accept a snippet");

        assert_eq!(error, json!("This session is not connected."));
        assert!(!error.to_string().contains(SNIPPET_CONTENT));
    }

    /// Validation errors name the field and never repeat the text they refused.
    #[test]
    fn snippet_validation_errors_never_repeat_the_content() {
        let (_app, webview, _temp, _id) = snippet_window(true);

        for (snippet, why) in [
            (snippet_input("   ", SNIPPET_CONTENT), "a blank name"),
            (snippet_input(SNIPPET_NAME, ""), "empty content"),
            (
                snippet_input(SNIPPET_NAME, &format!("{}x", "a".repeat(65_536))),
                "content over the limit",
            ),
            (snippet_input(SNIPPET_NAME, "echo\u{0}"), "a NUL byte"),
        ] {
            let error =
                invoke(&webview, "create_snippet", json!({ "snippet": snippet })).expect_err(why);
            assert!(
                !error.to_string().contains(SNIPPET_CONTENT),
                "{why} leaked the content into {error}"
            );
            assert!(
                !error.to_string().contains(SNIPPET_NAME),
                "{why} leaked the name into {error}"
            );
        }
    }

    /// Quick Connect still answers over the same dispatch, and still reports its
    /// own validation before any socket is opened.
    #[test]
    fn quick_connect_still_reports_an_incomplete_request() {
        let (_app, webview, _temp, _host_id) = saved_host_window(true, true);

        let error = invoke(
            &webview,
            "connect_ssh",
            json!({
                "request": {
                    "sessionId": "quick-1",
                    "host": "example.com",
                    "port": 22,
                    "username": "root",
                    "auth": "password",
                    "cols": 100,
                    "rows": 30
                }
            }),
        )
        .expect_err("a password connection needs a password");

        assert_eq!(error, json!("Enter the password for this host."));
    }
}
