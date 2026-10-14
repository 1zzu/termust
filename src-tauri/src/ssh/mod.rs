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
//! - `known_hosts.rs`: where an approved key is remembered, in memory and in the
//!   encrypted vault.
//! - `saved.rs`: turning a saved host and its stored credential into a request.
//! - `auth.rs`: password and private-key authentication.
//!
//! Nothing in this module writes credentials to disk or to a log. The connect
//! request deliberately has no `Debug` implementation so it cannot be printed by
//! accident.

mod auth;
mod host_key;
mod known_hosts;
mod saved;
mod session;

#[cfg(test)]
mod live;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::Duration;

use known_hosts::HostKeyTrust;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

use crate::vault;
use session::{Command, run_session};

// What the saved-host command takes, so `lib.rs` can name it.
pub use saved::SavedConnectRequest;

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
    sessions: Arc<StdMutex<SessionTable>>,
    /// Host keys approved during this run, by canonical host identity.
    ///
    /// This is what makes a locked vault usable: an approval given while the
    /// vault is closed still holds until the app exits. With the vault open the
    /// same approval is also written to it, which is what outlives a restart.
    known_hosts: known_hosts::Remembered,
}

/// The sessions and the restore guard as one table.
///
/// They share a lock on purpose: a restore has to find no session and claim the
/// guard in the same critical section that a new session checks it in, or the
/// two could slip past each other.
#[derive(Default)]
struct SessionTable {
    sessions: HashMap<String, Session>,
    /// Set while a restore holds the maintenance guard.
    maintaining: bool,
}

impl SshState {
    fn lock(&self) -> MutexGuard<'_, SessionTable> {
        // A poisoned lock only means an earlier command panicked. The table
        // itself is still usable, so recover instead of failing every later
        // command.
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take(&self, session_id: &str) -> Option<Session> {
        self.lock().sessions.remove(session_id)
    }

    /// Whether any session exists, including one that is still connecting.
    ///
    /// A session is registered before it connects and removed when its task ends,
    /// so this answers "could anything be talking to a host right now" without
    /// reaching the network. The production paths ask [`Self::begin_maintenance`]
    /// instead, because a restore has to see the sessions and claim the guard in
    /// one step; a test uses this to say a session really went away.
    #[cfg(test)]
    pub(crate) fn any_session(&self) -> bool {
        !self.lock().sessions.is_empty()
    }

    /// Claims the restore guard, or reports that a session is in the way.
    ///
    /// Looking at the sessions and setting the guard are one critical section,
    /// so nothing can register a session in between: either a session is already
    /// there and the caller gets `None`, or the guard is claimed first and every
    /// later registration is refused until it is dropped. The guard is held for
    /// a whole restore, including its blocking work, and it locks nothing else:
    /// the lock is released before this returns.
    pub(crate) fn begin_maintenance(&self) -> Option<MaintenanceGuard> {
        let mut table = self.lock();
        if table.maintaining || !table.sessions.is_empty() {
            return None;
        }
        table.maintaining = true;
        Some(MaintenanceGuard {
            sessions: Arc::clone(&self.sessions),
        })
    }

    /// Adds one session, unless a restore is in progress.
    ///
    /// Every path that can register a session goes through this — a quick
    /// connect, a saved host and the tests' fake sessions alike — so the guard
    /// cannot be bypassed. The previous session under the same id, if any, is
    /// handed back so the caller can stop it.
    fn register(&self, session_id: &str, session: Session) -> Result<Option<Session>, String> {
        let mut table = self.lock();
        if table.maintaining {
            return Err("A backup restore is in progress. Try again when it finishes.".to_string());
        }
        Ok(table.sessions.insert(session_id.to_string(), session))
    }

    fn attach_task(&self, session_id: &str, task: JoinHandle<()>) {
        let mut table = self.lock();
        if let Some(session) = table.sessions.get_mut(session_id) {
            session.task = Some(task);
        }
    }
}

/// A restore in progress, as long as this value is alive.
///
/// While it exists no SSH session can be registered, which is what makes the
/// session check and the restore one atomic decision. It holds only a handle on
/// the session table, never the lock itself, so the restore's file and crypto
/// work happens outside every lock. The guard is released by `Drop`, which runs
/// on success, on error, on an early return and during unwinding, so there is no
/// path that can leave the app unable to open a session again.
pub(crate) struct MaintenanceGuard {
    sessions: Arc<StdMutex<SessionTable>>,
}

impl Drop for MaintenanceGuard {
    fn drop(&mut self) {
        let mut table = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        table.maintaining = false;
    }
}

/// A session that accepts input without connecting anywhere, for tests that
/// drive the input path on its own.
///
/// The session is registered exactly like a real one, so the code under test
/// reaches it through the same `write` it would use in the app. What it was sent
/// is collected instead of being written to a socket.
#[cfg(test)]
pub(crate) struct RecordedInput {
    commands: StdMutex<mpsc::Receiver<Command>>,
}

