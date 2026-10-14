//! Tests that drive a real SSH server through `run_session`.
//!
//! They are ignored by default because they need a reachable server, network
//! access and working credentials:
//!
//! ```sh
//! TERMUST_LIVE_HOST=host TERMUST_LIVE_USER=user TERMUST_LIVE_PASSWORD=secret \
//!     cargo test live_ -- --ignored --nocapture
//! ```
//!
//! Private-key authentication reads its key from `TERMUST_LIVE_PRIVATE_KEY`, or
//! from the file named by `TERMUST_LIVE_PRIVATE_KEY_PATH`, and an encrypted key
//! takes its passphrase from `TERMUST_LIVE_PASSPHRASE`. Every secret comes from
//! the environment, a key file is only ever read, and nothing here prints a
//! secret. A test whose variables are missing reports why and does nothing.
//!
//! `scripts/test-ssh-integration.sh` sets all of that up for real: it starts
//! disposable servers with Docker and runs these tests against them, including
//! the one that needs a server offering only the legacy `ssh-rsa` signature.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use super::host_key::host_key_id;
use super::known_hosts::HostKeyTrust;
use super::saved::{self, SavedConnectRequest};
use super::session::{Command, run_session};
use super::{
    AuthMethod, COMMAND_QUEUE, ConnectRequest, EVENT_DATA, EVENT_ERROR, EVENT_HOST_KEY,
    EVENT_STATUS, Notifier, SshState, TEST_TASK_TIMEOUT,
};
use crate::inventory::{
    CredentialSecret, HostInput, KnownKey, create_host_record, create_password_record,
    create_private_key_record, remember_known_host,
};
use crate::snippets::{SnippetInput, create_snippet_record, get_snippet, send_content};
use crate::vault::{Vault, VaultHandle, VaultState, VaultStatus, test_support::TempVault};

/// The master password every live test creates its vault with.
const VAULT_PASSWORD: &str = "a-strong-master-password";

/// One event as the webview would receive it.
type Event = (String, Value);
type Events = Arc<StdMutex<Vec<Event>>>;

/// Where the live tests connect, taken from the environment.
struct LiveServer {
    host: String,
    port: u16,
    username: String,
}

impl LiveServer {
    /// `None` when the environment does not describe a server, so a test can say
    /// why it did nothing instead of reporting a failure that is not one.
    fn from_env() -> Option<Self> {
        let host = std::env::var("TERMUST_LIVE_HOST").ok()?;
        let username = std::env::var("TERMUST_LIVE_USER").ok()?;
        let port = std::env::var("TERMUST_LIVE_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(22);

        if host.trim().is_empty() || username.trim().is_empty() {
            return None;
        }

        Some(Self {
            host,
            port,
            username,
        })
    }
}

/// A session started the way `connect_ssh` starts one, plus the pieces a test
/// needs in order to watch and drive it.
struct RunningSession {
    events: Events,
    commands: mpsc::Sender<Command>,
    answer: Option<oneshot::Sender<bool>>,
    task: Option<JoinHandle<Result<(), String>>>,
}

impl RunningSession {
    /// Starts a session with an in-memory known-hosts record the test controls
    /// and no vault behind it, the way a headless run has none.
    fn start(request: ConnectRequest, known_hosts: HashMap<String, KnownKey>) -> Self {
        Self::spawn(
            request,
            HostKeyTrust::new(Arc::new(StdMutex::new(known_hosts)), None),
        )
    }

    /// Starts a session whose trust comes from a real vault, with nothing in
    /// memory, so only the vault can answer.
    fn start_with_vault(request: ConnectRequest, vault: VaultHandle) -> Self {
        Self::spawn(request, HostKeyTrust::new(Arc::default(), Some(vault)))
    }

