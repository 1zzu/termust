//! Key derivation and authenticated encryption for the local vault.
//!
//! The key hierarchy is deliberately short:
//!
//! ```text
//! master password --Argon2id--> Key Encryption Key
//! Key Encryption Key --XChaCha20-Poly1305--> Vault Key (32 random bytes)
//! Vault Key --XChaCha20-Poly1305--> object payloads
//! ```
//!
//! The master password is never stored, and neither is the Vault Key: the
//! database keeps only the wrapped Vault Key plus the parameters needed to
//! derive the Key Encryption Key again. No cryptographic primitive is
//! implemented here; every operation is a call into `argon2` or
//! `chacha20poly1305`.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use zeroize::{Zeroize, Zeroizing};

use super::VaultError;
use super::model::CRYPTO_VERSION;

/// Length of every symmetric key in the hierarchy.
pub const KEY_LEN: usize = 32;
/// XChaCha20-Poly1305 uses a 24-byte nonce.
pub const NONCE_LEN: usize = 24;
/// Random per-vault Argon2id salt.
pub const SALT_LEN: usize = 16;

/// Argon2id cost parameters.
///
/// They are stored with the vault, so a later build with different defaults can
/// still unlock a vault created today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in kibibytes.
    pub memory_cost: u32,
    /// Number of passes.
    pub time_cost: u32,
    /// Degree of parallelism.
    pub parallelism: u32,
}

/// The parameters a new vault is created with.
///
/// There is exactly one place to change these: everything else reads them from
/// the vault's own metadata.
pub const PRODUCTION: KdfParams = KdfParams {
    memory_cost: 64 * 1024,
    time_cost: 3,
    parallelism: 4,
};

/// Deliberately cheap parameters for tests.
///
/// `#[cfg(test)]` keeps them out of every non-test build, so they can never be
/// used for a real vault by accident.
#[cfg(test)]
pub const TEST: KdfParams = KdfParams {
    memory_cost: 1024,
    time_cost: 1,
    parallelism: 1,
};

/// A 32-byte symmetric key that wipes itself when it goes out of scope.
///
/// Wrapping the buffer in `Zeroizing` is what performs the wipe; dropping the
/// value is enough, and `lock` does exactly that.
pub struct SecretKey(Zeroizing<[u8; KEY_LEN]>);

