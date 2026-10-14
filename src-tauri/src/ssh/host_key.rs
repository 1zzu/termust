//! Whether a server's host key may be used.
//!
//! The trust rule lives here and nowhere else: a key that was never approved is
//! never trusted, a key that changed is refused instead of being replaced, and a
//! key that was approved earlier is used without asking again. A refusal is
//! remembered so its reason survives the handshake ending with a generic
//! `UnknownKey`.
//!
//! What "approved earlier" means is `known_hosts.rs`: this file only compares
//! what it is given and reports the user's answer back.

use std::sync::{Arc, Mutex as StdMutex};

use russh::client;
use russh::keys::{HashAlg, PublicKeyOrCertificate};
use tokio::sync::oneshot;

use super::{EVENT_HOST_KEY, HostKeyEvent, Notifier, Status, emit, emit_status};
use crate::inventory::KnownKey;

/// The canonical identity of an SSH target, and the key the whole known-hosts
/// store is built on: one record per identity.
///
/// The port is always part of it, so the same host on two ports is two
/// identities. A hostname is folded to lower case, because DNS is
/// case-insensitive and `Example.com` must not be a second record for
/// `example.com`. An IPv6 literal is bracketed the way URLs spell it, so
/// `[::1]:22` cannot be read as the host `::1:22`.
pub(super) fn host_key_id(host: &str, port: u16) -> String {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    let host = host.to_lowercase();

    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// The key as the store spells it: `algorithm fingerprint`, for the one place
/// that shows both to a person.
fn described(key: &KnownKey) -> String {
    format!("{} {}", key.algorithm, key.fingerprint)
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
///
/// The algorithm is compared as well as the fingerprint: a key that keeps its
/// fingerprint but changes its algorithm is not the key that was approved.
fn decide_host_key(known: Option<&KnownKey>, presented: &KnownKey) -> HostKeyDecision {
    match known {
        Some(recorded) if recorded == presented => HostKeyDecision::Trusted,
        Some(recorded) => HostKeyDecision::Changed {
            recorded: described(recorded),
            presented: described(presented),
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
    /// What this host had recorded before the connection started. Looking it up
    /// happens before the handshake, never during it.
    pub(super) known: Option<KnownKey>,
    /// Whether an approval now would outlive this run. Only what the prompt
    /// tells the user.
    pub(super) persistent: bool,
    /// The key the user approved, for the session to store once the handshake is
    /// over: the probe itself never touches the vault.
    pub(super) approved: Arc<StdMutex<Option<KnownKey>>>,
    pub(super) rejection_reason: Arc<StdMutex<Option<String>>>,
}

impl client::Handler for HostKeyProbe {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_key.public_key();
        let presented = KnownKey {
            algorithm: key.algorithm().as_str().to_string(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        };
        let key_id = host_key_id(&self.host, self.port);

        match decide_host_key(self.known.as_ref(), &presented) {
            // A recorded key that still matches needs no second prompt.
            HostKeyDecision::Trusted => return Ok(true),
            HostKeyDecision::Changed {
                recorded,
                presented,
            } => {
                // The new key is never stored: replacing it is a decision the
                // user has to make on purpose, by forgetting the old one first.
                return self.reject(format!(
                    "The host key for {key_id} changed. {recorded} was approved earlier, but the server presented {presented}. The connection was stopped and the new key was not saved. Forget the old key to trust this one."
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
                algorithm: presented.algorithm.clone(),
                fingerprint: presented.fingerprint.clone(),
                persistent: self.persistent,
            },
        );

        match decision.await {
            Ok(true) => {
                // The session stores this once the handshake is over.
                *self
                    .approved
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(presented);
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

/// The key the user approved during this handshake, if there was one.
pub(super) fn take_approval(approved: &Arc<StdMutex<Option<KnownKey>>>) -> Option<KnownKey> {
    approved
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(algorithm: &str, fingerprint: &str) -> KnownKey {
        KnownKey {
            algorithm: algorithm.to_string(),
            fingerprint: fingerprint.to_string(),
        }
    }

    #[test]
    fn host_key_ids_distinguish_ports() {
        assert_eq!(host_key_id("example.com", 22), "example.com:22");
        assert_ne!(
            host_key_id("example.com", 22),
            host_key_id("example.com", 2222)
        );
    }

    #[test]
    fn a_hostname_is_case_insensitive_and_trimmed() {
        assert_eq!(host_key_id("  Example.COM  ", 22), "example.com:22");
    }

    #[test]
    fn an_ipv6_literal_is_bracketed_so_it_cannot_be_misread() {
        assert_eq!(host_key_id("::1", 22), "[::1]:22");
        assert_eq!(host_key_id("[::1]", 22), "[::1]:22");
        assert_eq!(host_key_id("2001:db8::1", 2222), "[2001:db8::1]:2222");
    }

    #[test]
    fn an_unseen_host_key_always_needs_approval() {
        // The core safety rule: nothing is trusted without a user decision.
        assert_eq!(
            decide_host_key(None, &key("ssh-ed25519", "SHA256:abc")),
            HostKeyDecision::NeedsApproval
        );
    }

    #[test]
    fn a_recorded_host_key_that_matches_is_trusted() {
        assert_eq!(
            decide_host_key(
                Some(&key("ssh-ed25519", "SHA256:abc")),
                &key("ssh-ed25519", "SHA256:abc")
            ),
            HostKeyDecision::Trusted
        );
    }

    #[test]
    fn a_changed_host_key_is_reported_with_both_keys() {
        assert_eq!(
            decide_host_key(
                Some(&key("ssh-ed25519", "SHA256:abc")),
                &key("ssh-ed25519", "SHA256:xyz")
            ),
            HostKeyDecision::Changed {
                recorded: "ssh-ed25519 SHA256:abc".to_string(),
                presented: "ssh-ed25519 SHA256:xyz".to_string(),
            }
        );
    }

    #[test]
    fn the_same_fingerprint_under_another_algorithm_is_a_changed_key() {
        assert_eq!(
            decide_host_key(
                Some(&key("ssh-ed25519", "SHA256:abc")),
                &key("ssh-rsa", "SHA256:abc")
            ),
            HostKeyDecision::Changed {
                recorded: "ssh-ed25519 SHA256:abc".to_string(),
                presented: "ssh-rsa SHA256:abc".to_string(),
            }
        );
    }

    #[test]
    fn an_empty_record_never_matches_a_real_key() {
        assert!(matches!(
            decide_host_key(Some(&key("", "")), &key("ssh-ed25519", "SHA256:abc")),
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

    #[test]
    fn an_approval_is_taken_once_and_then_cleared() {
        let approved: Arc<StdMutex<Option<KnownKey>>> = Arc::default();
        *approved.lock().unwrap() = Some(key("ssh-ed25519", "SHA256:abc"));

        assert_eq!(
            take_approval(&approved),
            Some(key("ssh-ed25519", "SHA256:abc"))
        );
        assert_eq!(take_approval(&approved), None);
    }

    /// The algorithm a host key reports is its own type, not the signature
    /// algorithm that was negotiated, so the comparison above cannot fire just
    /// because a server chose `rsa-sha2-512` this time and `rsa-sha2-256` the
    /// next. These two public keys pin that down.
    #[test]
    fn a_host_key_reports_its_key_type() {
        // Public keys only, generated for this test. A public key reveals
        // nothing secret.
        const ED25519: &str =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDB2ptA2LlMx45yO8eaRZcbpAE6zHXZpQJpRsyTXOwZq";
        const RSA: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQC+CAF4kb7j7CuYFZGcUtFZjFSHjnSfwvRDyRFwPgLSsbuFPx0oR2nra/79vnKx9m6AxLPi9LPlevbH8B8D1LnAW2Xgf+z6u+OiTxGh4P2WRbO9InCMLfcsojLzAmLawKDp5jptGl3/M+yD6Wdgl3cdzWYAQX4dpChUunHi7GCs8wKjmtYHJkqCK53jK/VrCJa47mpu5PnQGCy2ltpr5jPZpudGowkRLE5UlAace9Zd/5F9zFFhE0h1R5NcPfBve/iY73PgOqf9h0T77W7yqwCnxusIiJTIuzTF+kf/tDWrKijjwEmoMODjqGNg+1AhcuO22OxBPIO9iL+mp6MhmZjL";
        let ed25519 = russh::keys::ssh_key::PublicKey::from_openssh(ED25519).expect("a valid key");
        let rsa = russh::keys::ssh_key::PublicKey::from_openssh(RSA).expect("a valid key");

        assert_eq!(ed25519.algorithm().as_str(), "ssh-ed25519");
        assert_eq!(rsa.algorithm().as_str(), "ssh-rsa");
        assert!(
            rsa.algorithm().as_str().contains("ssh-rsa"),
            "an RSA host key keeps its key type whatever signature algorithm was negotiated"
        );
    }
}