    fn spawn(request: ConnectRequest, trust: HostKeyTrust) -> Self {
        let events: Events = Arc::default();
        let sink = Arc::clone(&events);
        let notifier: Notifier = Arc::new(move |event, payload| {
            sink.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push((event.to_string(), payload));
        });

        let (commands, commands_rx) = mpsc::channel(COMMAND_QUEUE);
        let (answer, decision_rx) = oneshot::channel();

        // The command layer wraps `run_session` in a Tauri task; spawning it on
        // tokio here keeps the session itself exactly the same.
        let task = tokio::spawn(run_session(
            notifier,
            request,
            commands_rx,
            decision_rx,
            trust,
        ));

        Self {
            events,
            commands,
            answer: Some(answer),
            task: Some(task),
        }
    }

    /// Everything the session has reported so far.
    fn log(&self) -> Vec<Event> {
        snapshot(&self.events)
    }

    /// Waits until the session reports something the test is looking for.
    async fn saw(&self, ready: impl FnMut(&[Event]) -> bool) -> bool {
        wait_for(&self.events, ready).await
    }

    /// Answers the pending host-key prompt.
    fn answer_host_key(&mut self, accepted: bool) {
        if let Some(answer) = self.answer.take() {
            answer
                .send(accepted)
                .expect("the session still waits for an answer");
        }
    }

    /// Sends a command to the session, the way a UI command would.
    async fn send(&self, command: Command) {
        self.commands
            .send(command)
            .await
            .expect("the session accepted the command");
    }

    /// Disconnects the way `disconnect_ssh` does and reports what the session
    /// ended with.
    async fn shutdown(&mut self) -> Result<(), String> {
        let _ = self.commands.send(Command::Close).await;
        let task = self.task.take().expect("the session was already shut down");

        tokio::time::timeout(TEST_TASK_TIMEOUT, task)
            .await
            .expect("disconnect stopped the session task")
            .expect("the session task did not panic")
    }
}

/// Approves the host key, waits for a shell, runs a marker command, resizes and
/// disconnects. Returns the events the session produced.
async fn run_live_session(request: ConnectRequest) -> Vec<Event> {
    let target = format!("{}:{}", request.host, request.port);
    let mut session = RunningSession::start(request, HashMap::new());

    // A host nobody has approved yet must ask before anything else happens.
    assert!(
        session
            .saw(|events| host_key_prompt(events).is_some())
            .await,
        "no host key prompt arrived: {:?}",
        session.log()
    );
    let prompt = host_key_prompt(&session.log())
        .expect("the prompt was just found")
        .clone();
    let fingerprint = prompt["fingerprint"].as_str().unwrap_or_default();
    assert!(
        fingerprint.starts_with("SHA256:"),
        "unexpected fingerprint: {prompt:?}"
    );
    println!(
        "host key approved: {} {} for {target}",
        prompt["algorithm"], fingerprint
    );
    session.answer_host_key(true);

    // When no shell appears, the reason the session failed explains far more
    // than the event log does, so report that instead.
    if !session
        .saw(|events| status_reached(events, "connected"))
        .await
    {
        let outcome = session.shutdown().await;
        panic!("the session never reached a shell: {outcome:?}");
    }

    session
        .send(Command::Input(b"echo termust-live-check\n".to_vec()))
        .await;
    assert!(
        session
            .saw(|events| terminal_text(events).contains("termust-live-check"))
            .await,
        "the shell never echoed the marker: {:?}",
        session.log()
    );

    session
        .send(Command::Resize {
            cols: 120,
            rows: 40,
        })
        .await;

    let outcome = session.shutdown().await;
    assert!(outcome.is_ok(), "the session failed: {outcome:?}");

    session.log()
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_password_authentication_opens_a_shell() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    assert_no_error_event(run_live_session(password_request(&server, password)).await);
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_private_key_authentication_opens_a_shell() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Some(private_key) = private_key_from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_PRIVATE_KEY_PATH or TERMUST_LIVE_PRIVATE_KEY");
        return;
    };
    let passphrase = std::env::var("TERMUST_LIVE_PASSPHRASE").ok();

