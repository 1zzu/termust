//! SSH sessions built directly on `russh`.
//!
//! One `connect_ssh` call spawns one task that owns the whole SSH connection for
//! the lifetime of the session: TCP, handshake, authentication, the PTY channel
//! and the read/write loop. The frontend drives that task through the commands in
//! `lib.rs` and learns what happens through the Tauri events declared here.
//!
//! Nothing in this module writes credentials to disk or to a log. The connect
//! request deliberately has no `Debug` implementation so it cannot be printed by
//! accident.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, decode_secret_key};
use russh::{ChannelMsg, Disconnect};
use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};

/// Event names shared with the frontend. Keep in sync with `src/lib/ssh.ts`.
pub const EVENT_DATA: &str = "ssh://data";
pub const EVENT_STATUS: &str = "ssh://status";
pub const EVENT_HOST_KEY: &str = "ssh://host-key";
pub const EVENT_ERROR: &str = "ssh://error";

/// Terminal type advertised to the server. The UI configures xterm.js to match.
const TERM: &str = "xterm-256color";
/// Buffered commands per session. A person cannot type faster than this.
const COMMAND_QUEUE: usize = 256;
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// How long `stop` waits for a graceful close before aborting the session task.
const CLOSE_GRACE: Duration = Duration::from_secs(2);

/// Where a session reports what happens. In the app this forwards to the webview;
/// the live test at the bottom of this file collects the same stream instead.
type Notifier = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// The Tauri-managed state shared by every SSH command.
#[derive(Default)]
pub struct SshState {
    sessions: StdMutex<HashMap<String, Session>>,
    /// Host keys approved during this run. The spike keeps them in memory only;
    /// persisting them was explicitly left for a later phase.
    known_hosts: Arc<StdMutex<HashMap<String, String>>>,
}

impl SshState {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        // A poisoned lock only means an earlier command panicked. The map itself
        // is still usable, so recover instead of failing every later command.
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take(&self, session_id: &str) -> Option<Session> {
        self.lock().remove(session_id)
    }

    fn attach_task(&self, session_id: &str, task: JoinHandle<()>) {
        let mut guard = self.lock();
        if let Some(session) = guard.get_mut(session_id) {
            session.task = Some(task);
        }
    }
}

/// A running session as seen by the commands. The task itself lives in
/// `run_session`; this is only the frontend's handle on it.
struct Session {
    commands: mpsc::Sender<Command>,
    /// Resolves the pending host-key prompt. `None` once it has been answered.
    host_key_decision: Option<oneshot::Sender<bool>>,
    /// Set right after the task is spawned, so `stop` can abort a stuck session.
    task: Option<JoinHandle<()>>,
}

