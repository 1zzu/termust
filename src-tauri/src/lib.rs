//! Termust desktop shell.
//!
//! The frontend owns the screens; this layer only exposes the SSH session
//! commands it needs and keeps the session state alive between calls.

mod inventory;
mod ssh;
// The vault is a self-contained core the app builds on; keeping it public lets
// its API be used and tested directly, without going through the frontend.
pub mod vault;

use tauri::{AppHandle, Manager, State};

use ssh::{ConnectRequest, SshState};
use vault::{Vault, VaultState};

#[tauri::command]
fn connect_ssh(
    app: AppHandle,
    state: State<'_, SshState>,
    request: ConnectRequest,
) -> Result<(), String> {
    ssh::start(&app, &state, request)
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
            inventory::forget_known_host
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

    use super::{disconnect_ssh, resize_ssh, respond_host_key, write_ssh};
    use crate::inventory::{KnownKey, remember_known_host};
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
}