    assert_no_error_event(
        run_live_session(private_key_request(&server, private_key, passphrase)).await,
    );
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_rejecting_the_host_key_stops_the_connection() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    // A vault that is open, so a refusal storing anything would be visible
    // afterwards: refusing has to leave no record behind.
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault
        .create("a-strong-master-password")
        .expect("the vault was created");

    let mut session = RunningSession::start_with_vault(
        password_request(&server, password),
        VaultState::new(vault).handle(),
    );
    assert!(
        session
            .saw(|events| host_key_prompt(events).is_some())
            .await,
        "no host key prompt arrived: {:?}",
        session.log()
    );
    session.answer_host_key(false);

    let outcome = session.shutdown().await;
    let message = outcome.expect_err("a refused host key must fail the connection");
    assert!(message.contains("host key"), "{message}");

    let log = session.log();
    assert!(
        !status_reached(&log, "connected"),
        "a refused host key must never reach a shell: {log:?}"
    );

    // Nothing was stored: a later connection, with nothing in memory, still has
    // to ask, exactly as the first one did.
    let mut reopened = temp.vault();
    reopened
        .unlock("a-strong-master-password")
        .expect("the vault unlocked");
    let stored = HostKeyTrust::new(Arc::default(), Some(VaultState::new(reopened).handle()))
        .lookup(&host_key_id(&server.host, server.port))
        .await;
    assert_eq!(stored, None, "a refused host key must not be stored");
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_changed_host_key_is_blocked_without_asking() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    // Pretend this host was approved earlier with a different key. The recorded
    // algorithm is the one the test servers use; against another server the
    // mismatch simply reports the algorithm as well as the fingerprint.
    let mut known_hosts = HashMap::new();
    known_hosts.insert(
        host_key_id(&server.host, server.port),
        KnownKey {
            algorithm: "ssh-ed25519".to_string(),
            fingerprint: "SHA256:notTheKeyThisServerUses".to_string(),
        },
    );

    let mut session = RunningSession::start(password_request(&server, password), known_hosts);

    let outcome = session.shutdown().await;
    let message = outcome.expect_err("a changed host key must fail the connection");
    assert!(message.contains("changed"), "{message}");
    assert!(message.contains("SHA256:"), "{message}");

    let log = session.log();
    assert!(
        !log.iter().any(|(event, _)| event == EVENT_HOST_KEY),
        "a changed host key must not be offered for approval: {log:?}"
    );
    assert!(
        !status_reached(&log, "connected"),
        "a changed host key must never reach a shell: {log:?}"
    );
}

