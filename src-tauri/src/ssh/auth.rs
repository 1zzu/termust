//! Password and private-key authentication.
//!
//! Authentication never reads a credential from anywhere but the connect
//! request, and every message it produces describes the problem without
//! repeating the secret.

use std::sync::Arc;

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, decode_secret_key};

use super::host_key::HostKeyProbe;
use super::session::describe_connection_error;
use super::{AuthMethod, ConnectRequest};

/// Shown when the server would only have accepted a SHA-1 RSA signature.
const SHA1_ONLY: &str = "This server only offers the legacy ssh-rsa (SHA-1) signature for RSA keys, which Termust does not use. Try an Ed25519 or ECDSA key.";

/// Shown when the server never said which RSA signatures it accepts.
const SHA1_UNKNOWN: &str = "This server did not say which RSA signatures it accepts, and Termust does not fall back to the legacy ssh-rsa (SHA-1) signature. Try an Ed25519 or ECDSA key.";

/// Authenticates with either the password or the private key from the request.
pub(super) async fn authenticate(
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
            // Only RSA keys are signed with a chosen hash; `russh` ignores it for
            // every other key type.
            let hash_alg = if key.algorithm().is_rsa() {
                Some(rsa_signature_hash(handle.best_supported_rsa_hash().await)?)
            } else {
                None
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

/// Picks the hash an RSA key is signed with, refusing the legacy SHA-1 fallback.
///
/// `russh` maps a missing hash to `ssh-rsa`, which signs with SHA-1, and its own
/// helper offers that as a last resort. The server's answer is therefore
/// inspected here instead of being passed straight through.
fn rsa_signature_hash(
    server_choice: Result<Option<Option<HashAlg>>, russh::Error>,
) -> Result<HashAlg, String> {
    match server_choice {
        // The server named at least one SHA-2 RSA signature algorithm.
        Ok(Some(Some(hash))) => Ok(hash),
        // It named its algorithms, and only `ssh-rsa` was among them.
        Ok(Some(None)) => Err(SHA1_ONLY.to_string()),
        // It never said, so `russh` would have signed with SHA-1.
        Ok(None) | Err(_) => Err(SHA1_UNKNOWN.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_rsa_key_is_only_signed_with_sha2() {
        assert_eq!(
            rsa_signature_hash(Ok(Some(Some(HashAlg::Sha512)))),
            Ok(HashAlg::Sha512)
        );
        assert_eq!(
            rsa_signature_hash(Ok(Some(Some(HashAlg::Sha256)))),
            Ok(HashAlg::Sha256)
        );
    }

    #[test]
    fn a_server_that_only_offers_sha1_is_refused() {
        let message = rsa_signature_hash(Ok(Some(None))).unwrap_err();

        assert!(message.contains("ssh-rsa"));
        assert!(message.contains("SHA-1"));
    }

    #[test]
    fn a_server_that_says_nothing_about_rsa_is_refused() {
        for refused in [
            rsa_signature_hash(Ok(None)),
            rsa_signature_hash(Err(russh::Error::SendError)),
        ] {
            let message = refused.unwrap_err();

            assert!(message.contains("ssh-rsa"), "{message}");
            assert!(message.contains("SHA-1"), "{message}");
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
}