#[cfg(test)]
impl RecordedInput {
    /// Everything sent so far, decoded as UTF-8 text and in order.
    pub(crate) fn take(&self) -> Vec<String> {
        let mut received = Vec::new();
        let mut commands = self
            .commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(command) = commands.try_recv() {
            if let Command::Input(bytes) = command {
                received.push(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        received
    }
}

#[cfg(test)]
impl SshState {
    /// Registers a fake session under this id and hands back what it receives.
    ///
    /// It registers through the same gate a real session uses, so a restore
    /// guard refuses it too; `try_record_input` is the version a test can assert
    /// the refusal with.
    pub(crate) fn record_input(&self, session_id: &str) -> RecordedInput {
        self.try_record_input(session_id)
            .expect("a session was registered while a restore held the guard")
    }

    /// The same, reporting the refusal instead of panicking on it.
    pub(crate) fn try_record_input(&self, session_id: &str) -> Result<RecordedInput, String> {
        let (commands, receiver) = mpsc::channel(COMMAND_QUEUE);
        self.register(
            session_id,
            Session {
                commands,
                host_key_decision: None,
                task: None,
            },
        )?;
        Ok(RecordedInput {
            commands: StdMutex::new(receiver),
        })
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

/// Connection settings a session runs on, whatever started it.
///
/// Quick Connect builds one from what the user typed; `saved.rs` builds the same
/// type from a stored host and credential. Secrets live here and nowhere else,
/// wrapped so their bytes are wiped when the request is dropped at the end of
/// the session. There is still no `Debug` implementation, so neither the
/// password nor the private key can be printed by accident.
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
    pub password: Option<Zeroizing<String>>,
    pub private_key: Option<Zeroizing<String>>,
    pub passphrase: Option<Zeroizing<String>>,
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
                    .map(String::as_str)
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
    /// Whether accepting this key will still be trusted after a restart. Only
    /// the backend can answer this, because only it knows if the vault is open.
    persistent: bool,
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
pub fn start<R: Runtime>(
    app: &AppHandle<R>,
    state: &SshState,
    request: ConnectRequest,
) -> Result<(), String> {
    request.validate()?;

    let (commands_tx, commands_rx) = mpsc::channel(COMMAND_QUEUE);
    let (decision_tx, decision_rx) = oneshot::channel();

    let session_id = request.session_id.clone();
    let session_id_for_task = session_id.clone();
    // The session task outlives this command, so it is given a handle on the
    // vault instead of the `AppHandle` this command was called with.
    let vault = app
        .try_state::<vault::VaultState>()
        .map(|state| state.handle());
    let trust = HostKeyTrust::new(Arc::clone(&state.known_hosts), vault);
    let app_for_task = app.clone();

    let notify_app = app.clone();
    let notifier: Notifier = Arc::new(move |event, payload| {
        // Best effort: if the window is gone there is nobody left to tell.
        let _ = notify_app.emit(event, payload);
    });

    // Register before spawning so a command arriving immediately already finds
    // the session, and so a prompt can be answered while it is still pending.
    // The registration is also the gate a restore guard closes: if one is held,
    // the session is refused here, before any task exists. Reconnecting with the
    // same id replaces whatever was running before.
    if let Some(previous) = state.register(
        &session_id,
        Session {
            commands: commands_tx,
            host_key_decision: Some(decision_tx),
            task: None,
        },
    )? && let Some(task) = previous.task
    {
        task.abort();
    }

    let task = tauri::async_runtime::spawn(async move {
        let outcome = run_session(
            Arc::clone(&notifier),
            request,
            commands_rx,
            decision_rx,
            trust,
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

/// Starts the session a saved host describes.
///
/// The frontend only names the host; the address, the username and the secret
/// all come from the vault, so nothing it sends can point the connection at
/// another server or another credential. Reading the vault happens on the
/// blocking pool and finishes before the session is spawned, which is what keeps
/// the vault lock away from every network operation.
pub async fn start_saved<R: Runtime>(
    app: &AppHandle<R>,
    state: &SshState,
    request: SavedConnectRequest,
) -> Result<(), String> {
    request.validate()?;

    let Some(vault) = app
        .try_state::<vault::VaultState>()
        .map(|vault| vault.handle())
    else {
        return Err("The vault is not available.".to_string());
    };
    let resolved = saved::request_from_vault(vault, &request).await?;

    // Same lifecycle as Quick Connect: one session, one task, one set of events.
    start(app, state, resolved)
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
        .sessions
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
        .sessions
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
        request.password = Some(Zeroizing::new("hunter2".to_string()));
        assert!(request.validate().is_ok());
    }

    #[test]
    fn valid_private_key_request_is_accepted() {
        let mut request = request(AuthMethod::PrivateKey);
        request.private_key = Some(Zeroizing::new(
            "-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
        ));
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
        request.private_key = Some(Zeroizing::new("   \n".to_string()));
        assert_eq!(
            request.validate().unwrap_err(),
            "Paste the private key for this host."
        );
    }

    #[test]
    fn blank_host_and_username_are_reported() {
        let mut blank_host = request(AuthMethod::Password);
        blank_host.password = Some(Zeroizing::new("hunter2".to_string()));
        blank_host.host = "  ".to_string();
        assert_eq!(
            blank_host.validate().unwrap_err(),
            "Enter a hostname or IP address."
        );

        let mut blank_user = request(AuthMethod::Password);
        blank_user.password = Some(Zeroizing::new("hunter2".to_string()));
        blank_user.username = String::new();
        assert_eq!(blank_user.validate().unwrap_err(), "Enter a username.");
    }

    #[test]
    fn port_zero_is_reported() {
        let mut request = request(AuthMethod::Password);
        request.password = Some(Zeroizing::new("hunter2".to_string()));
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
            request.password = Some(Zeroizing::new("correct-horse-battery-staple".to_string()));
            request.private_key = Some(Zeroizing::new("super-secret-key-material".to_string()));
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
        assert_eq!(
            request.passphrase.as_deref().map(String::as_str),
            Some("unlock")
        );
    }
}