/// The point of this phase, end to end against a real server: approve a key once
/// with the vault open, then connect again without being asked, including after
/// the vault was closed and opened again.
#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_an_approved_host_key_is_remembered_across_a_restart() {
    const VAULT_PASSWORD: &str = "a-strong-master-password";

    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    // A vault of this test's own, open, exactly as the app's is after unlocking.
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(VAULT_PASSWORD).expect("the vault was created");

    let mut first = RunningSession::start_with_vault(
        password_request(&server, password.clone()),
        VaultState::new(vault).handle(),
    );

    assert!(
        first.saw(|events| host_key_prompt(events).is_some()).await,
        "no host key prompt arrived: {:?}",
        first.log()
    );
    let prompt = host_key_prompt(&first.log())
        .expect("the prompt was just found")
        .clone();
    assert_eq!(
        prompt["persistent"].as_bool(),
        Some(true),
        "an open vault has to say the approval will be kept: {prompt:?}"
    );
    first.answer_host_key(true);
    assert!(
        first
            .saw(|events| status_reached(events, "connected"))
            .await,
        "the first session never reached a shell: {:?}",
        first.log()
    );
    let outcome = first.shutdown().await;
    assert!(outcome.is_ok(), "the first session failed: {outcome:?}");

    // A restart: the vault is closed and opened again, and the second session
    // keeps nothing in memory, so only the stored record can answer.
    let mut reopened = temp.vault();
    assert_eq!(reopened.status().unwrap(), VaultStatus::Locked);
    reopened.unlock(VAULT_PASSWORD).expect("the vault unlocked");

    let mut second = RunningSession::start_with_vault(
        password_request(&server, password),
        VaultState::new(reopened).handle(),
    );
    assert!(
        second
            .saw(|events| status_reached(events, "connected"))
            .await,
        "the remembered key did not open a shell: {:?}",
        second.log()
    );

    let log = second.log();
    assert!(
        !log.iter().any(|(event, _)| event == EVENT_HOST_KEY),
        "a remembered key must be used without asking: {log:?}"
    );
    let outcome = second.shutdown().await;
    assert!(outcome.is_ok(), "the second session failed: {outcome:?}");

    // And what the vault kept is exactly the key that was approved.
    let mut reopened = temp.vault();
    reopened.unlock(VAULT_PASSWORD).expect("the vault unlocked");
    let stored = HostKeyTrust::new(Arc::default(), Some(VaultState::new(reopened).handle()))
        .lookup(&host_key_id(&server.host, server.port))
        .await;
    assert_eq!(
        stored,
        Some(KnownKey {
            algorithm: prompt["algorithm"].as_str().unwrap_or_default().to_string(),
            fingerprint: prompt["fingerprint"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        }),
        "the vault did not keep the key that was approved"
    );
}

/// A server that only offers the legacy `ssh-rsa` (SHA-1) signature must be
/// refused instead of being talked into SHA-1. Point the variables at a server
/// configured with `PubkeyAcceptedAlgorithms ssh-rsa` and at an RSA key.
#[tokio::test]
#[ignore = "needs a server that only accepts ssh-rsa; see the module comment"]
async fn live_rsa_only_server_is_refused() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Some(private_key) = private_key_from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_PRIVATE_KEY_PATH or TERMUST_LIVE_PRIVATE_KEY");
        return;
    };

    let mut session = RunningSession::start(
        private_key_request(&server, private_key, None),
        HashMap::new(),
    );

    // This is still the first connection to that host, so the host key is
    // approved as usual; the refusal has to come from authentication.
    assert!(
        session
            .saw(|events| host_key_prompt(events).is_some())
            .await,
        "no host key prompt arrived: {:?}",
        session.log()
    );
    session.answer_host_key(true);

    let outcome = session.shutdown().await;
    let message = outcome.expect_err("a server offering only ssh-rsa must be refused");
    assert!(message.contains("ssh-rsa"), "{message}");
    assert!(message.contains("SHA-1"), "{message}");

    let log = session.log();
    assert!(
        !status_reached(&log, "connected"),
        "a server offering only ssh-rsa must never reach a shell: {log:?}"
    );
}

// --- a saved host, end to end ----------------------------------------------

/// A vault of its own, open, holding one saved host that points at the given
/// credential: what the app has once the user created both.
fn saved_host_vault(
    credential: CredentialSecret,
    server: &LiveServer,
) -> (TempVault, Vault, String) {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(VAULT_PASSWORD).expect("the vault was created");

    let credential_id = match credential {
        CredentialSecret::Password { password } => {
            create_password_record(&vault, "live password", password.as_str())
                .expect("the credential was created")
                .id
        }
        CredentialSecret::PrivateKey {
            private_key,
            passphrase,
        } => {
            create_private_key_record(
                &vault,
                "live key",
                private_key.as_str(),
                passphrase.as_ref().map(|value| value.as_str()),
            )
            .expect("the credential was created")
            .id
        }
    };

    let host = create_host_record(
        &vault,
        &HostInput {
            label: "Live server".to_string(),
            address: server.host.clone(),
            port: i64::from(server.port),
            username: server.username.clone(),
            group_id: None,
            credential_id: Some(credential_id),
        },
    )
    .expect("the saved host was created");

    (temp, vault, host.id)
}

