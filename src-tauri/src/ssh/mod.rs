//! SSH sessions built directly on `russh`.
//!
//! One `connect_ssh` call spawns one task that owns the whole SSH connection for
//! the lifetime of the session: TCP, handshake, authentication, the PTY channel
//! and the read/write loop. The frontend drives that task through the commands in
//! `lib.rs` and learns what happens through the Tauri events declared here.
//!
//! This file holds the command-facing API and the state those commands share.
//! The rest is split by concern:
//!
//! - `session.rs`: the lifetime of one connection, and the algorithm policy.
//! - `host_key.rs`: whether a server's host key may be used.
//! - `auth.rs`: password and private-key authentication.
//!
//! Nothing in this module writes credentials to disk or to a log. The connect
//! request deliberately has no `Debug` implementation so it cannot be printed by
//! accident.

mod auth;
mod host_key;
mod session;

#[cfg(test)]
mod live;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};

use session::{Command, run_session};

/// Event names shared with the frontend. Keep in sync with `src/lib/ssh.ts`.
pub const EVENT_DATA: &str = "ssh://data";
pub const EVENT_STATUS: &str = "ssh://status";
pub const EVENT_HOST_KEY: &str = "ssh://host-key";
pub const EVENT_ERROR: &str = "ssh://error";

/// Buffered commands per session. A person cannot type faster than this.
const COMMAND_QUEUE: usize = 256;
/// How long `stop` waits for a graceful close before aborting the session task.
const CLOSE_GRACE: Duration = Duration::from_secs(2);
/// How long the live tests wait for a session task to finish.
#[cfg(test)]
const TEST_TASK_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a session reports what happens. In the app this forwards to the webview;
/// the live tests collect the same stream instead.
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
}
