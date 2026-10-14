import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

/**
 * Encrypted backups of the local vault.
 *
 * The crypto, the file work and the restore all happen in Rust; what lives here
 * is the wire model, the three commands, the two native dialogs, and the small
 * decisions the dialogs make before they call anything.
 *
 * Two things are deliberately absent. The backup password is only ever held by
 * the dialog's own state — never in local storage, never in a URL, never in a
 * log — and it is dropped as soon as the work is done, the dialog closes, or the
 * vault locks. And a file path is only ever what the native dialog returned: no
 * path is typed in, and the full path is not shown back to the user.
 */

/** The extension every backup file has. */
export const BACKUP_EXTENSION = 'termust-backup';

/** The shortest backup password the backend accepts. Checked here first. */
export const MIN_BACKUP_PASSWORD_LEN = 12;

/** The phrase a replace requires, matching `REPLACE_CONFIRMATION` in Rust. */
export const RESTORE_PHRASE = 'RESTORE';

/** The largest backup the backend reads, in bytes. */
export const MAX_BACKUP_BYTES = 64 * 1024 * 1024;

/** The only restore mode this build has, matching `RESTORE_MODE_REPLACE`. */
export const RESTORE_MODE = 'replace';

// --- the wire model -------------------------------------------------------

/** How many records a backup holds, by kind. */
export interface KindCount {
	kind: string;
	active: number;
	deleted: number;
}

/** What `inspect_encrypted_backup` answers with: counts, never a secret. */
export interface BackupSummary {
	createdAt: number;
	sourceAppVersion: string;
	totalObjects: number;
	activeObjects: number;
	deletedObjects: number;
	countByKind: KindCount[];
	/**
	 * SHA-256 of the exact file that was read, as the backend reports it.
	 *
	 * It is not a secret; the restore sends it back so the backend can tell that
	 * the file it is about to read is the one the user previewed.
	 */
	fileDigest: string;
}

export interface ExportSummary {
	fileName: string;
	bytes: number;
	createdAt: number;
	totalObjects: number;
	activeObjects: number;
	deletedObjects: number;
	countByKind: KindCount[];
}

export interface RestoreSummary {
	fileName: string;
	createdAt: number;
	replacedObjects: number;
	activeObjects: number;
	deletedObjects: number;
	countByKind: KindCount[];
}

// --- what the dialogs send ------------------------------------------------

/** The body of `export_encrypted_backup`. */
export interface ExportRequestBody {
	destinationPath: string;
	backupPassword: string;
	/** Whether the user already agreed, in the native dialog, to replace a file. */
	overwrite: boolean;
}

export interface InspectRequestBody {
	sourcePath: string;
	backupPassword: string;
}

export interface RestoreRequestBody {
	sourcePath: string;
	backupPassword: string;
	mode: typeof RESTORE_MODE;
	confirmation: string;
	/** The digest the preview reported for this file. */
	fileDigest: string;
}

// --- IPC ------------------------------------------------------------------

/**
 * Encrypts the whole vault to the file the user chose.
 *
 * `overwrite` says that the native save dialog already asked about replacing an
 * existing file. Without it the backend refuses to write over one, so a caller
 * that did not ask cannot destroy a backup by accident.
 */
export function exportEncryptedBackup(request: ExportRequestBody): Promise<ExportSummary> {
	return invoke('export_encrypted_backup', { request });
}

/** Reads a backup file and reports how much data it holds. Writes nothing. */
export function inspectEncryptedBackup(request: InspectRequestBody): Promise<BackupSummary> {
	return invoke('inspect_encrypted_backup', { request });
}

/**
 * Replaces every record in this vault with the contents of a backup file.
 *
 * Both the mode and the typed phrase travel with the request: the backend checks
 * them itself, so a disabled button is never the only thing standing between the
 * user and a replaced vault.
 */
export function restoreEncryptedBackup(request: RestoreRequestBody): Promise<RestoreSummary> {
	return invoke('restore_encrypted_backup', { request });
}

// --- the native dialogs ---------------------------------------------------

/** The file filter both dialogs use. The backend writes nothing else. */
export const BACKUP_FILTERS = [{ name: 'Termust backup', extensions: [BACKUP_EXTENSION] }];

/**
 * Asks for a place to write a backup and returns the chosen path, or null when
 * the user cancelled. A cancelled dialog is not an error.
 *
 * The save dialog is the native one, and it asks before replacing a file that is
 * already there; that answer is what `overwrite` reports to the backend.
 */
export async function chooseBackupToSave(suggestedName: string): Promise<string | null> {
	const chosen = await save({
		title: 'Save encrypted backup',
		defaultPath: suggestedName,
		filters: BACKUP_FILTERS
	});
	return pathOrNull(chosen);
}

/**
 * Asks for the backup file to restore and returns its path, or null when the
 * user cancelled. Only a file can be chosen: no directory, and no typed path.
 */
export async function chooseBackupToOpen(): Promise<string | null> {
	const chosen = await open({
		title: 'Open a Termust backup',
		multiple: false,
		directory: false,
		filters: BACKUP_FILTERS
	});
	return pathOrNull(chosen);
}

/** A dialog answer as a path, or null when nothing was chosen. */
export function pathOrNull(chosen: string | string[] | null): string | null {
	if (typeof chosen === 'string' && chosen.length > 0) {
		return chosen;
	}
	return null;
}

/** Whether a dialog answer means the user cancelled. */
export function cancelled(chosen: string | null): boolean {
	return chosen === null;
}

/**
 * The file name offered in the save dialog: a safe default that says what the
 * file is and when it was made.
 */