fn password_secret(password: &str) -> CredentialSecret {
    CredentialSecret::Password {
        password: Zeroizing::new(password.to_string()),
    }
}

fn key_secret(private_key: &str, passphrase: Option<&str>) -> CredentialSecret {
    CredentialSecret::PrivateKey {
        private_key: Zeroizing::new(private_key.to_string()),
        passphrase: passphrase.map(|value| Zeroizing::new(value.to_string())),
    }
}

/// A session state whose one fake session forwards everything it is sent into a
/// real session, so a live test can drive the app's input path.
fn forwarding_session(session_id: &str, target: mpsc::Sender<Command>) -> SshState {
    let ssh = SshState::default();
    let (commands, mut receiver) = mpsc::channel(COMMAND_QUEUE);
    tokio::spawn(async move {
        while let Some(command) = receiver.recv().await {
            if target.send(command).await.is_err() {
                break;
            }
        }
    });
    ssh.lock().insert(
        session_id.to_string(),
        super::Session {
            commands,
            host_key_decision: None,
            task: None,
        },
    );
    ssh
}

/// Resolves a saved host the way `start_saved_ssh` does, before any session is
/// spawned.
async fn resolve_saved(handle: &VaultHandle, host_id: &str) -> Result<ConnectRequest, String> {
    saved::request_from_vault(
        handle.clone(),
        &SavedConnectRequest {
            session_id: "live-saved".to_string(),
            host_id: host_id.to_string(),
            cols: 100,
            rows: 30,
        },
    )
    .await
}

/// No event the frontend receives may carry the secret that opened the session.
fn assert_secret_hidden(events: &[Event], secret: &str) {
    let text = serde_json::to_string(events).expect("the events serialize");
    assert!(
        !text.contains(secret),
        "a secret reached the event stream: {text}"
    );
}

/// Opens a shell through a saved host, runs a marker command and disconnects.
/// Returns the events, as `run_live_session` does for Quick Connect.
async fn run_saved_session(handle: &VaultHandle, host_id: &str) -> Vec<Event> {
    let request = resolve_saved(handle, host_id)
        .await
        .expect("the saved host resolves to a request");
    let mut session = RunningSession::start_with_vault(request, handle.clone());

    assert!(
        session
            .saw(|events| host_key_prompt(events).is_some())
            .await,
        "no host key prompt arrived: {:?}",
        session.log()
    );
    session.answer_host_key(true);

    if !session
        .saw(|events| status_reached(events, "connected"))
        .await
    {
        let outcome = session.shutdown().await;
        panic!("the saved host never reached a shell: {outcome:?}");
    }

    session
        .send(Command::Input(b"echo termust-saved-check\n".to_vec()))
        .await;
    assert!(
        session
            .saw(|events| terminal_text(events).contains("termust-saved-check"))
            .await,
        "the shell never echoed the marker: {:?}",
        session.log()
    );

    let outcome = session.shutdown().await;
    assert!(outcome.is_ok(), "the session failed: {outcome:?}");

    session.log()
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_saved_host_password_credential_opens_a_shell() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    let (_temp, vault, host_id) = saved_host_vault(password_secret(&password), &server);
    let handle = VaultState::new(vault).handle();

    // The request really is built from the host record and its credential.
    let request = resolve_saved(&handle, &host_id)
        .await
        .expect("the saved host resolves");
    assert_eq!(request.auth, AuthMethod::Password);
    assert_eq!(request.host, server.host);
    assert_eq!(request.port, server.port);
    assert_eq!(request.username, server.username);

    let log = run_saved_session(&handle, &host_id).await;
    assert_no_error_event(log.clone());
    assert_secret_hidden(&log, &password);
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_saved_host_private_key_credential_opens_a_shell() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Some(private_key) = private_key_from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_PRIVATE_KEY_PATH or TERMUST_LIVE_PRIVATE_KEY");
        return;
    };
    let passphrase = std::env::var("TERMUST_LIVE_PASSPHRASE").ok();

    let (_temp, vault, host_id) =
        saved_host_vault(key_secret(&private_key, passphrase.as_deref()), &server);
    let handle = VaultState::new(vault).handle();

    let request = resolve_saved(&handle, &host_id)
        .await
        .expect("the saved host resolves");
    assert_eq!(request.auth, AuthMethod::PrivateKey);

    let log = run_saved_session(&handle, &host_id).await;
    assert_no_error_event(log.clone());
    assert_secret_hidden(&log, &private_key);
}

