//! SSH sessions built directly on `russh`.
//!
//! One `connect_ssh` call spawns one task that owns the whole SSH connection for
//! the lifetime of the session: TCP, handshake, authentication, the PTY channel
//! and the read/write loop. The frontend drives that task through the commands in
//! `lib.rs` and learns what happens through the Tauri events declared here.
//!
//! A window can hold several of these at once, one per terminal tab. They are
//! kept in a map by session id, and every command and every event names the
//! session it is for, so no session can reach another.
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

use std::collections::{HashMap, HashSet};
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
/// The same wait as a fallback for `stop`; a test shortens it rather than
/// changing what production does.
#[cfg(test)]
const TEST_CLOSE_GRACE: Duration = Duration::from_millis(50);
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

/// The sessions, the ids that are still stopping, and the restore guard.
///
/// They share a lock on purpose: a restore has to find nothing running and claim
/// the guard in the same critical section a new session checks it in, or the two
/// could slip past each other.
///
/// An id that is stopping belongs to that same decision. Its task is still
/// winding down — a graceful close, an abort, or the connection simply ending —
/// so the id is neither free for a new session nor invisible to a restore.
#[derive(Default)]
struct SessionTable {
    sessions: HashMap<String, Session>,
    /// Ids whose task has not really ended yet, reserved until it has.
    stopping: HashSet<String>,
    /// Counts registrations, so a task can only ever remove the entry it was
    /// registered as and never a newer one that took the id after it.
    last_generation: u64,
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

    /// Whether anything is still registered, including a session that is only
    /// connecting or one whose task is still stopping.
    ///
    /// A session is registered before it connects and removed either when its
    /// task ends or when the id is released after a stop, so this answers "could
    /// anything be talking to a host right now" without reaching the network.
    /// The production paths ask [`Self::begin_maintenance`] instead, because a
    /// restore has to see the sessions and claim the guard in one step; a test
    /// uses this to say a session really went away.
    #[cfg(test)]
    pub(crate) fn any_session(&self) -> bool {
        let table = self.lock();
        !table.sessions.is_empty() || !table.stopping.is_empty()
    }

    /// Whether this id is reserved while its task is still winding down.
    #[cfg(test)]
    pub(crate) fn is_stopping(&self, session_id: &str) -> bool {
        self.lock().stopping.contains(session_id)
    }

    /// The generation this id is registered as, for tests that play the part of
    /// a task whose cleanup arrives late.
    #[cfg(test)]
    pub(crate) fn generation_of(&self, session_id: &str) -> Option<u64> {
        self.lock()
            .sessions
            .get(session_id)
            .map(|session| session.generation)
    }

    /// Claims the restore guard, or reports that something is still in the way.
    ///
    /// Looking at the sessions and setting the guard are one critical section,
    /// so nothing can register a session in between: either a session is already
    /// there and the caller gets `None`, or the guard is claimed first and every
    /// later registration is refused until it is dropped.
    ///
    /// A session that is still stopping is in the way for the same reason a live
    /// one is: its task is still shutting the connection down, so a restore must
    /// not touch the vault underneath it. That is what makes the guard answer
    /// "is anything running" rather than "is anything in the map".
    ///
    /// The guard is held for a whole restore, including its blocking work, and
    /// it locks nothing else: the lock is released before this returns.
    pub(crate) fn begin_maintenance(&self) -> Option<MaintenanceGuard> {
        let mut table = self.lock();
        if table.maintaining || !table.sessions.is_empty() || !table.stopping.is_empty() {
            return None;
        }
        table.maintaining = true;
        Some(MaintenanceGuard {
            sessions: Arc::clone(&self.sessions),
        })
    }

