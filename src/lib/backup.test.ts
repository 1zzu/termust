import { beforeEach, describe, expect, it, vi } from 'vitest';

// The IPC boundary and the two native dialogs are the only things mocked here:
// these tests pin the command names, the argument names and the request bodies
// the backend has to answer to.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

import {
	BACKUP_FILTERS,
	MAX_BACKUP_BYTES,
	MIN_BACKUP_PASSWORD_LEN,
	RESTORE_PHRASE,
	cancelled,
	chooseBackupToOpen,
	chooseBackupToSave,
	createdAtLabel,
	defaultBackupFileName,
	exportEncryptedBackup,
	exportPasswordProblem,
	exportReady,
	exportRequest,
	exportWarningLines,
	fileNameOf,
	formatBytes,
	inspectEncryptedBackup,
	inspectRequest,
	kindCountLines,
	pathOrNull,
	restoreEncryptedBackup,
	restoreProblem,
	restoreReady,
	restoreRequest,
	restoreResultLine,
	restoreWarningLines,
	summaryLine,
	type BackupSummary,
	type ExportSummary,
	type RestoreSummary
} from './backup';

/** A backup password long enough to pass, with a marker a leak test can look for. */
const PASSWORD = 'marker-backup-password-4d7e';
const OTHER_PASSWORD = 'marker-vault-password-8a1c';
const SOURCE_PATH = '/Users/someone/Backups/termust-backup-2026-10-14.termust-backup';
const DESTINATION_PATH = '/Users/someone/Backups/chosen.termust-backup';
/** What a preview reports for the file above; the restore sends it back. */
const FILE_DIGEST = 'sha256:marker-file-digest-7c2a';

function summary(overrides: Partial<BackupSummary> = {}): BackupSummary {
	return {
		createdAt: 1_700_000_000,
		sourceAppVersion: '0.1.0',
		totalObjects: 5,
		activeObjects: 4,
		deletedObjects: 1,
		countByKind: [
			{ kind: 'group', active: 1, deleted: 0 },
			{ kind: 'host', active: 1, deleted: 1 },
			{ kind: 'known_host', active: 1, deleted: 0 },
			{ kind: 'snippet', active: 1, deleted: 0 }
		],
		fileDigest: FILE_DIGEST,
		...overrides
	};
}

beforeEach(() => {
	vi.mocked(invoke).mockReset();
	vi.mocked(open).mockReset();
	vi.mocked(save).mockReset();
});

describe('exportPasswordProblem', () => {
	it('refuses a password below the minimum length', () => {
		expect(exportPasswordProblem('short-one', 'short-one')).toBe(
			`The backup password must be at least ${MIN_BACKUP_PASSWORD_LEN} characters.`
		);
		expect(exportPasswordProblem('eleven-char', 'eleven-char')).toBe(
			`The backup password must be at least ${MIN_BACKUP_PASSWORD_LEN} characters.`
		);
	});

	it('refuses a confirmation that does not match', () => {
		expect(exportPasswordProblem(PASSWORD, `${PASSWORD}!`)).toBe(
			'The backup password and its confirmation do not match.'
		);
		expect(exportPasswordProblem(PASSWORD, '')).toBe(
			'The backup password and its confirmation do not match.'
		);
	});

	it('accepts a long enough password that is confirmed', () => {
		expect(exportPasswordProblem(PASSWORD, PASSWORD)).toBeNull();
		// Twelve characters is the minimum, counted in characters.
		expect(exportPasswordProblem('twelve-chars', 'twelve-chars')).toBeNull();
	});

	it('never repeats the password it refused', () => {
		for (const problem of [
			exportPasswordProblem('short-one', 'short-one'),
			exportPasswordProblem(PASSWORD, 'other'),
			restoreProblem('not-the-phrase', false, false)
		]) {
			expect(problem ?? '').not.toContain(PASSWORD);
		}
	});
});