export function defaultBackupFileName(now: Date = new Date()): string {
	const year = now.getFullYear().toString().padStart(4, '0');
	const month = (now.getMonth() + 1).toString().padStart(2, '0');
	const day = now.getDate().toString().padStart(2, '0');
	return `termust-backup-${year}-${month}-${day}.${BACKUP_EXTENSION}`;
}

/**
 * The last segment of a path.
 *
 * A summary names the file and never the whole path: the rest of it is the
 * user's own directory layout.
 */
export function fileNameOf(path: string): string {
	const parts = path.split(/[/\\]/);
	return parts[parts.length - 1] ?? path;
}

// --- the export form ------------------------------------------------------

/**
 * Why the export form cannot be submitted, or null when it can.
 *
 * The confirmation only exists here; the length is enforced again in the
 * backend, so the rule holds even if this is never reached.
 */
export function exportPasswordProblem(password: string, confirmation: string): string | null {
	if (password.length < MIN_BACKUP_PASSWORD_LEN) {
		return `The backup password must be at least ${MIN_BACKUP_PASSWORD_LEN} characters.`;
	}
	if (password !== confirmation) {
		return 'The backup password and its confirmation do not match.';
	}
	return null;
}

/** Whether the export button can be pressed. */
export function exportReady(password: string, confirmation: string, busy: boolean): boolean {
	return !busy && exportPasswordProblem(password, confirmation) === null;
}

/** The body of an export, built in one place so a test can read it. */
export function exportRequest(destinationPath: string, backupPassword: string): ExportRequestBody {
	return { destinationPath, backupPassword, overwrite: true };
}

/** The body of an inspect, built in one place so a test can read it. */
export function inspectRequest(sourcePath: string, backupPassword: string): InspectRequestBody {
	return { sourcePath, backupPassword };
}

/**
 * The body of a restore.
 *
 * `mode` and `confirmation` are not optional and are not derived from anything
 * the backend said: every restore names the replace mode and carries the phrase
 * the user typed. `fileDigest` is the one thing that comes from the preview: it
 * is the file the user read, so a file that changed after that is refused
 * instead of silently restored.
 */
export function restoreRequest(
	sourcePath: string,
	backupPassword: string,
	confirmation: string,
	fileDigest: string
): RestoreRequestBody {
	return {
		sourcePath,
		backupPassword,
		mode: RESTORE_MODE,
		confirmation: confirmation.trim(),
		fileDigest
	};
}

// --- the restore form -----------------------------------------------------

/**
 * Why a restore cannot be confirmed, or null when it can.
 *
 * A live session comes first: a restore replaces the records sessions were
 * started from, so it is refused before anything else is considered. Every
 * session counts, not only the one on screen.
 */
export function restoreProblem(
	typed: string,
	hasLiveSessions: boolean,
	busy: boolean
): string | null {
	if (hasLiveSessions) {
		return 'Disconnect every SSH session before restoring a backup.';
	}
	if (typed.trim() !== RESTORE_PHRASE) {
		return `Type ${RESTORE_PHRASE} to confirm that this vault will be replaced.`;
	}
	if (busy) {
		return 'The restore is already running.';
	}
	return null;
}

/** Whether the restore button can be pressed. */
export function restoreReady(typed: string, hasLiveSessions: boolean, busy: boolean): boolean {
	return restoreProblem(typed, hasLiveSessions, busy) === null;
}

/**
 * The warning the confirmation step shows. It is the whole promise a restore
 * makes, so the dialog and its test read it from here.
 */
export function restoreWarningLines(): string[] {
	return [
		'Every group, host, credential, known host and snippet in this vault will be replaced by the contents of the backup.',
		'The vault itself stays: its master password and its key are not changed, and every restored record is encrypted again with them.',
		'This cannot be undone unless you have another backup.',
		'An open SSH session blocks a restore, and a restore never disconnects one for you.'
	];
}

/** The warning the export step shows. */
export function exportWarningLines(): string[] {
	return [
		'The file is encrypted with a password you choose here. The vault master password is not used and is not stored in it.',
		'Nothing can recover the file if you lose that password: there is no reset, and no support key.',
		'A backup replaces nothing until you restore it, and it is not a sync: it is a copy of this vault at this moment.'
	];
}

// --- reading a summary ----------------------------------------------------

/** `3 groups · 2 active, 1 deleted` per kind, ready to render. */
export function kindCountLines(countByKind: KindCount[]): string[] {
	return countByKind.map(
		(count) => `${count.kind.replace('_', ' ')}: ${count.active} active, ${count.deleted} deleted`
	);
}

/** The head line of a summary: how much data the file holds. */
export function summaryLine(summary: BackupSummary | ExportSummary): string {
	const total = summary.totalObjects;
	return `${total} ${total === 1 ? 'record' : 'records'} in this backup: ${summary.activeObjects} active, ${summary.deletedObjects} deleted.`;
}

/** The line a finished restore shows. */
export function restoreResultLine(summary: RestoreSummary): string {
	return `${summary.replacedObjects} ${summary.replacedObjects === 1 ? 'record' : 'records'} restored from ${summary.fileName}.`;
}

/** A byte count as the success step shows it. */
export function formatBytes(bytes: number): string {
	if (bytes < 1024) {
		return `${bytes} B`;
	}
	const kib = bytes / 1024;
	if (kib < 1024) {
		return `${kib.toFixed(1)} KiB`;
	}
	return `${(kib / 1024).toFixed(1)} MiB`;
}

/** When a backup was made, as the summary shows it. */
export function createdAtLabel(createdAt: number): string {
	if (createdAt <= 0) {
		return 'an unknown time';
	}
	return new Date(createdAt * 1000).toLocaleString();
}