    /// Registers a session and its task in one step, unless a restore holds the
    /// guard or the id is taken.
    ///
    /// Every path that can register a session goes through this — a quick
    /// connect, a saved host and the tests' fake sessions alike — so the guard
    /// cannot be bypassed, and so an id can only ever name one session.
    ///
    /// The entry and the task handle are installed together, inside one critical
    /// section, so there is no moment at which a session exists without the
    /// handle `stop` needs and none at which a task runs without an entry to
    /// reach it. A `stop` arriving around this call therefore either finds the
    /// whole session — task included — or finds nothing at all, and neither
    /// order can leave a detached SSH task behind.
    ///
    /// The task is built while the lock is held. It may already be running by
    /// the time this returns, but its own cleanup only takes this same lock,
    /// which the caller releases here, and nothing in the build performs a
    /// network or async wait.
    ///
    /// An id that is already taken is refused instead of replaced. The frontend
    /// mints a UUID per tab, so a duplicate means an id was reused rather than
    /// minted, and refusing it is what keeps a session that is still running —
    /// or still stopping — from having its commands and its events handed to a
    /// second one.
    fn launch(&self, session_id: &str, build: impl FnOnce(u64) -> Session) -> Result<u64, String> {
        let mut table = self.lock();
        if table.maintaining {
            return Err("A backup restore is in progress. Try again when it finishes.".to_string());
        }
        if table.sessions.contains_key(session_id) || table.stopping.contains(session_id) {
            return Err("This session id is already in use.".to_string());
        }
        table.last_generation += 1;
        let generation = table.last_generation;
        table
            .sessions
            .insert(session_id.to_string(), build(generation));
        Ok(generation)
    }

    /// Claims a session to stop, reserving its id until its task has ended.
    ///
    /// The id moves out of the live map and into the stopping set in one
    /// critical section: from here on a restore is refused and no new session
    /// may take the id, however long the task takes to finish. The reservation
    /// is released by dropping it, so it is given back on every path out of a
    /// stop — a graceful end, the abort timeout, and even a panic.
    fn begin_stop(&self, session_id: &str) -> Option<(Session, Stopping)> {
        let mut table = self.lock();
        let session = table.sessions.remove(session_id)?;
        table.stopping.insert(session_id.to_string());
        Some((
            session,
            Stopping {
                table: Arc::clone(&self.sessions),
                session_id: session_id.to_string(),
            },
        ))
    }

    /// Removes the entry of a task that ended on its own.
    ///
    /// Only the generation that registered this id may remove it, so a task that
    /// finishes late cannot take a newer session's place away. A session that a
    /// `stop` claimed is already out of the map, and its id is kept by the
    /// reservation instead, so this leaves it alone.
    fn retire(&self, session_id: &str, generation: u64) {
        let mut table = self.lock();
        let mine = table
            .sessions
            .get(session_id)
            .map(|session| session.generation);
        if mine == Some(generation) {
            table.sessions.remove(session_id);
        }
    }
}

/// The id of a session whose task has not really ended yet.
///
/// It is held for exactly as long as the stop runs and gives the id back when it
/// is dropped, which is why every path out of a stop — including a panic while
/// one is running — leaves the table usable instead of holding an id forever.
struct Stopping {
    table: Arc<StdMutex<SessionTable>>,
    session_id: String,
}

impl Drop for Stopping {
    fn drop(&mut self) {
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        table.stopping.remove(&self.session_id);
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
    /// What a host-key answer would resolve, so a test can see which session an
    /// answer was routed to.
    decision: StdMutex<oneshot::Receiver<bool>>,
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

    /// The resizes sent so far, as `(cols, rows)` and in order.
    pub(crate) fn take_resizes(&self) -> Vec<(u32, u32)> {
        let mut received = Vec::new();
        let mut commands = self
            .commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while let Ok(command) = commands.try_recv() {
            if let Command::Resize { cols, rows } = command {
                received.push((cols, rows));
            }
        }
        received
    }

    /// The host-key answer this session was given, or `None` while it has none.
    pub(crate) fn take_decision(&self) -> Option<bool> {
        self.decision
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .try_recv()
            .ok()
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
        let (decision, decisions) = oneshot::channel();
        // Registered through the same gate a real session uses, so a restore
        // guard and an id in use refuse it too.
        self.launch(session_id, |generation| Session {
            commands,
            host_key_decision: Some(decision),
            // A fake session has no task: `stop` finds nothing to wait for and
            // releases its id right away.
            task: None,
            generation,
        })?;
        Ok(RecordedInput {
            commands: StdMutex::new(receiver),
            decision: StdMutex::new(decisions),
        })
    }
}

/// A running session as seen by the commands. The task itself lives in
/// `run_session`; this is only the frontend's handle on it.
struct Session {
    commands: mpsc::Sender<Command>,
    /// Resolves the pending host-key prompt. `None` once it has been answered.
    host_key_decision: Option<oneshot::Sender<bool>>,
    /// Set by the same registration that inserted this session, so `stop` always
    /// has something to wait for and to abort.
    task: Option<JoinHandle<()>>,
    /// Which registration this is, so a task can only ever remove its own entry.
    generation: u64,
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