describe('exportReady', () => {
	it('is only true for a valid form that is not busy', () => {
		expect(exportReady(PASSWORD, PASSWORD, false)).toBe(true);
		expect(exportReady(PASSWORD, PASSWORD, true)).toBe(false);
		expect(exportReady(PASSWORD, 'other', false)).toBe(false);
		expect(exportReady('short', 'short', false)).toBe(false);
	});
});

describe('defaultBackupFileName', () => {
	it('names the file after the day it was made', () => {
		expect(defaultBackupFileName(new Date(2026, 9, 14))).toBe(
			'termust-backup-2026-10-14.termust-backup'
		);
		expect(defaultBackupFileName(new Date(2026, 0, 3))).toBe(
			'termust-backup-2026-01-03.termust-backup'
		);
	});

	it('is a file name and nothing else', () => {
		const name = defaultBackupFileName(new Date(2026, 9, 14));
		expect(name).not.toContain('/');
		expect(fileNameOf(name)).toBe(name);
	});
});

describe('fileNameOf', () => {
	it('shows the last segment of a path', () => {
		expect(fileNameOf(SOURCE_PATH)).toBe('termust-backup-2026-10-14.termust-backup');
		expect(fileNameOf('C:\\Users\\someone\\backup.termust-backup')).toBe('backup.termust-backup');
		expect(fileNameOf('backup.termust-backup')).toBe('backup.termust-backup');
	});
});

describe('the native dialogs', () => {
	it('asks for a place to write with a safe default name and filter', async () => {
		vi.mocked(save).mockResolvedValue(DESTINATION_PATH);

		await expect(chooseBackupToSave('termust-backup-2026-10-14.termust-backup')).resolves.toBe(
			DESTINATION_PATH
		);

		expect(save).toHaveBeenCalledWith({
			title: 'Save encrypted backup',
			defaultPath: 'termust-backup-2026-10-14.termust-backup',
			filters: BACKUP_FILTERS
		});
	});

	it('asks for one file, never a folder', async () => {
		vi.mocked(open).mockResolvedValue(SOURCE_PATH);

		await expect(chooseBackupToOpen()).resolves.toBe(SOURCE_PATH);

		expect(open).toHaveBeenCalledWith({
			title: 'Open a Termust backup',
			multiple: false,
			directory: false,
			filters: BACKUP_FILTERS
		});
	});

	it('treats a cancelled dialog as no answer, not as an error', async () => {
		vi.mocked(save).mockResolvedValue(null);
		vi.mocked(open).mockResolvedValue(null);

		await expect(chooseBackupToSave('x.termust-backup')).resolves.toBeNull();
		await expect(chooseBackupToOpen()).resolves.toBeNull();

		expect(cancelled(null)).toBe(true);
		expect(cancelled(DESTINATION_PATH)).toBe(false);
		expect(pathOrNull(null)).toBeNull();
		expect(pathOrNull('')).toBeNull();
		expect(pathOrNull(['/one', '/two'])).toBeNull();
		expect(pathOrNull(SOURCE_PATH)).toBe(SOURCE_PATH);
	});
});

