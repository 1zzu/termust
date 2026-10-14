import { invoke } from '@tauri-apps/api/core';

/**
 * Importing hosts from a basic OpenSSH config. The parsing, the duplicate check
 * and the writes all happen in Rust; what lives here is the wire model, the two
 * commands, and the small decisions the dialog makes before it calls them.
 *
 * Nothing in this file reads a file, and the config never reaches local storage.
 * The text is only ever held by the dialog's own state, and only until the vault
 * closes or the dialog does.
 */

/** The largest config the backend accepts, in bytes. Checked here first. */
export const MAX_CONFIG_BYTES = 1024 * 1024;

/** How one entry of a preview ended up, as `EntryStatus` in Rust serializes. */
export type EntryStatus = 'ready' | 'already_exists' | 'unsupported' | 'invalid';

/** A note about one line of the config, or about the config as a whole. */
export interface ConfigWarning {
	/** The 1-based line the note is about, or null when it is about no line. */
	line: number | null;
	message: string;
}

/** One `Host` block, as the preview shows it. */
export interface ImportEntry {
	index: number;
	label: string;
	address: string;
	port: number | null;
	username: string;
	status: EntryStatus;
	warnings: ConfigWarning[];
}

export interface ImportPreview {
	entries: ImportEntry[];
	warnings: ConfigWarning[];
}

export type ImportOutcome = 'imported' | 'skipped' | 'failed';

export interface ImportResultEntry {
	index: number;
	label: string;
	outcome: ImportOutcome;
	reason?: string;
	hostId?: string;
}

/** What `import_openssh_hosts` answers with. The import is not atomic. */
export interface ImportSummary {
	imported: number;
	skipped: number;
	failed: number;
	results: ImportResultEntry[];
}

// --- IPC ------------------------------------------------------------------

/**
 * Parses a config and reports what could be imported. Writes nothing, so the
 * user can look at the list before deciding.
 */
export function previewOpenSshConfig(content: string): Promise<ImportPreview> {
	return invoke('preview_openssh_config', { request: { content } });
}

/**
 * Imports the entries the user selected and reports what happened to each one.
 * The config is parsed again on the way in, so what is imported is what the
 * preview would show for this text — never what some earlier answer said.
 */
export function importOpenSshHosts(
	content: string,
	selectedEntryIndexes: number[]
): Promise<ImportSummary> {
	return invoke('import_openssh_hosts', { request: { content, selectedEntryIndexes } });
}

// --- what the dialog decides ----------------------------------------------

/** The size of a config in bytes, the way the backend counts it. */
export function configBytes(content: string): number {
	return new TextEncoder().encode(content).length;
}

/**
 * Why a config cannot be sent at all, or null when it can.
 *
 * The size is passed in when it is already known — a chosen file knows its own
 * byte length — so a large file is never encoded twice.
 */
export function configProblem(content: string, size: number = configBytes(content)): string | null {
	if (content.trim() === '') {
		return 'Paste a config or choose a file first.';
	}
	if (size > MAX_CONFIG_BYTES) {
		return `The config is larger than ${MAX_CONFIG_BYTES} bytes.`;
	}
	return null;
}

/**
 * Whether an entry can be imported. Only a `ready` entry can: everything else
 * is already saved, uses syntax this build does not read, or is incomplete.
 */
export function selectable(entry: ImportEntry): boolean {
	return entry.status === 'ready';
}

/**
 * The entries ticked when a preview arrives: the ones that can be imported.
 * A duplicate, an unsupported block and an incomplete one all start unticked.
 */
export function defaultSelection(preview: ImportPreview): number[] {
	return preview.entries.filter(selectable).map((entry) => entry.index);
}

/**
 * Ticks or unticks one entry. An entry that cannot be imported cannot be picked
 * either, so a stray click changes nothing.
 */
export function toggleSelection(
	selection: number[],
	index: number,
	preview: ImportPreview
): number[] {
	const entry = preview.entries.find((candidate) => candidate.index === index);
	if (!entry || !selectable(entry)) {
		return selection;
	}
	if (selection.includes(index)) {
		return selection.filter((selected) => selected !== index);
	}
	return [...selection, index].sort((left, right) => left - right);
}

/** How an entry's status reads on screen. */
export function entryStatusLabel(status: EntryStatus): string {
	switch (status) {
		case 'ready':
			return 'Ready';
		case 'already_exists':
			return 'Already saved';
		case 'unsupported':
			return 'Unsupported';
		case 'invalid':
			return 'Incomplete';
	}
}

/** How an outcome reads on screen. */
export function outcomeLabel(outcome: ImportOutcome): string {
	switch (outcome) {
		case 'imported':
			return 'Imported';
		case 'skipped':
			return 'Skipped';
		case 'failed':
			return 'Failed';
	}
}

/** A note, with the line it is about when it is about one. */
export function warningLabel(warning: ConfigWarning): string {
	return warning.line === null ? warning.message : `Line ${warning.line}: ${warning.message}`;
}

/**
 * What the confirmation step says before anything is written. It is the whole
 * promise the import makes, so the dialog and its test read it from here.
 */
export function confirmationLines(count: number): string[] {
	return [
		`${count} ${count === 1 ? 'host' : 'hosts'} will be added to this vault.`,
		'They will have no Credential yet, so their Connect button stays disabled until you edit each one and choose a Credential.',
		'Directives Termust does not read are not imported, and nothing on your existing hosts is changed.'
	];
}

/** The one-line summary the result step shows. */
export function resultLine(summary: ImportSummary): string {
	return `${summary.imported} imported, ${summary.skipped} skipped, ${summary.failed} failed.`;
}

/**
 * The entries whose outcome the user should read: a skipped or failed entry
 * always has a reason, and a failure is never hidden behind a count.
 */
export function problemResults(summary: ImportSummary): ImportResultEntry[] {
	return summary.results.filter((result) => result.outcome !== 'imported');
}

/** Every note a preview carries, file-level ones first. */
export function previewWarnings(preview: ImportPreview): ConfigWarning[] {
	return [...preview.warnings, ...preview.entries.flatMap((entry) => entry.warnings)];
}
