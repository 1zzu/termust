//! Connecting to a saved host with the credential stored beside it.
//!
//! The frontend names a host and nothing else. The address, the port, the
//! username and the secret are all read from the vault here, turned into the
//! same [`ConnectRequest`] Quick Connect builds, and handed to the same session
//! lifecycle. A secret therefore exists only inside this process: it is never an
//! argument of a Tauri command, never part of an event, and it is dropped with
//! the request when the session ends.
//!
//! The vault is read on the blocking pool, in one closure, and the lock is
//! released before the session is spawned. Nothing here touches the network.

use serde::Deserialize;

use crate::inventory::{Credential, CredentialSecret, Host, credential_for_host};
use crate::vault::VaultHandle;

use super::{AuthMethod, ConnectRequest};

/// What the frontend sends to connect a saved host: which host, and how big the
/// terminal is.
///
/// There is deliberately no address, port, username, credential id or secret
/// here, and `deny_unknown_fields` means a frontend that tries to send one is
/// refused instead of quietly ignored. Which server is used and which credential
/// opens it are decisions the backend makes from the vault.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedConnectRequest {
    pub session_id: String,
    pub host_id: String,
    pub cols: u32,
    pub rows: u32,
}

impl SavedConnectRequest {
    /// Rejects a request that cannot name a host, before the vault is opened.
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.session_id.trim().is_empty() {
            return Err("The request had no session id.".to_string());
        }
        if self.host_id.trim().is_empty() {
            return Err("The request named no saved host.".to_string());
        }
        Ok(())
    }
}

/// Reads the saved host and its credential, and builds the session request.
///
/// Every failure here happens before a socket is opened, so a locked vault, a
/// deleted host or a missing credential is reported instead of a connection
/// being attempted.
pub(super) async fn request_from_vault(
    vault: VaultHandle,
    request: &SavedConnectRequest,
) -> Result<ConnectRequest, String> {
    let host_id = request.host_id.clone();
    let session_id = request.session_id.clone();
    let (cols, rows) = (request.cols, request.rows);

    tokio::task::spawn_blocking(move || {
        // One vault read: the host, its credential, and the checks between them.
        // The lock is taken here and dropped again before this returns.
        vault
            .with(|vault| credential_for_host(vault, &host_id))
            .map_err(|error| error.to_string())
            .map(|(host, credential)| resolve(&session_id, host, credential, cols, rows))
    })
    .await
    .map_err(|_| "The saved host could not be read.".to_string())?
}

