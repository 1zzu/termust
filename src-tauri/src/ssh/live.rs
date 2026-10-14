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

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use super::host_key::host_key_id;
use super::session::{Command, run_session};
use super::{
    AuthMethod, COMMAND_QUEUE, ConnectRequest, EVENT_DATA, EVENT_ERROR, EVENT_HOST_KEY,
    EVENT_STATUS, Notifier, TEST_TASK_TIMEOUT,
};

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
    /// Starts a session with an in-memory known-hosts record the test controls.
    fn start(request: ConnectRequest, known_hosts: HashMap<String, String>) -> Self {
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
            Arc::new(StdMutex::new(known_hosts)),
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

    let mut session = RunningSession::start(password_request(&server, password), HashMap::new());
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

    // Pretend this host was approved earlier with a different key.
    let mut known_hosts = HashMap::new();
    known_hosts.insert(
        host_key_id(&server.host, server.port),
        "SHA256:notTheKeyThisServerUses".to_string(),
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
        password: Some(password),
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
        private_key: Some(private_key),
        passphrase,
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
