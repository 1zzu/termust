//! Which host keys this process trusts, and where an approval is kept.
//!
//! An approved key is remembered in two places, for two different reasons:
//!
//! - in memory, which is what keeps a *locked* vault usable: the decision holds
//!   for as long as the app runs and is deliberately written nowhere;
//! - in the encrypted vault, which is what makes it survive a restart.
//!
//! The vault is only ever touched from a blocking thread, one call at a time,
//! and never while the connection itself is being set up: the lookups happen
//! before the handshake starts and the write happens after it finished.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};

use crate::inventory::{KnownKey, find_known_host, remember_known_host};
use crate::vault::{VaultHandle, VaultStatus};

/// The host keys trusted during this run, keyed by canonical host identity.
pub(super) type Remembered = Arc<StdMutex<HashMap<String, KnownKey>>>;

/// Where an approved host key is remembered.
///
/// Cheap to clone: every session gets its own copy of the same two handles.
#[derive(Clone)]
pub(super) struct HostKeyTrust {
    remembered: Remembered,
    /// The vault, when the app has one. `None` only in a headless test.
    vault: Option<VaultHandle>,
}

impl HostKeyTrust {
    pub(super) fn new(remembered: Remembered, vault: Option<VaultHandle>) -> Self {
        Self { remembered, vault }
    }

    /// The key recorded for this host, from this run's approvals first and then
    /// from the vault. `None` means the host is unknown and has to be approved.
    ///
    /// While the vault is locked its records cannot be read, so a host that is
    /// only in the vault looks unknown. That is the safe direction: the user is
    /// asked again instead of a key being trusted on a guess.
    pub(super) async fn lookup(&self, host_key_id: &str) -> Option<KnownKey> {
        if let Some(key) = self.memory().get(host_key_id).cloned() {
            return Some(key);
        }

        let vault = self.vault.clone()?;
        let id = host_key_id.to_string();
        // Opening the database and decrypting one payload blocks the thread it
        // runs on, so it happens on the blocking pool rather than on the runtime
        // that is driving the connection.
        tokio::task::spawn_blocking(move || {
            vault.with(|vault| match find_known_host(vault, &id) {
                Ok(record) => record.map(|record| record.key()),
                // A locked vault cannot be read, so nothing is recorded.
                Err(_) => None,
            })
        })
        .await
        .unwrap_or(None)
    }

    /// Whether an approval given now would still be trusted after a restart.
    ///
    /// This is what the prompt tells the user, so it asks the vault rather than
    /// the frontend: only the backend knows whether the vault is really open.
    pub(super) async fn can_persist(&self) -> bool {
        let Some(vault) = self.vault.clone() else {
            return false;
        };
        tokio::task::spawn_blocking(move || {
            vault.with(|vault| matches!(vault.status(), Ok(VaultStatus::Unlocked)))
        })
        .await
        .unwrap_or(false)
    }

    /// Records an approval: always in memory, and in the vault when it is open.
    ///
    /// A write that fails is not fatal. The connection is already verified, and
    /// the cost of losing the record is one more prompt next time, which is the
    /// safe direction to fail in.
    pub(super) async fn remember(&self, host_key_id: &str, key: &KnownKey) {
        self.memory().insert(host_key_id.to_string(), key.clone());

        let Some(vault) = self.vault.clone() else {
            return;
        };
        let id = host_key_id.to_string();
        let key = key.clone();
        let _ = tokio::task::spawn_blocking(move || {
            vault.with(|vault| remember_known_host(vault, &id, &key))
        })
        .await;
    }

    fn memory(&self) -> MutexGuard<'_, HashMap<String, KnownKey>> {
        // A poisoned lock only means an earlier session panicked; the map itself
        // is still usable, so recover instead of losing every later approval.
        self.remembered
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{VaultState, test_support::TempVault};

    const PASSWORD: &str = "a-strong-master-password";

    fn key(fingerprint: &str) -> KnownKey {
        KnownKey {
            algorithm: "ssh-ed25519".to_string(),
            fingerprint: fingerprint.to_string(),
        }
    }

