//! Temporary-database scaffolding shared by the vault and inventory tests.
//!
//! Every test gets its own directory and removes it again, so no test ever
//! touches a real vault or the app data directory.

use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use super::{DB_FILE, Vault};

/// A temporary directory that removes itself, and the database path inside it.
pub(crate) struct TempVault {
    directory: PathBuf,
}

impl TempVault {
    pub(crate) fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("termust-vault-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("the temporary directory was created");
        Self { directory }
    }

    /// The database file inside the temporary directory.
    pub(crate) fn database(&self) -> PathBuf {
        self.directory.join(DB_FILE)
    }

    /// A fresh handle on the same database, as if the app had been restarted.
    pub(crate) fn vault(&self) -> Vault {
        Vault::with_test_kdf(self.database())
    }
}

impl Drop for TempVault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