/// The whole point of the phase, against a real server: approve the key once
/// through a saved host, then connect the same saved host again after the vault
/// was closed and opened, without being asked again.
#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_saved_host_keeps_its_host_key_across_a_restart() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    let (temp, vault, host_id) = saved_host_vault(password_secret(&password), &server);
    let handle = VaultState::new(vault).handle();
    run_saved_session(&handle, &host_id).await;

    // A restart: the app holds nothing in memory, and only the vault can answer.
    let mut reopened = temp.vault();
    assert_eq!(reopened.status().unwrap(), VaultStatus::Locked);
    reopened.unlock(VAULT_PASSWORD).expect("the vault unlocked");
    let handle = VaultState::new(reopened).handle();

    let request = resolve_saved(&handle, &host_id)
        .await
        .expect("the saved host resolves after a restart");
    let mut session = RunningSession::start_with_vault(request, handle);

    assert!(
        session
            .saw(|events| status_reached(events, "connected"))
            .await,
        "the remembered key did not open a shell: {:?}",
        session.log()
    );
    let log = session.log();
    assert!(
        !log.iter().any(|(event, _)| event == EVENT_HOST_KEY),
        "a remembered key must be used without asking: {log:?}"
    );
    let outcome = session.shutdown().await;
    assert!(outcome.is_ok(), "the second session failed: {outcome:?}");
    assert_secret_hidden(&log, &password);
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_saved_host_with_a_wrong_password_reports_it_without_repeating_it() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    let wrong = format!("{password}-wrong");
    let (_temp, vault, host_id) = saved_host_vault(password_secret(&wrong), &server);
    let handle = VaultState::new(vault).handle();

    let request = resolve_saved(&handle, &host_id)
        .await
        .expect("the saved host resolves");
    let mut session = RunningSession::start_with_vault(request, handle);
    assert!(
        session
            .saw(|events| host_key_prompt(events).is_some())
            .await,
        "no host key prompt arrived: {:?}",
        session.log()
    );
    session.answer_host_key(true);

    let outcome = session.shutdown().await;
    let message = outcome.expect_err("a wrong password must fail the login");
    assert!(message.contains("Authentication failed"), "{message}");
    assert!(
        !message.contains(&wrong),
        "the password was repeated: {message}"
    );

    let log = session.log();
    assert!(
        !status_reached(&log, "connected"),
        "a failed login must never reach a shell: {log:?}"
    );
    assert_secret_hidden(&log, &wrong);
}

#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_saved_host_with_a_changed_host_key_is_blocked() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    let (_temp, vault, host_id) = saved_host_vault(password_secret(&password), &server);
    let handle = VaultState::new(vault).handle();

    // Pretend an earlier connection approved a different key for this server.
    handle
        .with(|vault| {
            remember_known_host(
                vault,
                &host_key_id(&server.host, server.port),
                &KnownKey {
                    algorithm: "ssh-ed25519".to_string(),
                    fingerprint: "SHA256:notTheKeyThisServerUses".to_string(),
                },
            )
        })
        .expect("the recorded key was stored");

    let request = resolve_saved(&handle, &host_id)
        .await
        .expect("the saved host resolves");
    let mut session = RunningSession::start_with_vault(request, handle);

    let outcome = session.shutdown().await;
    let message = outcome.expect_err("a changed host key must fail the connection");
    assert!(message.contains("changed"), "{message}");

    let log = session.log();
    assert!(
        !log.iter().any(|(event, _)| event == EVENT_HOST_KEY),
        "a changed host key must not be offered for approval: {log:?}"
    );
    assert!(
        !status_reached(&log, "connected"),
        "a changed host key must never reach a shell: {log:?}"
    );
}

