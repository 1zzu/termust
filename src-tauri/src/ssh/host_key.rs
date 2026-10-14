//! Whether a server's host key may be used.
//!
//! The trust rule lives here and nowhere else: a key that was never approved is
//! never trusted, and a key that changed is refused instead of being replaced.
//! A refusal is remembered so its reason survives the handshake ending with a
//! generic `UnknownKey`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use russh::client;
use russh::keys::{HashAlg, PublicKeyOrCertificate};
use tokio::sync::oneshot;

use super::{EVENT_HOST_KEY, HostKeyEvent, Notifier, Status, emit, emit_status};

/// The in-memory known-hosts key for a target.
pub(super) fn host_key_id(host: &str, port: u16) -> String {
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
pub(super) struct HostKeyProbe {
    pub(super) notifier: Notifier,
    pub(super) session_id: String,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) decision: Option<oneshot::Receiver<bool>>,
    pub(super) known_hosts: Arc<StdMutex<HashMap<String, String>>>,
    pub(super) rejection_reason: Arc<StdMutex<Option<String>>>,
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

/// The reason a host key was refused, if one was recorded.
pub(super) fn take_reason(reason: &Arc<StdMutex<Option<String>>>) -> Option<String> {
    reason
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_reason_is_reported_once_and_then_cleared() {
        let reason: Arc<StdMutex<Option<String>>> = Arc::default();
        *reason.lock().unwrap() = Some("the key changed".to_string());

        assert_eq!(take_reason(&reason).as_deref(), Some("the key changed"));
        assert_eq!(take_reason(&reason), None);
    }
}