describe('the request bodies', () => {
	it('sends an export with the path the dialog returned and the confirmation', () => {
		expect(exportRequest(DESTINATION_PATH, PASSWORD)).toEqual({
			destinationPath: DESTINATION_PATH,
			backupPassword: PASSWORD,
			// The native save dialog already asked about replacing a file.
			overwrite: true
		});
	});

	it('sends an inspect with the chosen file and the password', () => {
		expect(inspectRequest(SOURCE_PATH, PASSWORD)).toEqual({
			sourcePath: SOURCE_PATH,
			backupPassword: PASSWORD
		});
	});

	it('sends a restore that names the replace mode, the typed phrase and the previewed file', () => {
		expect(restoreRequest(SOURCE_PATH, PASSWORD, RESTORE_PHRASE, FILE_DIGEST)).toEqual({
			sourcePath: SOURCE_PATH,
			backupPassword: PASSWORD,
			mode: 'replace',
			confirmation: 'RESTORE',
			fileDigest: FILE_DIGEST
		});

		// The phrase is trimmed, so a stray space in the field is not a refusal
		// from the backend that the user cannot see the reason for.
		expect(
			restoreRequest(SOURCE_PATH, PASSWORD, `  ${RESTORE_PHRASE} `, FILE_DIGEST).confirmation
		).toBe(RESTORE_PHRASE);
	});

	it('carries no field the backend did not ask for', () => {
		for (const body of [
			exportRequest(DESTINATION_PATH, PASSWORD),
			inspectRequest(SOURCE_PATH, PASSWORD),
			restoreRequest(SOURCE_PATH, PASSWORD, RESTORE_PHRASE, FILE_DIGEST)
		]) {
			const fields = Object.keys(body).sort();
			expect(fields).toEqual(
				fields.filter((field) =>
					[
						'backupPassword',
						'confirmation',
						'destinationPath',
						'fileDigest',
						'mode',
						'overwrite',
						'sourcePath'
					].includes(field)
				)
			);
		}
	});
});

describe('the commands', () => {
	it('names the three commands and nests the request', async () => {
		vi.mocked(invoke).mockResolvedValue({});

		await exportEncryptedBackup(exportRequest(DESTINATION_PATH, PASSWORD));
		expect(invoke).toHaveBeenLastCalledWith('export_encrypted_backup', {
			request: {
				destinationPath: DESTINATION_PATH,
				backupPassword: PASSWORD,
				overwrite: true
			}
		});

		await inspectEncryptedBackup(inspectRequest(SOURCE_PATH, PASSWORD));
		expect(invoke).toHaveBeenLastCalledWith('inspect_encrypted_backup', {
			request: { sourcePath: SOURCE_PATH, backupPassword: PASSWORD }
		});

		await restoreEncryptedBackup(
			restoreRequest(SOURCE_PATH, PASSWORD, RESTORE_PHRASE, FILE_DIGEST)
		);
		expect(invoke).toHaveBeenLastCalledWith('restore_encrypted_backup', {
			request: {
				sourcePath: SOURCE_PATH,
				backupPassword: PASSWORD,
				mode: 'replace',
				confirmation: RESTORE_PHRASE,
				fileDigest: FILE_DIGEST
			}
		});
	});

	it('reports the backend error as it is', async () => {
		vi.mocked(invoke).mockRejectedValue(
			'The backup could not be opened. The password is wrong or the file is damaged.'
		);

		await expect(inspectEncryptedBackup(inspectRequest(SOURCE_PATH, OTHER_PASSWORD))).rejects.toBe(
			'The backup could not be opened. The password is wrong or the file is damaged.'
		);
	});
});

describe('restoreProblem', () => {
	it('refuses a restore while a session is live, before anything else', () => {
		expect(restoreProblem(RESTORE_PHRASE, true, false)).toBe(
			'Disconnect every SSH session before restoring a backup.'
		);
		// Even a wrong phrase reports the session first: the session is the
		// blocker the user has to clear.
		expect(restoreProblem('nope', true, false)).toBe(
			'Disconnect every SSH session before restoring a backup.'
		);
	});

	it('requires the exact phrase', () => {
		expect(restoreProblem('', false, false)).toBe(
			`Type ${RESTORE_PHRASE} to confirm that this vault will be replaced.`
		);
		expect(restoreProblem('restore', false, false)).toBe(
			`Type ${RESTORE_PHRASE} to confirm that this vault will be replaced.`
		);
		expect(restoreProblem('REPLACE', false, false)).toBe(
			`Type ${RESTORE_PHRASE} to confirm that this vault will be replaced.`
		);
		expect(restoreProblem(RESTORE_PHRASE, false, false)).toBeNull();
		// Surrounding whitespace is trimmed rather than refused.
		expect(restoreProblem(` ${RESTORE_PHRASE}\n`, false, false)).toBeNull();
	});

	it('refuses a second submit while one is running', () => {
		expect(restoreProblem(RESTORE_PHRASE, false, true)).toBe('The restore is already running.');
	});

	it('never repeats the phrase it refused', () => {
		for (const problem of [
			restoreProblem('wrong', false, false),
			restoreProblem(RESTORE_PHRASE, true, false)
		]) {
			expect(problem ?? '').not.toContain(PASSWORD);
		}
	});
});

