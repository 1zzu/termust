//! The vault-aware half of the import: parse a config, compare it with the hosts
//! that are already saved, and write the entries the user picked.
//!
//! Nothing here opens a file, so a config cannot name something this code reads.
//! The only writing is one new `host` object per selected entry, created through
//! the same service the host form uses, so an imported host is an ordinary host
//! with the same validation, encryption and relationships as any other: no
//! group, no credential, and nothing that records where it came from.

use std::collections::HashSet;
use std::net::IpAddr;

use crate::inventory::{Host, HostInput};
use crate::vault::Vault;

use super::OpenSshError;
use super::model::{
    ConfigWarning, EntryStatus, ImportEntry, ImportOutcome, ImportPreview, ImportResultEntry,
    ImportSummary, MAX_CONFIG_BYTES,
};
use super::parser::parse;

/// Parses the config and marks what is already saved. Writes nothing: the only
/// vault call is a read of the saved hosts.
pub fn preview(vault: &Vault, content: &str) -> Result<ImportPreview, OpenSshError> {
    let content = usable(content)?;
    let mut preview = parse(content);
    let hosts = crate::inventory::list_host_records(vault)?;

    for entry in &mut preview.entries {
        if entry.status != EntryStatus::Ready {
            continue;
        }

        if same_target(entry, &hosts) {
            entry.status = EntryStatus::AlreadyExists;
            entry.warnings.push(ConfigWarning {
                line: None,
                message: "A saved host already has this address, port and username.".to_string(),
            });
            continue;
        }

        // A label used by another host is worth saying out loud, but it changes
        // nothing: the entry is added beside that host, never over it.
        if hosts
            .iter()
            .any(|host| host.label.eq_ignore_ascii_case(entry.label.trim()))
        {
            entry.warnings.push(ConfigWarning {
                line: None,
                message: "A saved host already uses this label. This entry will be added as a second host, not overwrite it.".to_string(),
            });
        }
    }

    Ok(preview)
}

/// Imports the selected entries, one at a time.
///
/// The config is parsed again here, with the same parser, because a preview is
/// not something the frontend can be trusted to hand back unchanged. The
/// duplicate check is run again for the same reason: importing the same config
/// twice skips what the first run created instead of saving it twice.
pub fn import(
    vault: &Vault,
    content: &str,
    selected: &[usize],
) -> Result<ImportSummary, OpenSshError> {
    let preview = parse(usable(content)?);
    let selected = checked_selection(&preview, selected)?;
    // Read once, then kept up to date with what this run writes, so one import
    // cannot save the same target twice either.
    let mut saved = crate::inventory::list_host_records(vault)?;

    let mut results = Vec::with_capacity(selected.len());
    for index in selected {
        results.push(import_one(vault, &preview.entries[index], &mut saved));
    }

    Ok(ImportSummary::new(results))
}

fn import_one(vault: &Vault, entry: &ImportEntry, saved: &mut Vec<Host>) -> ImportResultEntry {
    let result = |outcome: ImportOutcome, reason: Option<String>, host_id: Option<String>| {
        ImportResultEntry {
            index: entry.index,
            label: entry.label.clone(),
            outcome,
            reason,
            host_id,
        }
    };

    if entry.status != EntryStatus::Ready {
        return result(
            ImportOutcome::Skipped,
            Some("This entry is not one Termust can import.".to_string()),
            None,
        );
    }

    if same_target(entry, saved) {
        return result(
            ImportOutcome::Skipped,
            Some("A saved host already has this address, port and username.".to_string()),
            None,
        );
    }

    let Some(port) = entry.port else {
        return result(
            ImportOutcome::Failed,
            Some("This entry has no usable port.".to_string()),
            None,
        );
    };

    let input = HostInput {
        label: entry.label.clone(),
        address: entry.address.clone(),
        port: i64::from(port),
        username: entry.username.clone(),
        group_id: None,
        credential_id: None,
    };

    match crate::inventory::create_host_record(vault, &input) {
        Ok(host) => {
            saved.push(host.clone());
            result(ImportOutcome::Imported, None, Some(host.id))
        }
        Err(error) => result(ImportOutcome::Failed, Some(error.to_string()), None),
    }
}

/// Whether a saved host already is the connection this entry describes.
///
/// This is deliberately small, and it is the only place a target is compared:
/// the address and the username are trimmed, a hostname is compared without
/// regard to case, an IP literal is compared exactly as written, and the port is
/// compared as the number it is.
pub fn same_target(entry: &ImportEntry, hosts: &[Host]) -> bool {
    let Some(port) = entry.port else {
        return false;
    };
    let address = fold_address(&entry.address);
    let username = entry.username.trim();

    hosts.iter().any(|host| {
        host.port == port
            && fold_address(&host.address) == address
            && host.username.trim() == username
    })
}

/// A hostname is not case-sensitive, so one is folded before it is compared. An
/// IP literal is not touched: `192.0.2.1` is compared as it is written, never
/// rewritten into some other spelling of the same address.
fn fold_address(address: &str) -> String {
    let address = address.trim();
    if is_ip_literal(address) {
        address.to_string()
    } else {
        address.to_lowercase()
    }
}

fn is_ip_literal(address: &str) -> bool {
    let bare = address
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(address);
    bare.parse::<IpAddr>().is_ok()
}

/// The config text, refused before it is parsed when it is empty or too big.
fn usable(content: &str) -> Result<&str, OpenSshError> {
    if content.trim().is_empty() {
        return Err(OpenSshError::Invalid(
            "Paste a config or choose a file first.".to_string(),
        ));
    }
    if content.len() > MAX_CONFIG_BYTES {
        return Err(OpenSshError::Invalid(format!(
            "The config is larger than {MAX_CONFIG_BYTES} bytes."
        )));
    }
    Ok(content)
}

/// The selected indexes, checked before anything is written.
///
/// Every index has to name an entry of this parse, and no index may be sent
/// twice. The message names the number only; nothing from the config is
/// repeated.
fn checked_selection(
    preview: &ImportPreview,
    selected: &[usize],
) -> Result<Vec<usize>, OpenSshError> {
    if selected.is_empty() {
        return Err(OpenSshError::Invalid(
            "Select at least one host to import.".to_string(),
        ));
    }

    let mut seen = HashSet::new();
    for index in selected {
        if *index >= preview.entries.len() {
            return Err(OpenSshError::Invalid(format!(
                "The config has no entry at position {index}."
            )));
        }
        if !seen.insert(*index) {
            return Err(OpenSshError::Invalid(format!(
                "Entry {index} was selected twice."
            )));
        }
    }

    Ok(selected.to_vec())
}