// --- a snippet, end to end -------------------------------------------------

/// The point of this phase, against a real server: a snippet stored in the vault
/// is handed to a live shell through the same input path the app uses, and the
/// remote shell really runs it.
///
/// The arithmetic is computed by the remote shell, so the numbers can only
/// appear in the output if the command ran: the terminal echoing the line back
/// cannot produce them.
#[tokio::test]
#[ignore = "needs a real SSH server; see the module comment"]
async fn live_snippet_runs_on_the_remote_shell() {
    let Some(server) = LiveServer::from_env() else {
        eprintln!("skipping: set TERMUST_LIVE_HOST and TERMUST_LIVE_USER to run the live tests");
        return;
    };
    let Ok(password) = std::env::var("TERMUST_LIVE_PASSWORD") else {
        eprintln!("skipping: TERMUST_LIVE_PASSWORD is not set");
        return;
    };

    const SINGLE: &str = "echo termust-snippet-1c-$((6 * 7))";
    const MULTI: &str =
        "echo termust-snippet-multi-a-$((2 + 2))\necho termust-snippet-multi-b-$((3 + 3))";

    // A vault of this test's own, with two stored snippets.
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(VAULT_PASSWORD).expect("the vault was created");
    let single = create_snippet_record(
        &vault,
        &SnippetInput {
            name: "Marker".to_string(),
            content: SINGLE.to_string(),
        },
    )
    .expect("the snippet was stored");
    let multi = create_snippet_record(
        &vault,
        &SnippetInput {
            name: "Multiline".to_string(),
            content: MULTI.to_string(),
        },
    )
    .expect("the multiline snippet was stored");

    // A real session, opened the way a saved host opens one.
    let handle = VaultState::new(vault).handle();
    let mut session =
        RunningSession::start_with_vault(password_request(&server, password), handle.clone());

    // The host key has to be approved before anything else happens.
    assert!(
        session
            .saw(|events| host_key_prompt(events).is_some())
            .await,
        "no host key prompt arrived: {:?}",
        session.log()
    );
    session.answer_host_key(true);
    if !session
        .saw(|events| status_reached(events, "connected"))
        .await
    {
        let outcome = session.shutdown().await;
        panic!("the session never reached a shell: {outcome:?}");
    }

    // The session the run path writes to stands in for the one the commands
    // keep, and forwards what it receives into the live one, so the input really
    // goes through the path the app uses.
    let ssh = forwarding_session("live-snippet", session.commands.clone());

    // The content comes from the vault, exactly as `run_snippet` reads it.
    let content = handle
        .with(|vault| get_snippet(vault, &single.id))
        .expect("the snippet was read")
        .content;
    assert_eq!(content, SINGLE);
    send_content(&ssh, "live-snippet", &content).expect("the session accepted the snippet");

    assert!(
        session
            .saw(|events| terminal_text(events).contains("termust-snippet-1c-42"))
            .await,
        "the remote shell never ran the snippet: {:?}",
        terminal_text(&session.log())
    );

    // A snippet with more than one line runs as one piece with one submit.
    let content = handle
        .with(|vault| get_snippet(vault, &multi.id))
        .expect("the snippet was read")
        .content;
    send_content(&ssh, "live-snippet", &content).expect("the session accepted the snippet");

    assert!(
        session
            .saw(|events| {
                let text = terminal_text(events);
                text.contains("termust-snippet-multi-a-4")
                    && text.contains("termust-snippet-multi-b-6")
            })
            .await,
        "the multiline snippet did not run: {:?}",
        terminal_text(&session.log())
    );

    // And the session still takes typed input afterwards.
    session
        .send(Command::Input(
            b"echo termust-after-snippet-$((5 + 5))\n".to_vec(),
        ))
        .await;
    assert!(
        session
            .saw(|events| terminal_text(events).contains("termust-after-snippet-10"))
            .await,
        "the session stopped taking input after a snippet: {:?}",
        terminal_text(&session.log())
    );

    let log = session.log();
    let outcome = session.shutdown().await;
    assert!(outcome.is_ok(), "the session failed: {outcome:?}");
    assert_no_error_event(log);
}

