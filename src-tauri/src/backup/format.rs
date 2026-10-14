//! The bytes of a backup file: encrypting a payload into one, and decrypting one
//! back.
//!
//! Nothing here touches a database or a file. `encode` and `decode` work on
//! buffers, which is what makes the format testable on its own and keeps the
//! file handling in `service.rs`.
//!
//! The crypto is the vault's own: the same Argon2id, the same
//! XChaCha20-Poly1305, called through the same audited wrappers. What is
//! different is the *key* — a backup password derives its own key, which opens
//! the file and nothing else — and the Additional Authenticated Data, which is
//! the fixed, format-versioned backup identity, so a backup payload can never be
//! read as a vault object or a wrapped Vault Key, and a later format version
//! cannot read v1 bytes.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use zeroize::Zeroizing;

use crate::vault::crypto::{
    KdfParams, NONCE_LEN, SALT_LEN, derive_kek, open_backup, random_bytes, seal_backup,
    validate_kdf,
};

use super::BackupError;
use super::model::{
    BACKUP_AAD, BACKUP_CIPHER, BACKUP_KDF, BackupEnvelope, BackupPayload, EnvelopeKdf,
    FORMAT_VERSION, MAGIC, MAX_BACKUP_BYTES, MAX_PAYLOAD_BYTES, PAYLOAD_VERSION,
};

/// Encrypts a payload into the bytes of a backup file.
///
/// A fresh random salt and a fresh random nonce are drawn here, so two exports of
/// the same vault share no key material and no ciphertext. The parameters come
/// from the caller, which is what lets a test use cheap ones while the app uses
/// the production cost.
pub(super) fn encode(
    payload: &BackupPayload,
    password: &str,
    kdf: KdfParams,
) -> Result<Vec<u8>, BackupError> {
    validate_kdf(kdf).map_err(|_| BackupError::Kdf)?;

    let salt = random_bytes(SALT_LEN)?;
    let nonce = random_bytes(NONCE_LEN)?;

    // The serialised snapshot is the largest secret this module handles: it holds
    // every record, credentials included. It lives in a buffer that wipes itself.
    let plaintext =
        Zeroizing::new(serde_json::to_vec(payload).map_err(|_| BackupError::InvalidSnapshot)?);
    if plaintext.len() > MAX_PAYLOAD_BYTES {
        return Err(BackupError::TooLarge);
    }

    let key = derive_kek(password, &salt, kdf)?;
    let ciphertext = seal_backup(&key, &nonce, &plaintext, BACKUP_AAD)?;

    let envelope = BackupEnvelope {
        magic: MAGIC.to_string(),
        format_version: FORMAT_VERSION,
        kdf: EnvelopeKdf {
            algorithm: BACKUP_KDF.to_string(),
            salt: BASE64.encode(&salt),
            memory_cost: kdf.memory_cost,
            time_cost: kdf.time_cost,
            parallelism: kdf.parallelism,
        },
        cipher: BACKUP_CIPHER.to_string(),
        nonce: BASE64.encode(&nonce),
        ciphertext: BASE64.encode(&ciphertext),
    };

    // Pretty-printed on purpose: the head is meant to be readable by a person
    // debugging a file, and the ciphertext is one long line either way.
    serde_json::to_vec_pretty(&envelope).map_err(|_| BackupError::InvalidSnapshot)
}

/// Decrypts a backup file into its payload.
///
/// Every check that can be made before a key is derived is made first: the magic,
/// the format version, the algorithm names, the key-derivation bounds and the
/// shape of the base64. A wrong password and a damaged tag are indistinguishable
/// here, which is what keeps this from being a password oracle.
pub(super) fn decode(bytes: &[u8], password: &str) -> Result<BackupPayload, BackupError> {
    if bytes.len() > MAX_BACKUP_BYTES {
        return Err(BackupError::TooLarge);
    }

    let envelope: BackupEnvelope =
        serde_json::from_slice(bytes).map_err(|_| BackupError::NotABackup)?;
    if envelope.magic != MAGIC {
        return Err(BackupError::NotABackup);
    }
    if envelope.format_version != FORMAT_VERSION {
        return Err(BackupError::UnsupportedVersion {
            found: envelope.format_version,
            supported: FORMAT_VERSION,
        });
    }
    if envelope.cipher != BACKUP_CIPHER {
        return Err(BackupError::UnsupportedCipher);
    }
    if envelope.kdf.algorithm != BACKUP_KDF {
        return Err(BackupError::UnsupportedKdf);
    }

    // The parameters come from the file, so they are bounded before Argon2id ever
    // sees them: a hostile file must not be able to ask for gigabytes or for a
    // core busy for minutes.
    let params = KdfParams {
        memory_cost: envelope.kdf.memory_cost,
        time_cost: envelope.kdf.time_cost,
        parallelism: envelope.kdf.parallelism,
    };
    validate_kdf(params).map_err(|_| BackupError::Kdf)?;

    let salt = decode_base64(&envelope.kdf.salt)?;
    if salt.len() != SALT_LEN {
        return Err(BackupError::NotABackup);
    }
    let nonce = decode_base64(&envelope.nonce)?;
    if nonce.len() != NONCE_LEN {
        return Err(BackupError::NotABackup);
    }
    let ciphertext = decode_base64(&envelope.ciphertext)?;
    if ciphertext.len() > MAX_BACKUP_BYTES {
        return Err(BackupError::TooLarge);
    }

    let key = derive_kek(password, &salt, params)?;
    let plaintext = Zeroizing::new(
        open_backup(&key, &nonce, &ciphertext, BACKUP_AAD).map_err(|_| BackupError::CannotOpen)?,
    );
    if plaintext.len() > MAX_PAYLOAD_BYTES {
        return Err(BackupError::TooLarge);
    }

    // The payload is deserialised straight out of the decrypted buffer. There
    // is deliberately no intermediate `serde_json::Value` here: such a tree
    // would hold a second copy of every credential secret and snippet in the
    // file, and only the payload type's own fields can wipe themselves.
    let payload: BackupPayload =
        serde_json::from_slice(&plaintext).map_err(|_| BackupError::CannotOpen)?;

    // The version is checked once the payload is decoded, so a file written by a
    // later build is reported as a version this build does not support rather
    // than as a file that failed to parse. A file whose version does not even
    // fit the field is a damaged file and takes the generic open message.
    if payload.payload_version != PAYLOAD_VERSION {
        return Err(BackupError::UnsupportedPayloadVersion {
            found: u64::from(payload.payload_version),
            supported: PAYLOAD_VERSION,
        });
    }

    Ok(payload)
}

fn decode_base64(value: &str) -> Result<Vec<u8>, BackupError> {
    BASE64.decode(value).map_err(|_| BackupError::NotABackup)
}