    // The session and its task are registered in one step, so a command
    // arriving immediately already finds the session, a prompt can be answered
    // while it is still pending, and a `stop` arriving at this very moment
    // either finds the whole session or finds nothing to leave behind. The
    // registration is also the gate a restore guard closes: if one is held, the
    // session is refused here, before any task exists. An id that another
    // session is still using — or is still stopping with — is refused for the
    // same reason: two sessions would otherwise answer to one id, and the one
    // that ends first would take the other's place away.
    state.launch(&session_id, |generation| {
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
            // Only this registration may be removed by it, so a task that ends
            // late cannot take a newer session's place away.
            if let Some(state) = app_for_task.try_state::<SshState>() {
                state.retire(&session_id_for_task, generation);
            }
        });

        Session {
            commands: commands_tx,
            host_key_decision: Some(decision_tx),
            task: Some(task),
            generation,
        }
    })?;

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
    stop_with_grace(state, session_id, CLOSE_GRACE).await
}

/// The same stop, with the wait for a graceful close given explicitly.
///
/// Production always uses [`CLOSE_GRACE`]; a test passes a much shorter one so
/// that a session wedged in its shutdown can be exercised without a two second
/// wait, and without changing what the app does.
///
/// The id stays reserved for the whole call: it is only given back once the task
/// has really ended, whether it ended on its own, on the abort below, or because
/// it never started at all.
async fn stop_with_grace(state: &SshState, session_id: &str, grace: Duration) {
    let Some((mut session, reservation)) = state.begin_stop(session_id) else {
        return;
    };

    // Best effort: if the queue is already full the task is stuck anyway and the
    // timeout below aborts it. Dropping the senders is itself a signal to stop.
    let _ = session.commands.try_send(Command::Close);

    if let Some(mut task) = session.task.take() {
        // A session with no task — a fake one, or a start that was refused — has
        // nothing to wait for, and gives its id back as soon as it is dropped.
        let closed_in_time = tokio::time::timeout(grace, &mut task).await.is_ok();
        if !closed_in_time {
            task.abort();
            // Aborting only asks the task to stop. The reservation is not given
            // back until it has actually finished, so nothing can take the id
            // while the old task is still winding down — and waiting here is
            // what keeps that wait out of the lock: the table is not held.
            let _ = task.await;
        }
    }

    // The session is over and its senders are dropped with it; only now may the
    // id be used again.
    drop(session);
    drop(reservation);
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

    /// Both sessions are registered the way a real one is, so these tests drive
    /// the same `write`, `resize`, `respond_host_key` and `stop` the app uses.
    #[test]
    fn two_sessions_run_side_by_side_and_keep_their_input_apart() {
        let state = SshState::default();
        let a = state.record_input("session-a");
        let b = state.record_input("session-b");

        write(&state, "session-a", "for-a").expect("session a is connected");
        write(&state, "session-b", "for-b").expect("session b is connected");
        write(&state, "session-a", "for-a-again").expect("session a is connected");

        assert_eq!(a.take(), vec!["for-a", "for-a-again"]);
        assert_eq!(b.take(), vec!["for-b"]);
    }

    #[test]
    fn input_for_a_session_that_is_not_there_is_refused() {
        let state = SshState::default();
        let a = state.record_input("session-a");

        let error = write(&state, "session-b", "hello").unwrap_err();

        assert_eq!(error, "This session is not connected.");
        assert!(a.take().is_empty(), "another session received the input");
    }

    #[test]
    fn a_session_id_that_is_already_in_use_is_refused() {
        let state = SshState::default();
        let first = state.record_input("session-a");

        // `expect_err` needs the value it discards to be `Debug`, and a
        // recorded session holds channels rather than anything printable.
        let error = match state.try_record_input("session-a") {
            Ok(_) => panic!("a live session keeps its id"),
            Err(error) => error,
        };

        assert_eq!(error, "This session id is already in use.");
        // The session that had the id still has it, and only it.
        write(&state, "session-a", "still-mine").expect("the first session is untouched");
        assert_eq!(first.take(), vec!["still-mine"]);
    }

    #[tokio::test]
    async fn disconnecting_one_session_leaves_the_other_running() {
        let state = SshState::default();
        let a = state.record_input("session-a");
        let b = state.record_input("session-b");

        stop(&state, "session-a").await;

        assert!(
            write(&state, "session-a", "gone").is_err(),
            "the stopped session still took input"
        );
        assert!(a.take().is_empty());
        assert!(
            !state.is_stopping("session-b"),
            "stopping one session reserved another session's id"
        );

        // The session that was not disconnected is still there and still takes
        // input, even though it started before the other one ended.
        write(&state, "session-b", "still-up").expect("session b did not survive session a");
        assert_eq!(b.take(), vec!["still-up"]);
    }

    #[test]
    fn resize_and_host_key_answers_go_to_the_session_they_name() {
        let state = SshState::default();
        let a = state.record_input("session-a");
        let b = state.record_input("session-b");

        resize(&state, "session-a", 120, 40).expect("session a is connected");

        assert_eq!(a.take_resizes(), vec![(120, 40)]);
        assert!(
            b.take_resizes().is_empty(),
            "a resize reached another session"
        );

        respond_host_key(&state, "session-b", false).expect("session b was waiting");

        assert_eq!(b.take_decision(), Some(false));
        assert_eq!(a.take_decision(), None, "an answer reached another session");
    }

    #[tokio::test]
    async fn the_restore_guard_waits_for_every_session_to_end() {
        let state = SshState::default();
        let _a = state.record_input("session-a");
        let _b = state.record_input("session-b");

        assert!(
            state.begin_maintenance().is_none(),
            "a restore was allowed while sessions were open"
        );

        stop(&state, "session-a").await;
        assert!(
            state.begin_maintenance().is_none(),
            "one session was still open"
        );

        stop(&state, "session-b").await;
        assert!(!state.any_session(), "every session ended");

        let guard = state
            .begin_maintenance()
            .expect("nothing is open, so a restore may start");
        assert!(
            state.try_record_input("while-restoring").is_err(),
            "a session was registered while the restore held the guard"
        );

        drop(guard);
        state
            .try_record_input("after-the-restore")
            .expect("the guard was released");
    }

    // --- sessions that are still stopping ---------------------------------

    /// A registered session whose task the test ends itself.
    ///
    /// It is installed by the same `launch` a real session goes through, and its
    /// task ends the way a real one does: it calls `retire` for its own
    /// registration on the way out. What the tests below assert is therefore
    /// about the production paths rather than about a copy of them.
    struct Held {
        /// What the session was sent, which is how a test sees that a stop has
        /// claimed it without waiting for anything to happen in time.
        commands: mpsc::Receiver<Command>,
        /// Opening this ends the task.
        end: oneshot::Sender<()>,
        /// Resolves once the task has ended and its cleanup has run.
        done: oneshot::Receiver<()>,
    }

    fn held_session(state: &Arc<SshState>, session_id: &str) -> Held {
        let (commands, commands_rx) = mpsc::channel(COMMAND_QUEUE);
        let (end, gate) = oneshot::channel::<()>();
        let (done_tx, done) = oneshot::channel::<()>();
        let owner = Arc::clone(state);
        let id = session_id.to_string();

        state
            .launch(session_id, |generation| Session {
                commands,
                host_key_decision: None,
                task: Some(tauri::async_runtime::spawn(async move {
                    // Nothing ends this task but the gate: it stands for a
                    // connection that is wedged in its own shutdown.
                    let _ = gate.await;
                    owner.retire(&id, generation);
                    let _ = done_tx.send(());
                })),
                generation,
            })
            .expect("nothing else holds this id");

        Held {
            commands: commands_rx,
            end,
            done,
        }
    }

    /// A session that a stop has claimed but whose task has not ended yet is in
    /// the way of a restore, and its id is not free for a new session.
    #[test]
    fn a_session_that_is_still_stopping_keeps_a_restore_out() {
        let state = SshState::default();
        let _recorded = state.record_input("session-a");

        // The stop has claimed the session; its task has not ended yet.
        let (session, reservation) = state
            .begin_stop("session-a")
            .expect("the session was there");

        assert!(
            state.is_stopping("session-a"),
            "the id was not reserved for the whole stop"
        );
        assert!(
            state.begin_maintenance().is_none(),
            "a restore was allowed while a session was still stopping"
        );
        let error = match state.try_record_input("session-a") {
            Ok(_) => panic!("a stopping id was handed to a new session"),
            Err(error) => error,
        };
        assert_eq!(error, "This session id is already in use.");

        // The task ends, and the reservation goes with it.
        drop(session);
        drop(reservation);

        assert!(
            !state.is_stopping("session-a"),
            "the id stayed reserved after the stop"
        );
        assert!(!state.any_session(), "something was left behind");
        assert!(
            state.begin_maintenance().is_some(),
            "a restore is still refused although nothing is running"
        );
    }

    /// A stop given back only once its task has ended is the whole point: until
    /// then a restore stays out and the id stays taken, and the close it sends is
    /// what tells the test the stop is in flight.
    #[tokio::test]
    async fn a_graceful_stop_gives_the_id_back_only_once_the_task_has_ended() {
        let state = Arc::new(SshState::default());
        let mut held = held_session(&state, "session-a");
        let stopper = {
            let state = Arc::clone(&state);
            tauri::async_runtime::spawn(async move { stop(&state, "session-a").await })
        };

        assert!(
            matches!(held.commands.recv().await, Some(Command::Close)),
            "the stop did not ask the session to close"
        );
        assert!(state.is_stopping("session-a"), "the id was not reserved");
        assert!(
            state.begin_maintenance().is_none(),
            "a restore was allowed while the task was still stopping"
        );

        // The task ends, and only then does the stop let the id go.
        held.end.send(()).expect("the task is waiting for the gate");
        stopper.await.expect("the stop finished");

        assert!(
            !state.is_stopping("session-a"),
            "the id came back before the task ended"
        );
        assert!(!state.any_session(), "the stopped session was left behind");
        assert!(
            state.begin_maintenance().is_some(),
            "a restore is still refused after every task ended"
        );
    }

    /// The other way out of a stop: the task never answers, so it is aborted, and
    /// the id is still given back — but not before the aborted task has finished.
    #[tokio::test]
    async fn a_stop_that_has_to_abort_still_gives_the_id_back() {
        let state = SshState::default();
        let (commands, _commands_rx) = mpsc::channel(COMMAND_QUEUE);

        state
            .launch("session-a", |generation| Session {
                commands,
                host_key_decision: None,
                // A task that never ends: only the abort can finish it.
                task: Some(tauri::async_runtime::spawn(std::future::pending())),
                generation,
            })
            .expect("the id is free");

        // A short wait rather than production's two seconds: what is under test
        // is that the abort path gives the id back, not how long it waits.
        stop_with_grace(&state, "session-a", TEST_CLOSE_GRACE).await;

        assert!(
            !state.is_stopping("session-a"),
            "the id stayed reserved after the abort"
        );
        assert!(!state.any_session(), "the aborted session was left behind");
        assert!(
            state.begin_maintenance().is_some(),
            "a restore is still refused after the abort"
        );
    }

    /// A session whose task ends on its own — the remote closed it, or the
    /// connection failed — leaves nothing behind, and neither its id nor a
    /// restore is held by it.
    #[tokio::test]
    async fn a_session_whose_task_ends_on_its_own_leaves_nothing_behind() {
        let state = Arc::new(SshState::default());
        let held = held_session(&state, "session-a");

        assert!(
            state.begin_maintenance().is_none(),
            "a restore was allowed while a session was open"
        );

        held.end.send(()).expect("the task is waiting for the gate");
        held.done
            .await
            .expect("the task ended and cleaned up after itself");

        assert!(!state.any_session(), "the session was left behind");
        assert!(
            state.begin_maintenance().is_some(),
            "a restore is refused although every task ended"
        );
    }

    /// One session stopping while another is live keeps a restore out until both
    /// are really over, and the stop of one reaches nothing of the other.
    #[tokio::test]
    async fn one_stopping_and_one_live_session_both_keep_a_restore_out() {
        let state = Arc::new(SshState::default());
        let mut a = held_session(&state, "session-a");
        let b = state.record_input("session-b");
        let stopper = {
            let state = Arc::clone(&state);
            tauri::async_runtime::spawn(async move { stop(&state, "session-a").await })
        };

        assert!(matches!(a.commands.recv().await, Some(Command::Close)));
        assert!(
            state.begin_maintenance().is_none(),
            "a stopping session did not keep the restore out"
        );

        // The stop of one session reaches nothing of the other.
        write(&state, "session-b", "still-up").expect("the other session is untouched");
        assert_eq!(b.take(), vec!["still-up"]);
        assert!(
            !state.is_stopping("session-b"),
            "the stop reserved another session's id"
        );

        // A is over, B alone is still enough to refuse the restore.
        a.end.send(()).expect("the task is waiting for the gate");
        stopper.await.expect("the stop finished");
        assert!(state.any_session(), "the live session is still there");
        assert!(
            state.begin_maintenance().is_none(),
            "a live session no longer kept the restore out"
        );

        // Only when both are over.
        stop(&state, "session-b").await;
        assert!(!state.any_session(), "something is still registered");
        let guard = state
            .begin_maintenance()
            .expect("a restore is allowed once nothing is left");
        drop(guard);
    }

    /// A stop that finds no session reserves nothing and removes nothing, so the
    /// session that starts next is not held back by it.
    #[tokio::test]
    async fn a_stop_that_finds_nothing_reserves_nothing() {
        let state = SshState::default();

        stop(&state, "session-a").await;

        assert!(
            !state.is_stopping("session-a"),
            "a stop with nothing to stop reserved an id"
        );
        assert!(
            !state.any_session(),
            "a stop with nothing to stop left something behind"
        );
        assert!(
            state.begin_maintenance().is_some(),
            "a stop with nothing to stop kept a restore out"
        );

        // The id is free again for the session that comes next.
        let _recorded = state.record_input("session-a");
    }

    /// The entry and its task handle arrive in one step, so a stop always finds
    /// the whole session, and a registration that is refused builds no task at
    /// all — a refused session cannot leave one running.
    #[tokio::test]
    async fn a_registered_session_carries_the_task_its_stop_needs() {
        let state = Arc::new(SshState::default());
        let mut held = held_session(&state, "session-a");

        let (session, reservation) = state
            .begin_stop("session-a")
            .expect("the entry is complete");
        assert!(
            session.task.is_some(),
            "a registered session had no task to stop"
        );

        let refused = state.launch("session-a", |_| {
            panic!("a refused registration built a task")
        });
        assert!(refused.is_err(), "a stopping id was handed out again");

        drop(session);
        drop(reservation);
        // Ending the task here is incidental: what this test is about is the
        // registration, and the task is only let go so the test leaves nothing
        // running behind it.
        let _ = held.end.send(());
        let _ = held.commands.recv().await;
    }

    /// A task whose cleanup arrives after its id was used again cannot take the
    /// newer session's place away.
    #[test]
    fn a_task_that_ends_late_cannot_remove_a_newer_session() {
        let state = SshState::default();
        let _first = state.record_input("session-a");
        let first = state
            .generation_of("session-a")
            .expect("the session is registered");

        // The first session is stopped, and its id is used again.
        let (session, reservation) = state
            .begin_stop("session-a")
            .expect("the session was there");
        drop(session);
        drop(reservation);
        let second = state.record_input("session-a");

        // The first task's cleanup arrives late, after the id was taken again.
        state.retire("session-a", first);

        assert!(
            state.any_session(),
            "a late task removed a newer session's entry"
        );
        write(&state, "session-a", "still-mine").expect("the newer session is untouched");
        assert_eq!(second.take(), vec!["still-mine"]);
    }

    /// A task that finishes before the registration has even returned must not
    /// leave a stale session behind: its own cleanup removes the entry, and the
    /// id is usable again.
    #[tokio::test]
    async fn a_task_that_finishes_at_once_leaves_no_stale_session() {
        let state = Arc::new(SshState::default());
        let (commands, _commands_rx) = mpsc::channel(COMMAND_QUEUE);
        let (done_tx, done) = oneshot::channel::<()>();
        let owner = Arc::clone(&state);

        state
            .launch("session-a", |generation| Session {
                commands,
                host_key_decision: None,
                task: Some(tauri::async_runtime::spawn(async move {
                    owner.retire("session-a", generation);
                    let _ = done_tx.send(());
                })),
                generation,
            })
            .expect("the id is free");

        done.await.expect("the task finished");

        assert!(!state.any_session(), "a stale session was left behind");
        assert!(
            state.begin_maintenance().is_some(),
            "a restore is refused although the task ended"
        );
        let _recorded = state
            .try_record_input("session-a")
            .expect("the id was given back");
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