/// This one needs no server: a saved host without a credential has to fail
/// while its record is read, before anything is looked up or connected. The
/// address is deliberately one that cannot resolve, so a failure that came from
/// the network instead would look different.
#[tokio::test]
async fn live_saved_host_without_a_credential_fails_before_any_connection() {
    let temp = TempVault::new();
    let mut vault = temp.vault();
    vault.create(VAULT_PASSWORD).expect("the vault was created");

    let host = create_host_record(
        &vault,
        &HostInput {
            label: "No credential".to_string(),
            address: "no-such-host.invalid".to_string(),
            port: 22,
            username: "nobody".to_string(),
            group_id: None,
            credential_id: None,
        },
    )
    .expect("the saved host was created");

    let handle = VaultState::new(vault).handle();
    let error = match resolve_saved(&handle, &host.id).await {
        Ok(_) => panic!("a host without a credential cannot be used"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        "This saved host has no credential yet. Edit the host and choose one."
    );
}

/// A session that ended on its own must not have reported a failure.
fn assert_no_error_event(log: Vec<Event>) {
    assert!(
        !log.iter().any(|(event, _)| event == EVENT_ERROR),
        "the session reported an error: {log:?}"
    );
}

fn password_request(server: &LiveServer, password: String) -> ConnectRequest {
    ConnectRequest {
        session_id: "live-password".to_string(),
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth: AuthMethod::Password,
        cols: 100,
        rows: 30,
        password: Some(Zeroizing::new(password)),
        private_key: None,
        passphrase: None,
    }
}

fn private_key_request(
    server: &LiveServer,
    private_key: String,
    passphrase: Option<String>,
) -> ConnectRequest {
    ConnectRequest {
        session_id: "live-private-key".to_string(),
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth: AuthMethod::PrivateKey,
        cols: 100,
        rows: 30,
        password: None,
        private_key: Some(Zeroizing::new(private_key)),
        passphrase: passphrase.map(Zeroizing::new),
    }
}

/// The private key for the live tests: the key itself, or a path to read it
/// from. Nothing is ever written back.
fn private_key_from_env() -> Option<String> {
    if let Ok(path) = std::env::var("TERMUST_LIVE_PRIVATE_KEY_PATH") {
        return std::fs::read_to_string(path).ok();
    }

    std::env::var("TERMUST_LIVE_PRIVATE_KEY").ok()
}

async fn wait_for(events: &Events, mut ready: impl FnMut(&[Event]) -> bool) -> bool {
    for _ in 0..200 {
        {
            let guard = events
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if ready(&guard) {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

fn snapshot(events: &Events) -> Vec<Event> {
    events
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn host_key_prompt(events: &[Event]) -> Option<&Value> {
    events
        .iter()
        .find(|(event, _)| event == EVENT_HOST_KEY)
        .map(|(_, payload)| payload)
}

fn status_reached(events: &[Event], wanted: &str) -> bool {
    events
        .iter()
        .any(|(event, payload)| event == EVENT_STATUS && payload["status"].as_str() == Some(wanted))
}

fn terminal_text(events: &[Event]) -> String {
    let mut bytes = Vec::new();
    for (event, payload) in events {
        if event != EVENT_DATA {
            continue;
        }
        if let Some(data) = payload["data"].as_str()
            && let Ok(decoded) = BASE64.decode(data)
        {
            bytes.extend_from_slice(&decoded);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
