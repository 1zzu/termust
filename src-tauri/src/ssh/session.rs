//! The lifetime of one SSH connection.
//!
//! `run_session` is the only place that knows how a session starts, runs and
//! ends: it owns the TCP connection through `russh`, the PTY channel and the
//! read/write loop. It reports everything through the notifier it is handed, so
//! the app and the live tests run exactly the same code.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use russh::client;
use russh::{ChannelMsg, Disconnect, Preferred};
use tokio::sync::{mpsc, oneshot};

use super::auth::authenticate;
use super::host_key::{HostKeyProbe, take_reason};
use super::{ConnectRequest, Notifier, Status, emit_data, emit_status};

/// Terminal type advertised to the server. The UI configures xterm.js to match.
const TERM: &str = "xterm-256color";
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// `ssh-rsa` signs with SHA-1. `ssh_key` spells it `Algorithm::Rsa { hash: None }`;
/// it is matched by its wire name here so the rest of the default list can stay
/// exactly as `russh` ships it.
const LEGACY_RSA_SIGNATURE: &str = "ssh-rsa";

/// Everything the frontend can ask a running session to do.
pub(super) enum Command {
    Input(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

/// The algorithm policy every session uses.
///
/// `russh`'s defaults already leave out SHA-1 key exchange, CBC ciphers and
/// SHA-1/MD5 MACs, but they still end the public key list with `ssh-rsa`, which
/// signs with SHA-1. Dropping that one name means this client never offers it and
/// never accepts a host key that uses it. Everything else is left as the library
/// ships it, including anything a later version adds.
fn session_config() -> client::Config {
    let mut preferred = Preferred::DEFAULT;
    preferred.key = Cow::Owned(
        preferred
            .key
            .iter()
            .filter(|algorithm| algorithm.as_str() != LEGACY_RSA_SIGNATURE)
            .cloned()
            .collect(),
    );

    client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        preferred,
        ..Default::default()
    }
}

/// The full lifetime of one SSH connection.
pub(super) async fn run_session(
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

    let mut handle = match client::connect(
        Arc::new(session_config()),
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

/// Turns a `russh` failure into something a person can act on. These errors never
/// contain the password or the private key, so the text is safe to show.
pub(super) fn describe_connection_error(error: &russh::Error) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The names a modern client must not offer: SHA-1 signatures and key
    /// exchange, CBC ciphers and MD5 MACs.
    const LEGACY: [&str; 5] = ["sha1", "-cbc", "md5", "ssh-rsa", "ssh-dss"];

    /// Every algorithm list of a config, as the wire names the server sees.
    fn offered(config: &client::Config) -> Vec<(&'static str, Vec<String>)> {
        vec![
            (
                "key exchange",
                config
                    .preferred
                    .kex
                    .iter()
                    .map(|name| name.as_ref().to_string())
                    .collect(),
            ),
            (
                "public key",
                config
                    .preferred
                    .key
                    .iter()
                    .map(|algorithm| algorithm.as_str().to_string())
                    .collect(),
            ),
            (
                "cipher",
                config
                    .preferred
                    .cipher
                    .iter()
                    .map(|name| name.as_ref().to_string())
                    .collect(),
            ),
            (
                "MAC",
                config
                    .preferred
                    .mac
                    .iter()
                    .map(|name| name.as_ref().to_string())
                    .collect(),
            ),
        ]
    }

    #[test]
    fn legacy_algorithms_are_not_offered_by_default() {
        let config = session_config();

        for (kind, names) in offered(&config) {
            assert!(
                !names.is_empty(),
                "no {kind} algorithm would be offered at all"
            );
            for name in names {
                for legacy in LEGACY {
                    assert!(
                        !name.contains(legacy),
                        "the {kind} list still offers {name}"
                    );
                }
            }
        }
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
    fn the_modern_algorithms_are_still_offered() {
        let config = session_config();
        let names: Vec<String> = offered(&config)
            .into_iter()
            .flat_map(|(_, names)| names)
            .collect();

        for expected in [
            "ssh-ed25519",
            "rsa-sha2-256",
            "rsa-sha2-512",
            "curve25519-sha256",
            "hmac-sha2-256",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "{expected} is missing from {names:?}"
            );
        }
    }
}