    /// A temporary vault that has been created, either left open or locked, with
    /// a handle on it exactly as the app holds it.
    fn vault(open: bool) -> (TempVault, VaultHandle) {
        let temp = TempVault::new();
        let mut vault = temp.vault();
        vault.create(PASSWORD).expect("the vault was created");
        if !open {
            vault.lock();
        }
        (temp, VaultState::new(vault).handle())
    }

    /// A trust with the same database but nothing in memory, as a restarted app
    /// would have. The vault is opened again first.
    fn after_restart(temp: &TempVault) -> HostKeyTrust {
        let mut vault = temp.vault();
        vault.unlock(PASSWORD).expect("the vault unlocked");
        HostKeyTrust::new(Arc::default(), Some(VaultState::new(vault).handle()))
    }

    #[tokio::test]
    async fn an_empty_vault_knows_no_host() {
        let (_temp, handle) = vault(true);
        let trust = HostKeyTrust::new(Arc::default(), Some(handle));

        assert_eq!(trust.lookup("example.com:22").await, None);
        assert!(
            trust.can_persist().await,
            "an open vault can keep an approval"
        );
    }

    #[tokio::test]
    async fn a_locked_vault_keeps_an_approval_in_memory_only() {
        let (temp, handle) = vault(false);
        let trust = HostKeyTrust::new(Arc::default(), Some(handle));

        assert_eq!(trust.lookup("example.com:22").await, None);
        assert!(
            !trust.can_persist().await,
            "a locked vault cannot keep an approval"
        );

        // The approval still works for this run.
        trust.remember("example.com:22", &key("SHA256:abc")).await;
        assert_eq!(
            trust.lookup("example.com:22").await,
            Some(key("SHA256:abc"))
        );

        assert_eq!(
            after_restart(&temp).lookup("example.com:22").await,
            None,
            "an approval made while locked must not reach the database"
        );
    }

    #[tokio::test]
    async fn a_headless_trust_remembers_nothing_but_this_run() {
        let trust = HostKeyTrust::new(Arc::default(), None);

        assert_eq!(trust.lookup("example.com:22").await, None);
        assert!(!trust.can_persist().await);

        trust.remember("example.com:22", &key("SHA256:abc")).await;
        assert_eq!(
            trust.lookup("example.com:22").await,
            Some(key("SHA256:abc"))
        );
    }

    #[tokio::test]
    async fn an_open_vault_keeps_an_approval_across_a_restart() {
        let (temp, handle) = vault(true);
        let trust = HostKeyTrust::new(Arc::default(), Some(handle));
        assert!(trust.can_persist().await);

        trust.remember("example.com:22", &key("SHA256:abc")).await;

        // A new process: nothing in memory, the vault opened again.
        let restarted = after_restart(&temp);
        assert_eq!(
            restarted.lookup("example.com:22").await,
            Some(key("SHA256:abc"))
        );
        assert!(restarted.can_persist().await);
    }

    #[tokio::test]
    async fn a_changed_key_replaces_what_the_vault_keeps() {
        let (temp, handle) = vault(true);
        let trust = HostKeyTrust::new(Arc::default(), Some(handle));

        trust.remember("example.com:22", &key("SHA256:first")).await;
        trust
            .remember("example.com:22", &key("SHA256:second"))
            .await;

        assert_eq!(
            after_restart(&temp).lookup("example.com:22").await,
            Some(key("SHA256:second")),
            "one host identity keeps exactly one key"
        );
    }

    #[tokio::test]
    async fn ports_are_separate_identities() {
        let (temp, handle) = vault(true);
        let trust = HostKeyTrust::new(Arc::default(), Some(handle));

        trust.remember("example.com:22", &key("SHA256:abc")).await;

        assert_eq!(trust.lookup("example.com:2222").await, None);
        assert_eq!(after_restart(&temp).lookup("example.com:2222").await, None);
        assert_eq!(
            after_restart(&temp).lookup("example.com:22").await,
            Some(key("SHA256:abc"))
        );
    }
}