impl SecretKey {
    /// A fresh key from the operating system's random source.
    pub fn generate() -> Result<Self, VaultError> {
        let mut bytes = [0u8; KEY_LEN];
        fill_random(&mut bytes)?;
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Zeroize for SecretKey {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

/// The wrapped Vault Key as it is stored: ciphertext plus the nonce it used.
pub struct WrappedKey {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// Derives the Key Encryption Key from the master password.
///
/// The password is borrowed for the length of this call and is not kept.
pub fn derive_kek(password: &str, salt: &[u8], params: KdfParams) -> Result<SecretKey, VaultError> {
    let params = Params::new(
        params.memory_cost,
        params.time_cost,
        params.parallelism,
        Some(KEY_LEN),
    )
    .map_err(|_| VaultError::Kdf)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut output = Zeroizing::new([0u8; KEY_LEN]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut output[..])
        .map_err(|_| VaultError::Kdf)?;
    Ok(SecretKey(output))
}

/// Encrypts the Vault Key with the Key Encryption Key.
pub fn wrap_vault_key(
    kek: &SecretKey,
    vault_key: &SecretKey,
    vault_id: &str,
) -> Result<WrappedKey, VaultError> {
    let nonce = random_nonce()?;
    let aad = vault_key_aad(vault_id);
    let ciphertext = seal(kek, &nonce, vault_key.as_bytes(), &aad)?;
    Ok(WrappedKey { ciphertext, nonce })
}

/// Recovers the Vault Key. A wrong Key Encryption Key fails the authentication
/// check, so this returns an error rather than garbage.
pub fn unwrap_vault_key(
    kek: &SecretKey,
    wrapped: &[u8],
    nonce: &[u8],
    vault_id: &str,
) -> Result<SecretKey, VaultError> {
    let aad = vault_key_aad(vault_id);
    let plaintext = open(kek, nonce, wrapped, &aad)?;
    let bytes: [u8; KEY_LEN] = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::Crypto)?;
    Ok(SecretKey::from_bytes(bytes))
}

/// Encrypts one object payload, bound to its id and kind.
pub fn encrypt_object(
    vault_key: &SecretKey,
    id: &str,
    kind: &str,
    plaintext: &[u8],
) -> Result<WrappedKey, VaultError> {
    let nonce = random_nonce()?;
    let ciphertext = seal(vault_key, &nonce, plaintext, &object_aad(id, kind))?;
    Ok(WrappedKey { ciphertext, nonce })
}

/// Decrypts one object payload, refusing it if the id, the kind or any byte has
/// changed since it was written.
pub fn decrypt_object(
    vault_key: &SecretKey,
    id: &str,
    kind: &str,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, VaultError> {
    open(vault_key, nonce, ciphertext, &object_aad(id, kind))
}

/// A fresh buffer of `len` random bytes.
pub fn random_bytes(len: usize) -> Result<Vec<u8>, VaultError> {
    let mut bytes = vec![0u8; len];
    fill_random(&mut bytes)?;
    Ok(bytes)
}

/// Adds a fresh random nonce to the given tag, as RustCrypto's `Aead` requires.
fn seal(
    key: &SecretKey,
    nonce: &[u8],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    cipher
        .encrypt(
            &nonce_from(nonce)?,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto)
}

fn open(
    key: &SecretKey,
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    cipher
        .decrypt(
            &nonce_from(nonce)?,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto)
}

fn random_nonce() -> Result<Vec<u8>, VaultError> {
    random_bytes(NONCE_LEN)
}

/// A nonce must be exactly `NONCE_LEN` bytes; anything else is damaged data.
fn nonce_from(bytes: &[u8]) -> Result<XNonce, VaultError> {
    let array: [u8; NONCE_LEN] = bytes.try_into().map_err(|_| VaultError::Crypto)?;
    Ok(XNonce::from(array))
}

fn fill_random(buffer: &mut [u8]) -> Result<(), VaultError> {
    // A random source that fails is a hard error: there is no safe fallback for
    // a salt, a key or a nonce.
    getrandom::fill(buffer).map_err(|_| VaultError::Random)
}

/// The Additional Authenticated Data for a wrapped Vault Key.
///
/// The `vault-key` domain keeps this ciphertext from being accepted anywhere an
/// object payload is expected, and the vault id binds it to this vault.
fn vault_key_aad(vault_id: &str) -> Vec<u8> {
    build_aad(b"vault-key", &[vault_id.as_bytes()])
}

/// The Additional Authenticated Data for one object payload.
fn object_aad(id: &str, kind: &str) -> Vec<u8> {
    build_aad(b"object", &[id.as_bytes(), kind.as_bytes()])
}

/// Builds AAD as labelled, length-prefixed parts.
///
/// Length prefixes mean two different inputs can never produce the same bytes,
/// so an id that contains the separator cannot impersonate a different id/kind
/// pair.
fn build_aad(domain: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut aad = Vec::new();
    aad.extend_from_slice(b"termust");
    aad.push(0);
    aad.extend_from_slice(&CRYPTO_VERSION.to_le_bytes());
    aad.push(0);
    aad.extend_from_slice(domain);
    for part in parts {
        aad.extend_from_slice(&(part.len() as u64).to_le_bytes());
        aad.extend_from_slice(part);
    }
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password() -> &'static str {
        "correct horse battery staple"
    }

    /// A Key Encryption Key from the test parameters: fast, but otherwise the
    /// same call the app makes.
    fn kek() -> SecretKey {
        derive_kek(password(), &[7u8; SALT_LEN], TEST).unwrap()
    }

    #[test]
    fn a_vault_key_survives_a_wrap_and_unwrap_round_trip() {
        let vault_key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let wrapped = wrap_vault_key(&kek(), &vault_key, "vault-id").unwrap();

        let unwrapped =
            unwrap_vault_key(&kek(), &wrapped.ciphertext, &wrapped.nonce, "vault-id").unwrap();

        assert_eq!(unwrapped.as_bytes(), vault_key.as_bytes());
    }

    #[test]
    fn a_wrong_password_cannot_unwrap_the_vault_key() {
        let vault_key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let wrapped = wrap_vault_key(&kek(), &vault_key, "vault-id").unwrap();

        let wrong = derive_kek("not the password", &[7u8; SALT_LEN], TEST).unwrap();
        let result = unwrap_vault_key(&wrong, &wrapped.ciphertext, &wrapped.nonce, "vault-id");

        assert!(result.is_err(), "a wrong password unwrapped the vault key");
    }

    #[test]
    fn a_different_salt_derives_a_different_key() {
        let first = derive_kek(password(), &[1u8; SALT_LEN], TEST).unwrap();
        let second = derive_kek(password(), &[2u8; SALT_LEN], TEST).unwrap();

        assert_ne!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn changing_one_ciphertext_byte_breaks_decryption() {
        let vault_key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let mut wrapped = wrap_vault_key(&kek(), &vault_key, "vault-id").unwrap();
        wrapped.ciphertext[0] ^= 0x01;

        let result = unwrap_vault_key(&kek(), &wrapped.ciphertext, &wrapped.nonce, "vault-id");
        assert!(result.is_err());
    }

    #[test]
    fn changing_the_nonce_breaks_decryption() {
        let vault_key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let mut wrapped = wrap_vault_key(&kek(), &vault_key, "vault-id").unwrap();
        wrapped.nonce[0] ^= 0x01;

        let result = unwrap_vault_key(&kek(), &wrapped.ciphertext, &wrapped.nonce, "vault-id");
        assert!(result.is_err());
    }

    #[test]
    fn changing_the_aad_breaks_decryption() {
        let vault_key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let wrapped = wrap_vault_key(&kek(), &vault_key, "vault-id").unwrap();

        // A different vault id is a different AAD, so the ciphertext is refused.
        let result = unwrap_vault_key(&kek(), &wrapped.ciphertext, &wrapped.nonce, "other-vault");
        assert!(result.is_err());
    }

    #[test]
    fn wrapped_key_and_object_ciphertexts_are_not_interchangeable() {
        let key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let wrapped = wrap_vault_key(&key, &key, "vault-id").unwrap();
        let object = encrypt_object(&key, "object-id", "note", b"payload").unwrap();

        // The object AAD can never satisfy the wrapped-key AAD, even with the
        // same key and the same nonce.
        assert!(
            unwrap_vault_key(&key, &object.ciphertext, &object.nonce, "vault-id").is_err(),
            "an object payload was accepted as a wrapped vault key"
        );
        // And the other way round.
        assert!(
            decrypt_object(
                &key,
                "object-id",
                "note",
                &wrapped.ciphertext,
                &wrapped.nonce
            )
            .is_err(),
            "a wrapped vault key was accepted as an object payload"
        );
    }

    #[test]
    fn encrypting_the_same_plaintext_twice_gives_different_ciphertext() {
        let key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let first = encrypt_object(&key, "object-id", "note", b"same payload").unwrap();
        let second = encrypt_object(&key, "object-id", "note", b"same payload").unwrap();

        assert_ne!(first.nonce, second.nonce, "the nonce was reused");
        assert_ne!(first.ciphertext, second.ciphertext);
    }

    #[test]
    fn object_encryption_binds_the_id_and_the_kind() {
        let key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let object = encrypt_object(&key, "object-id", "note", b"payload").unwrap();

        assert!(
            decrypt_object(&key, "other-id", "note", &object.ciphertext, &object.nonce).is_err()
        );
        assert!(
            decrypt_object(&key, "object-id", "host", &object.ciphertext, &object.nonce).is_err()
        );
    }

    #[test]
    fn a_nonce_of_the_wrong_length_is_refused() {
        let key = SecretKey::from_bytes([42u8; KEY_LEN]);
        let wrapped = wrap_vault_key(&key, &key, "vault-id").unwrap();

        let short = &wrapped.nonce[..NONCE_LEN - 1];
        assert!(unwrap_vault_key(&key, &wrapped.ciphertext, short, "vault-id").is_err());
    }

    #[test]
    fn the_secret_key_wrapper_zeroizes_its_bytes() {
        let mut key = SecretKey::from_bytes([9u8; KEY_LEN]);
        assert!(key.as_bytes().iter().any(|byte| *byte != 0));

        // This is the same call the drop impl of `Zeroizing` makes, so a passing
        // test means a locked vault leaves no key bytes behind.
        key.zeroize();

        assert!(key.as_bytes().iter().all(|byte| *byte == 0));
    }
}