describe('restoreReady', () => {
	it('is only true for the right phrase and no session', () => {
		expect(restoreReady(RESTORE_PHRASE, false, false)).toBe(true);
		expect(restoreReady(RESTORE_PHRASE, true, false)).toBe(false);
		expect(restoreReady(RESTORE_PHRASE, false, true)).toBe(false);
		expect(restoreReady('RESTORE!', false, false)).toBe(false);
	});
});

describe('the warnings', () => {
	it('says what a restore replaces and what it keeps', () => {
		const lines = restoreWarningLines().join(' ');
		expect(lines).toContain('replaced');
		expect(lines).toContain('master password');
		expect(lines).toContain('cannot be undone');
		expect(lines).toContain('SSH session');
		expect(lines).not.toContain(PASSWORD);
	});

	it('says the backup password cannot be recovered', () => {
		const lines = exportWarningLines().join(' ');
		expect(lines).toContain('Nothing can recover the file');
		expect(lines).toContain('not a sync');
		expect(lines).not.toContain(PASSWORD);
	});
});

describe('reading a summary', () => {
	it('counts by kind, one line each', () => {
		expect(kindCountLines(summary().countByKind)).toEqual([
			'group: 1 active, 0 deleted',
			'host: 1 active, 1 deleted',
			'known host: 1 active, 0 deleted',
			'snippet: 1 active, 0 deleted'
		]);
		expect(kindCountLines([])).toEqual([]);
	});

	it('leads with the totals', () => {
		expect(summaryLine(summary())).toBe('5 records in this backup: 4 active, 1 deleted.');
		expect(summaryLine(summary({ totalObjects: 1, activeObjects: 1, deletedObjects: 0 }))).toBe(
			'1 record in this backup: 1 active, 0 deleted.'
		);
		expect(summaryLine(summary({ totalObjects: 0, activeObjects: 0, deletedObjects: 0 }))).toBe(
			'0 records in this backup: 0 active, 0 deleted.'
		);
	});

	it('reports a finished restore by its file name', () => {
		const restored: RestoreSummary = {
			fileName: 'termust-backup-2026-10-14.termust-backup',
			createdAt: 1_700_000_000,
			replacedObjects: 6,
			activeObjects: 5,
			deletedObjects: 1,
			countByKind: []
		};
		expect(restoreResultLine(restored)).toBe(
			'6 records restored from termust-backup-2026-10-14.termust-backup.'
		);
		expect(restoreResultLine({ ...restored, replacedObjects: 1 })).toBe(
			'1 record restored from termust-backup-2026-10-14.termust-backup.'
		);
	});

	it('renders a size and a time without a full path', () => {
		expect(formatBytes(512)).toBe('512 B');
		expect(formatBytes(2048)).toBe('2.0 KiB');
		expect(formatBytes(3 * 1024 * 1024)).toBe('3.0 MiB');
		expect(MAX_BACKUP_BYTES).toBe(64 * 1024 * 1024);

		const label = createdAtLabel(1_700_000_000);
		expect(label).not.toContain('/Users/');
		expect(label.length).toBeGreaterThan(0);
		expect(createdAtLabel(0)).toBe('an unknown time');

		const exported: ExportSummary = {
			fileName: 'chosen.termust-backup',
			bytes: 2048,
			createdAt: 1_700_000_000,
			totalObjects: 5,
			activeObjects: 4,
			deletedObjects: 1,
			countByKind: []
		};
		expect(summaryLine(exported)).toBe('5 records in this backup: 4 active, 1 deleted.');
		expect(`${exported.fileName} ${formatBytes(exported.bytes)}`).not.toContain('/Users/');
	});
});