/// Everything the frontend can ask a running session to do.
enum Command {
    Input(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

/// Connection settings sent by the frontend. Secrets live here and nowhere else.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub session_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub cols: u32,
    pub rows: u32,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub passphrase: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Password,
    PrivateKey,
}

impl ConnectRequest {
    /// Rejects obviously unusable input before any socket is opened. The messages
    /// describe what is missing and never repeat the credential itself.
    fn validate(&self) -> Result<(), String> {
        if self.session_id.trim().is_empty() {
            return Err("The request had no session id.".to_string());
        }
        if self.host.trim().is_empty() {
            return Err("Enter a hostname or IP address.".to_string());
        }
        if self.port == 0 {
            return Err("Port must be between 1 and 65535.".to_string());
        }
        if self.username.trim().is_empty() {
            return Err("Enter a username.".to_string());
        }
        match self.auth {
            AuthMethod::Password if self.password.is_none() => {
                return Err("Enter the password for this host.".to_string());
            }
            AuthMethod::PrivateKey
                if self
                    .private_key
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty() =>
            {
                return Err("Paste the private key for this host.".to_string());
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataEvent {
    session_id: String,
    /// Base64 of the raw bytes, so the webview decodes them as UTF-8 itself.
    data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    session_id: String,
    status: Status,
    message: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Connecting,
    VerifyingHostKey,
    Authenticating,
    Connected,
    Disconnected,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostKeyEvent {
    session_id: String,
    host: String,
    port: u16,
    algorithm: String,
    fingerprint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEvent {
    session_id: String,
    message: String,
}

/// Starts a session and returns as soon as the task is spawned. Progress and
/// failures arrive as events, because connecting can block on the user's answer
/// to the host-key prompt.
pub fn start(app: &AppHandle, state: &SshState, request: ConnectRequest) -> Result<(), String> {
    request.validate()?;

    // Reconnecting with the same id replaces whatever was running before.
    if let Some(previous) = state.take(&request.session_id)
        && let Some(task) = previous.task
    {
        task.abort();
    }

    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);
    let (decision_tx, decision_rx) = oneshot::channel();

    let session_id = request.session_id.clone();
    let session_id_for_task = session_id.clone();
    let known_hosts = Arc::clone(&state.known_hosts);
    let app_for_task = app.clone();

    let notify_app = app.clone();
    let notifier: Notifier = Arc::new(move |event, payload| {
        // Best effort: if the window is gone there is nobody left to tell.
        let _ = notify_app.emit(event, payload);
    });

    // Register before spawning so a command arriving immediately already finds
    // the session, and so a prompt can be answered while it is still pending.
    state.lock().insert(
        session_id.clone(),
        Session {
            commands: commands_tx,
            host_key_decision: Some(decision_tx),
            task: None,
        },
    );

    let task = tauri::async_runtime::spawn(async move {
        let outcome = run_session(
            Arc::clone(&notifier),
            request,
            commands_rx,
            decision_rx,
            known_hosts,
        )
        .await;

        match outcome {
            Ok(()) => emit_status(
                &notifier,
                &session_id_for_task,
                Status::Disconnected,
                Some("The session ended.".to_string()),
            ),
            Err(message) => {
                emit(
                    &notifier,
                    EVENT_ERROR,
                    ErrorEvent {
                        session_id: session_id_for_task.clone(),
                        message,
                    },
                );
                emit_status(&notifier, &session_id_for_task, Status::Error, None);
            }
        }

        // The session is over, so it must not stay reachable by commands.
        if let Some(state) = app_for_task.try_state::<SshState>() {
            state.take(&session_id_for_task);
        }
    });

    state.attach_task(&session_id, task);
    Ok(())
}

/// Sends terminal input to a session.
pub fn write(state: &SshState, session_id: &str, data: &str) -> Result<(), String> {
    send(state, session_id, Command::Input(data.as_bytes().to_vec()))
}

/// Forwards a terminal resize to the remote PTY.
pub fn resize(state: &SshState, session_id: &str, cols: u32, rows: u32) -> Result<(), String> {
    send(state, session_id, Command::Resize { cols, rows })
}

/// Answers the pending host-key prompt. Answering twice is not an error.
pub fn respond_host_key(state: &SshState, session_id: &str, accepted: bool) -> Result<(), String> {
    let decision = state
        .lock()
        .get_mut(session_id)
        .and_then(|session| session.host_key_decision.take());

    match decision {
        Some(sender) => sender
            .send(accepted)
            .map_err(|_| "The connection attempt already ended.".to_string()),
        None => Ok(()),
    }
}

/// Closes a session and waits for its task to stop.
pub async fn stop(state: &SshState, session_id: &str) {
    let Some(session) = state.take(session_id) else {
        return;
    };

    // Best effort: if the queue is already full the task is stuck anyway and the
    // timeout below aborts it. Dropping the senders is itself a signal to stop.
    let _ = session.commands.try_send(Command::Close);

    let Some(mut task) = session.task else {
        return;
    };
    if tokio::time::timeout(CLOSE_GRACE, &mut task).await.is_err() {
        task.abort();
    }
}

fn send(state: &SshState, session_id: &str, command: Command) -> Result<(), String> {
    let guard = state.lock();
    let session = guard
        .get(session_id)
        .ok_or("This session is not connected.".to_string())?;
    session
        .commands
        .try_send(command)
        .map_err(|_| "This session is not accepting input.".to_string())
}

/// The full lifetime of one SSH connection.
async fn run_session(
    notifier: Notifier,
    request: ConnectRequest,
    mut commands: mpsc::Receiver<Command>,
    decision: oneshot::Receiver<bool>,
    known_hosts: Arc<StdMutex<HashMap<String, String>>>,
) -> Result<(), String> {
    let session_id = request.session_id.clone();
    emit_status(&notifier, &session_id, Status::Connecting, None);

    // Records why a host key was refused, so the reason survives the handshake
    // aborting with a generic `UnknownKey`.
    let rejection_reason: Arc<StdMutex<Option<String>>> = Arc::default();
    let probe = HostKeyProbe {
        notifier: Arc::clone(&notifier),
        session_id: session_id.clone(),
        host: request.host.clone(),
        port: request.port,
        decision: Some(decision),
        known_hosts,
        rejection_reason: Arc::clone(&rejection_reason),
    };

    let config = client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        ..Default::default()
    };

    let mut handle = match client::connect(
        Arc::new(config),
        (request.host.as_str(), request.port),
        probe,
    )
    .await
    {
        Ok(handle) => handle,
        Err(russh::Error::UnknownKey) => {
            return Err(take_reason(&rejection_reason).unwrap_or_else(|| {
                "Connection cancelled: the host key was not accepted.".to_string()
            }));
        }
        Err(error) => return Err(describe_connection_error(&error)),
    };

    emit_status(&notifier, &session_id, Status::Authenticating, None);
    authenticate(&mut handle, &request).await?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|_| "The server refused to open a session channel.".to_string())?;

    channel
        .request_pty(true, TERM, request.cols, request.rows, 0, 0, &[])
        .await
        .map_err(|_| "Could not request a terminal from the server.".to_string())?;
    channel
        .request_shell(true)
        .await
        .map_err(|_| "Could not start a shell on the server.".to_string())?;

    // Reading and writing are split so this one task can serve both without
    // borrowing the channel mutably from two futures at once.
    let (mut reader, writer) = channel.split();
    emit_status(&notifier, &session_id, Status::Connected, None);

    loop {
        tokio::select! {
            message = reader.wait() => match message {
                Some(ChannelMsg::Data { data }) => emit_data(&notifier, &session_id, &data),
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                    emit_data(&notifier, &session_id, &data);
                }
                Some(ChannelMsg::Failure) => {
                    return Err("The server rejected the terminal request.".to_string());
                }
                // The shell is done once it reports why it exited.
                Some(ChannelMsg::ExitStatus { .. }) => break,
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            },
            command = commands.recv() => match command {
                Some(Command::Input(bytes)) => writer
                    .data_bytes(bytes)
                    .await
                    .map_err(|_| "Could not send input to the server.".to_string())?,
                Some(Command::Resize { cols, rows }) => writer
                    .window_change(cols, rows, 0, 0)
                    .await
                    .map_err(|_| "Could not resize the remote terminal.".to_string())?,
                Some(Command::Close) | None => break,
            },
        }
    }

    // Best effort: the peer may already be gone, and dropping `handle` closes the
    // socket either way.
    let _ = writer.close().await;
    let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
    Ok(())
}

/// Authenticates with either the password or the private key from the request.
async fn authenticate(
    handle: &mut client::Handle<HostKeyProbe>,
    request: &ConnectRequest,
) -> Result<(), String> {
    let authenticated = match request.auth {
        AuthMethod::Password => {
            let password = request.password.as_deref().unwrap_or_default();
            handle
                .authenticate_password(&request.username, password)
                .await
        }
        AuthMethod::PrivateKey => {
            let pem = request.private_key.as_deref().unwrap_or_default();
            let key = decode_secret_key(pem, request.passphrase.as_deref())
                .map_err(|_| describe_key_error(request.passphrase.is_some()))?;
            // `best_supported_rsa_hash` is a nested Option: the outer one reports
            // whether the server advertised `server-sig-algs` at all.
            let hash_alg = match handle.best_supported_rsa_hash().await {
                Ok(Some(hash_alg)) => hash_alg,
                Ok(None) | Err(_) => None,
            };
            handle
                .authenticate_publickey(
                    &request.username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                )
                .await
        }
    };

    match authenticated {
        Ok(result) if result.success() => Ok(()),
        Ok(_) => Err("Authentication failed. Check the username and credentials.".to_string()),
        Err(error) => Err(describe_connection_error(&error)),
    }
}

/// Reports a private key that could not be read without repeating any of it.
fn describe_key_error(passphrase_was_given: bool) -> String {
    if passphrase_was_given {
        "Could not read the private key. Check that it is an OpenSSH private key and that the passphrase is correct.".to_string()
    } else {
        "Could not read the private key. Check that it is an OpenSSH private key; an encrypted key also needs its passphrase.".to_string()
    }
}

/// Turns a `russh` failure into something a person can act on. These errors never
/// contain the password or the private key, so the text is safe to show.
fn describe_connection_error(error: &russh::Error) -> String {
    match error {
        russh::Error::UnknownKey => {
            "Connection cancelled: the host key was not accepted.".to_string()
        }
        russh::Error::ConnectionTimeout => {
            "Timed out while connecting. Check the address, the port and your network.".to_string()
        }
        russh::Error::HUP | russh::Error::Disconnect => {
            "The server closed the connection.".to_string()
        }
        russh::Error::NoCommonAlgo { kind, .. } => format!(
            "The server and this client share no {kind:?} algorithm. Legacy algorithms are disabled by default."
        ),
        russh::Error::PacketAuth | russh::Error::WrongServerSig => {
            "The server failed host key verification.".to_string()
        }
        other => format!("SSH error: {other}"),
    }
}

fn take_reason(reason: &Arc<StdMutex<Option<String>>>) -> Option<String> {
    reason
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// The in-memory known-hosts key for a target.
fn host_key_id(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

/// What may be done with the host key a server presented.
#[derive(Debug, PartialEq, Eq)]
enum HostKeyDecision {
    /// It matches the key approved earlier for this host.
    Trusted,
    /// It does not match. The recorded key is kept so the UI can show both.
    Changed { recorded: String, presented: String },
    /// This host was never approved, so a person has to decide.
    NeedsApproval,
}

/// The trust rule for host keys, kept apart from the event plumbing so it can be
/// tested on its own. An unknown key is never trusted on its own.
fn decide_host_key(known: Option<&str>, presented: &str) -> HostKeyDecision {
    match known {
        Some(recorded) if recorded == presented => HostKeyDecision::Trusted,
        Some(recorded) => HostKeyDecision::Changed {
            recorded: recorded.to_string(),
            presented: presented.to_string(),
        },
        None => HostKeyDecision::NeedsApproval,
    }
}

/// Decides whether the server's host key may be used, asking the user the first
/// time a host is seen. Refusing aborts the handshake with `UnknownKey`.
struct HostKeyProbe {
    notifier: Notifier,
    session_id: String,
    host: String,
    port: u16,
    decision: Option<oneshot::Receiver<bool>>,
    known_hosts: Arc<StdMutex<HashMap<String, String>>>,
    rejection_reason: Arc<StdMutex<Option<String>>>,
}

impl client::Handler for HostKeyProbe {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_key.public_key();
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let key_id = host_key_id(&self.host, self.port);

        let known = self
            .known_hosts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key_id)
            .cloned();

        match decide_host_key(known.as_deref(), &fingerprint) {
            // A recorded key that still matches needs no second prompt.
            HostKeyDecision::Trusted => return Ok(true),
            HostKeyDecision::Changed {
                recorded,
                presented,
            } => {
                return self.reject(format!(
                    "The host key for {key_id} changed. Expected {recorded} but the server presented {presented}. The connection was stopped and the new key was not saved."
                ));
            }
            HostKeyDecision::NeedsApproval => {}
        }

        let Some(decision) = self.decision.take() else {
            // Already asked on this connection, and the answer was no.
            return Ok(false);
        };

        emit_status(
            &self.notifier,
            &self.session_id,
            Status::VerifyingHostKey,
            None,
        );
        emit(
            &self.notifier,
            EVENT_HOST_KEY,
            HostKeyEvent {
                session_id: self.session_id.clone(),
                host: self.host.clone(),
                port: self.port,
                algorithm: key.algorithm().as_str().to_string(),
                fingerprint: fingerprint.clone(),
            },
        );

        match decision.await {
            Ok(true) => {
                self.known_hosts
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(key_id, fingerprint);
                Ok(true)
            }
            // Refused by the user, or the prompt was abandoned because the
            // session was disconnected.
            Ok(false) | Err(_) => Ok(false),
        }
    }
}

impl HostKeyProbe {
    fn reject(&self, message: String) -> Result<bool, russh::Error> {
        *self
            .rejection_reason
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(message);
        Ok(false)
    }
}

fn emit_data(notifier: &Notifier, session_id: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    emit(
        notifier,
        EVENT_DATA,
        DataEvent {
            session_id: session_id.to_string(),
            data: BASE64.encode(bytes),
        },
    );
}

fn emit_status(notifier: &Notifier, session_id: &str, status: Status, message: Option<String>) {
    emit(
        notifier,
        EVENT_STATUS,
        StatusEvent {
            session_id: session_id.to_string(),
            status,
            message,
        },
    );
}

/// Events are best effort: a payload that cannot be serialised is a programming
/// error rather than a session failure, so the event is dropped.
fn emit(notifier: &Notifier, event: &str, payload: impl Serialize) {
    if let Ok(payload) = serde_json::to_value(payload) {
        notifier(event, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(auth: AuthMethod) -> ConnectRequest {
        ConnectRequest {
            session_id: "test-session".to_string(),
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth,
            cols: 80,
            rows: 24,
            password: None,
            private_key: None,
            passphrase: None,
        }
    }

    #[test]
    fn valid_password_request_is_accepted() {
        let mut request = request(AuthMethod::Password);
        request.password = Some("hunter2".to_string());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn valid_private_key_request_is_accepted() {
        let mut request = request(AuthMethod::PrivateKey);
        request.private_key = Some("-----BEGIN OPENSSH PRIVATE KEY-----".to_string());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn missing_password_is_reported() {
        let error = request(AuthMethod::Password).validate().unwrap_err();
        assert_eq!(error, "Enter the password for this host.");
    }

    #[test]
    fn blank_private_key_is_reported() {
        let mut request = request(AuthMethod::PrivateKey);
        request.private_key = Some("   \n".to_string());
        assert_eq!(
            request.validate().unwrap_err(),
            "Paste the private key for this host."
        );
    }

    #[test]
    fn blank_host_and_username_are_reported() {
        let mut blank_host = request(AuthMethod::Password);
        blank_host.password = Some("hunter2".to_string());
        blank_host.host = "  ".to_string();
        assert_eq!(
            blank_host.validate().unwrap_err(),
            "Enter a hostname or IP address."
        );

        let mut blank_user = request(AuthMethod::Password);
        blank_user.password = Some("hunter2".to_string());
        blank_user.username = String::new();
        assert_eq!(blank_user.validate().unwrap_err(), "Enter a username.");
    }

    #[test]
    fn port_zero_is_reported() {
        let mut request = request(AuthMethod::Password);
        request.password = Some("hunter2".to_string());
        request.port = 0;
        assert_eq!(
            request.validate().unwrap_err(),
            "Port must be between 1 and 65535."
        );
    }

    #[test]
    fn validation_messages_never_echo_credentials() {
        for auth in [AuthMethod::Password, AuthMethod::PrivateKey] {
            let mut request = request(auth);
            request.password = Some("correct-horse-battery-staple".to_string());
            request.private_key = Some("super-secret-key-material".to_string());
            // Force a failure that is not about the credential itself.
            request.host = "   ".to_string();

            let error = request.validate().unwrap_err();
            assert!(!error.contains("correct-horse-battery-staple"));
            assert!(!error.contains("super-secret-key-material"));
        }
    }

    #[test]
    fn key_errors_describe_the_problem_without_the_key() {
        let with_passphrase = describe_key_error(true);
        let without_passphrase = describe_key_error(false);

        assert!(with_passphrase.contains("passphrase"));
        assert!(without_passphrase.contains("passphrase"));
        assert_ne!(with_passphrase, without_passphrase);
    }

    #[test]
    fn rejected_host_keys_are_explained() {
        assert!(describe_connection_error(&russh::Error::UnknownKey).contains("host key"));
        assert!(describe_connection_error(&russh::Error::ConnectionTimeout).contains("Timed out"));
        assert!(describe_connection_error(&russh::Error::HUP).contains("closed the connection"));
    }

    #[test]
    fn no_common_algorithm_names_the_algorithm_kind() {
        let error = russh::Error::NoCommonAlgo {
            kind: russh::AlgorithmKind::Kex,
            ours: vec!["curve25519-sha256".to_string()],
            theirs: vec!["diffie-hellman-group1-sha1".to_string()],
        };
        let message = describe_connection_error(&error);
        assert!(message.contains("Kex"));
        assert!(message.contains("Legacy algorithms are disabled"));
    }

    #[test]
    fn auth_methods_match_the_frontend_spelling() {
        assert_eq!(
            serde_json::from_str::<AuthMethod>("\"password\"").unwrap(),
            AuthMethod::Password
        );
        assert_eq!(
            serde_json::from_str::<AuthMethod>("\"privateKey\"").unwrap(),
            AuthMethod::PrivateKey
        );
    }

    #[test]
    fn connect_request_accepts_camel_case_fields() {
        let request: ConnectRequest = serde_json::from_str(
            r#"{
                "sessionId": "abc",
                "host": "example.com",
                "port": 2222,
                "username": "root",
                "auth": "privateKey",
                "cols": 120,
                "rows": 40,
                "privateKey": "-----BEGIN OPENSSH PRIVATE KEY-----",
                "passphrase": "unlock"
            }"#,
        )
        .unwrap();

        assert_eq!(request.session_id, "abc");
        assert_eq!(request.port, 2222);
        assert_eq!(request.auth, AuthMethod::PrivateKey);
        assert_eq!(request.passphrase.as_deref(), Some("unlock"));
    }
    #[test]
    fn known_host_ids_distinguish_ports() {
        assert_eq!(host_key_id("example.com", 22), "example.com:22");
        assert_ne!(
            host_key_id("example.com", 22),
            host_key_id("example.com", 2222)
        );
    }

    #[test]
    fn an_unseen_host_key_always_needs_approval() {
        // The core safety rule: nothing is trusted without a user decision.
        assert_eq!(
            decide_host_key(None, "SHA256:abc"),
            HostKeyDecision::NeedsApproval
        );
    }

    #[test]
    fn a_recorded_host_key_that_matches_is_trusted() {
        assert_eq!(
            decide_host_key(Some("SHA256:abc"), "SHA256:abc"),
            HostKeyDecision::Trusted
        );
    }

    #[test]
    fn a_changed_host_key_is_reported_with_both_fingerprints() {
        assert_eq!(
            decide_host_key(Some("SHA256:abc"), "SHA256:xyz"),
            HostKeyDecision::Changed {
                recorded: "SHA256:abc".to_string(),
                presented: "SHA256:xyz".to_string(),
            }
        );
    }

    #[test]
    fn an_empty_record_never_matches_a_real_fingerprint() {
        assert!(matches!(
            decide_host_key(Some(""), "SHA256:abc"),
            HostKeyDecision::Changed { .. }
        ));
    }

    /// Drives a real session against a real SSH server: the host-key prompt,
    /// password authentication, a PTY, terminal input, a resize and disconnect.
    /// Ignored by default because it needs a server, usable credentials and
    /// network access:
    ///
    /// ```sh
    /// TERMUST_LIVE_HOST=host TERMUST_LIVE_USER=user TERMUST_LIVE_PASSWORD=secret \
    ///     cargo test live_ssh_session -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs a real SSH server; see the doc comment"]
    async fn live_ssh_session_connects_sends_input_and_disconnects() {
        type Recorded = Arc<StdMutex<Vec<(String, serde_json::Value)>>>;

        let host = live_setting("TERMUST_LIVE_HOST");
        let username = live_setting("TERMUST_LIVE_USER");
        let password = live_setting("TERMUST_LIVE_PASSWORD");
        let port = std::env::var("TERMUST_LIVE_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(22);

        // The same event stream the webview receives, collected in order.
        let events: Recorded = Arc::default();
        let sink = Arc::clone(&events);
        let notifier: Notifier = Arc::new(move |event, payload| {
            sink.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push((event.to_string(), payload));
        });

        let (commands, commands_rx) = mpsc::channel(COMMAND_QUEUE);
        let (decision_tx, decision_rx) = oneshot::channel();

        let request = ConnectRequest {
            session_id: "live".to_string(),
            host: host.clone(),
            port,
            username,
            auth: AuthMethod::Password,
            cols: 100,
            rows: 30,
            password: Some(password),
            private_key: None,
            passphrase: None,
        };

        let session = tokio::spawn(run_session(
            notifier,
            request,
            commands_rx,
            decision_rx,
            Arc::default(),
        ));

        // An unknown host must ask first, and show something verifiable.
        assert!(
            wait_for(&events, |events| host_key_prompt(events).is_some()).await,
            "no host key prompt arrived: {events:?}"
        );
        let recorded = snapshot_events(&events);
        let prompt = host_key_prompt(&recorded).expect("the prompt was just found");
        assert_eq!(prompt["host"], serde_json::json!(host));
        assert_eq!(prompt["port"], serde_json::json!(port));
        assert!(
            prompt["fingerprint"]
                .as_str()
                .unwrap_or_default()
                .starts_with("SHA256:"),
            "unexpected fingerprint: {prompt:?}"
        );
        println!("host key: {prompt:?}");
        decision_tx
            .send(true)
            .expect("the session still waits for an answer");

        assert!(
            wait_for(&events, |events| status_reached(events, "connected")).await,
            "never reached a shell: {events:?}"
        );

        commands
            .send(Command::Input(b"echo termust-live-check\n".to_vec()))
            .await
            .expect("the session accepted input");
        assert!(
            wait_for(&events, |events| terminal_text(events)
                .contains("termust-live-check"))
            .await,
            "the shell never echoed the marker: {:?}",
            snapshot_events(&events)
        );
        println!("terminal output: {:?}", snapshot_events(&events));

        commands
            .send(Command::Resize {
                cols: 120,
                rows: 40,
            })
            .await
            .expect("the session accepted a resize");

        commands
            .send(Command::Close)
            .await
            .expect("the session accepted close");
        let outcome = tokio::time::timeout(Duration::from_secs(10), session)
            .await
            .expect("disconnect stopped the session task")
            .expect("the session task did not panic");
        assert!(outcome.is_ok(), "the session failed: {outcome:?}");
    }

    fn live_setting(name: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set to run the live test"))
    }

    async fn wait_for(
        events: &Arc<StdMutex<Vec<(String, serde_json::Value)>>>,
        mut ready: impl FnMut(&[(String, serde_json::Value)]) -> bool,
    ) -> bool {
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

    fn snapshot_events(
        events: &Arc<StdMutex<Vec<(String, serde_json::Value)>>>,
    ) -> Vec<(String, serde_json::Value)> {
        events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn host_key_prompt(events: &[(String, serde_json::Value)]) -> Option<&serde_json::Value> {
        events
            .iter()
            .find(|(event, _)| event == EVENT_HOST_KEY)
            .map(|(_, payload)| payload)
    }

    fn status_reached(events: &[(String, serde_json::Value)], wanted: &str) -> bool {
        events.iter().any(|(event, payload)| {
            event == EVENT_STATUS && payload["status"].as_str() == Some(wanted)
        })
    }

    fn terminal_text(events: &[(String, serde_json::Value)]) -> String {
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
}