/// Turns one decrypted host and credential into the request a session runs.
///
/// The `match` is exhaustive on purpose: every credential kind this build can
/// store is one the SSH core authenticates with, so there is no "unsupported
/// kind" branch to fall back to, and a new kind would not compile until it is
/// handled here.
fn resolve(
    session_id: &str,
    host: Host,
    credential: Credential,
    cols: u32,
    rows: u32,
) -> ConnectRequest {
    let (auth, password, private_key, passphrase) = match credential.secret {
        // Moved, never copied: the secret leaves the credential and lands in the
        // request that authentication reads.
        CredentialSecret::Password { password } => {
            (AuthMethod::Password, Some(password), None, None)
        }
        CredentialSecret::PrivateKey {
            private_key,
            passphrase,
        } => (AuthMethod::PrivateKey, None, Some(private_key), passphrase),
    };

    ConnectRequest {
        session_id: session_id.to_string(),
        host: host.address,
        port: host.port,
        username: host.username,
        auth,
        cols,
        rows,
        password,
        private_key,
        passphrase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    const SECRET_MARKER: &str = "marker-secret-4b9d";
    /// The payload version every record this build writes carries.
    const VERSION: u8 = 1;

    fn host(id: &str, credential_id: Option<&str>) -> Host {
        Host {
            version: VERSION,
            id: id.to_string(),
            label: "Prod web".to_string(),
            address: "web.example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            group_id: None,
            credential_id: credential_id.map(str::to_string),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn credential(secret: CredentialSecret) -> Credential {
        Credential {
            version: VERSION,
            id: "credential-1".to_string(),
            name: "Prod".to_string(),
            secret,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn password(value: &str) -> CredentialSecret {
        CredentialSecret::Password {
            password: Zeroizing::new(value.to_string()),
        }
    }

    /// The address, port and username always come from the host, and the terminal
    /// size from the request.
    fn assert_host_fields(request: &ConnectRequest) {
        assert_eq!(request.host, "web.example.com");
        assert_eq!(request.port, 2222);
        assert_eq!(request.username, "deploy");
        assert_eq!(request.cols, 100);
        assert_eq!(request.rows, 30);
        assert_eq!(request.session_id, "session-1");
    }

    #[test]
    fn a_password_credential_becomes_a_password_request() {
        let request = resolve(
            "session-1",
            host("host-1", Some("credential-1")),
            credential(password(SECRET_MARKER)),
            100,
            30,
        );

        assert_host_fields(&request);
        assert_eq!(request.auth, AuthMethod::Password);
        assert_eq!(
            request.password.as_deref().map(String::as_str),
            Some(SECRET_MARKER)
        );
        assert_eq!(request.private_key, None);
        assert_eq!(request.passphrase, None);
    }

    #[test]
    fn a_private_key_credential_becomes_a_key_request() {
        let request = resolve(
            "session-1",
            host("host-1", Some("credential-1")),
            credential(CredentialSecret::PrivateKey {
                private_key: Zeroizing::new(SECRET_MARKER.to_string()),
                passphrase: None,
            }),
            100,
            30,
        );

        assert_host_fields(&request);
        assert_eq!(request.auth, AuthMethod::PrivateKey);
        assert_eq!(
            request.private_key.as_deref().map(String::as_str),
            Some(SECRET_MARKER)
        );
        assert_eq!(request.passphrase, None);
        assert_eq!(request.password, None);
    }

    #[test]
    fn an_encrypted_private_key_keeps_its_passphrase() {
        let request = resolve(
            "session-1",
            host("host-1", Some("credential-1")),
            credential(CredentialSecret::PrivateKey {
                private_key: Zeroizing::new("marker-key-6c2e".to_string()),
                passphrase: Some(Zeroizing::new(SECRET_MARKER.to_string())),
            }),
            100,
            30,
        );

        assert_eq!(request.auth, AuthMethod::PrivateKey);
        assert_eq!(
            request.passphrase.as_deref().map(String::as_str),
            Some(SECRET_MARKER)
        );
    }

    #[test]
    fn a_request_for_a_saved_host_accepts_only_the_four_session_fields() {
        let parsed: SavedConnectRequest =
            serde_json::from_str(r#"{"sessionId":"s-1","hostId":"host-1","cols":120,"rows":40}"#)
                .expect("the frontend payload parses");

        assert_eq!(parsed.session_id, "s-1");
        assert_eq!(parsed.host_id, "host-1");
        assert_eq!(parsed.cols, 120);
        assert_eq!(parsed.rows, 40);

        // Anything a secret could travel in is refused rather than ignored.
        for extra in [
            r#""password":"marker-secret-4b9d""#,
            r#""privateKey":"marker-key-6c2e""#,
            r#""passphrase":"marker-passphrase-8a1f""#,
            r#""address":"evil.example.com""#,
            r#""port":2222"#,
            r#""username":"root""#,
            r#""credentialId":"credential-1""#,
            r#""auth":"password""#,
        ] {
            let payload =
                format!(r#"{{"sessionId":"s-1","hostId":"host-1","cols":120,"rows":40,{extra}}}"#);
            assert!(
                serde_json::from_str::<SavedConnectRequest>(&payload).is_err(),
                "{extra} was accepted"
            );
        }
    }

    #[test]
    fn a_request_without_a_host_id_is_refused() {
        let missing = SavedConnectRequest {
            session_id: "s-1".to_string(),
            host_id: "   ".to_string(),
            cols: 80,
            rows: 24,
        };
        assert_eq!(
            missing.validate().unwrap_err(),
            "The request named no saved host."
        );

        let no_session = SavedConnectRequest {
            session_id: String::new(),
            host_id: "host-1".to_string(),
            cols: 80,
            rows: 24,
        };
        assert_eq!(
            no_session.validate().unwrap_err(),
            "The request had no session id."
        );
    }

    #[test]
    fn a_refused_payload_does_not_echo_what_it_tried_to_send() {
        // The window between "the secret is in the request" and "the request is
        // dropped" is where a leak would happen, so the rejection itself must not
        // repeat the value it refused.
        let payload = r#"{"sessionId":"s-1","hostId":"host-1","cols":120,"rows":40,"password":"marker-secret-4b9d"}"#;
        let error = match serde_json::from_str::<SavedConnectRequest>(payload) {
            Ok(_) => panic!("a secret does not belong in this payload"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("password"), "{error}");
        assert!(!error.contains(SECRET_MARKER), "{error}");
    }
}
