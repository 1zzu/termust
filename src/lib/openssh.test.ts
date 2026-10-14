import { beforeEach, describe, expect, it, vi } from 'vitest';

// The IPC boundary is the only thing mocked here: these tests pin the command
// names and the argument names the backend has to answer to.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';

import {
	MAX_CONFIG_BYTES,
	configBytes,
	configProblem,
	confirmationLines,
	defaultSelection,
	entryStatusLabel,
	importOpenSshHosts,
	outcomeLabel,
	previewOpenSshConfig,
	previewWarnings,
	problemResults,
	resultLine,
	selectable,
	toggleSelection,
	warningLabel,
	type ImportEntry,
	type ImportPreview,
	type ImportSummary
} from './openssh';

// Distinctive values, so a payload can be checked for what it must not carry.
const ALIAS_MARKER = '<img src=x onerror=alert(1)>';
const ADDRESS_MARKER = 'marker-address.example.com';

function entry(overrides: Partial<ImportEntry> = {}): ImportEntry {
	return {
		index: 0,
		label: 'production',
		address: ADDRESS_MARKER,
		port: 22,
		username: 'deploy',
		status: 'ready',
		warnings: [],
		...overrides
	};
}

function preview(overrides: Partial<ImportPreview> = {}): ImportPreview {
	return { entries: [], warnings: [], ...overrides };
}

function summary(overrides: Partial<ImportSummary> = {}): ImportSummary {
	return { imported: 0, skipped: 0, failed: 0, results: [], ...overrides };
}

describe('configProblem', () => {
	it('refuses an empty config before anything is sent', () => {
		expect(configProblem('')).toBe('Paste a config or choose a file first.');
		expect(configProblem('   \n\t\n')).toBe('Paste a config or choose a file first.');
	});

	it('accepts a config that has any text at all', () => {
		expect(configProblem('Host prod\n  User deploy\n')).toBeNull();
	});

	it('refuses a config over the limit, and accepts one at the limit', () => {
		const atLimit = 'a'.repeat(MAX_CONFIG_BYTES);
		const overLimit = 'a'.repeat(MAX_CONFIG_BYTES + 1);

		expect(configProblem(atLimit)).toBeNull();
		expect(configProblem(overLimit)).toBe(`The config is larger than ${MAX_CONFIG_BYTES} bytes.`);
	});

	it('counts bytes rather than characters, so a multibyte paste is measured honestly', () => {
		// Two bytes per character here: a paste this long is already over the
		// limit while it is still short of that many characters.
		const multibyte = 'é'.repeat(600_000);

		expect(multibyte.length).toBeLessThan(MAX_CONFIG_BYTES);
		expect(configBytes(multibyte)).toBeGreaterThan(MAX_CONFIG_BYTES);
		expect(configProblem(multibyte)).toBe(`The config is larger than ${MAX_CONFIG_BYTES} bytes.`);
	});

	it('uses a size that is already known, so a file is not encoded twice', () => {
		const content = 'Host prod\n';
		const reported = MAX_CONFIG_BYTES + 1;

		expect(configProblem(content, reported)).toBe(
			`The config is larger than ${MAX_CONFIG_BYTES} bytes.`
		);
		expect(configProblem(content, MAX_CONFIG_BYTES)).toBeNull();
	});
});

describe('selection', () => {
	it('ticks only the entries that can be imported', () => {
		const listed = preview({
			entries: [
				entry({ index: 0 }),
				entry({ index: 1, status: 'already_exists' }),
				entry({ index: 2, status: 'unsupported' }),
				entry({ index: 3, status: 'invalid' }),
				entry({ index: 4 })
			]
		});

		expect(defaultSelection(listed)).toEqual([0, 4]);
		expect(listed.entries.filter(selectable).map((candidate) => candidate.status)).toEqual([
			'ready',
			'ready'
		]);
	});

	it('starts with nothing selected when nothing can be imported', () => {
		expect(defaultSelection(preview({ entries: [entry({ status: 'invalid' })] }))).toEqual([]);
	});

	it('ticks and unticks one entry, and keeps the indexes in order', () => {
		const listed = preview({
			entries: [entry({ index: 0 }), entry({ index: 1 }), entry({ index: 2 })]
		});

		expect(toggleSelection([], 2, listed)).toEqual([2]);
		expect(toggleSelection([2], 0, listed)).toEqual([0, 2]);
		expect(toggleSelection([0, 2], 2, listed)).toEqual([0]);
	});

	it('does not let an entry that cannot be imported be picked', () => {
		const listed = preview({
			entries: [
				entry({ index: 0, status: 'already_exists' }),
				entry({ index: 1, status: 'unsupported' })
			]
		});

		expect(toggleSelection([], 0, listed)).toEqual([]);
		expect(toggleSelection([], 1, listed)).toEqual([]);
		// An index the preview does not have changes nothing either.
		expect(toggleSelection([], 9, listed)).toEqual([]);
	});

	it('leaves the selection it was given alone', () => {
		const listed = preview({ entries: [entry({ index: 0 }), entry({ index: 1 })] });
		const selection = [0];

		const next = toggleSelection(selection, 1, listed);

		expect(next).toEqual([0, 1]);
		expect(selection).toEqual([0]);
	});
});

describe('what the dialog shows', () => {
	it('names every status and every outcome', () => {
		expect(
			(['ready', 'already_exists', 'unsupported', 'invalid'] as const).map(entryStatusLabel)
		).toEqual(['Ready', 'Already saved', 'Unsupported', 'Incomplete']);
		expect((['imported', 'skipped', 'failed'] as const).map(outcomeLabel)).toEqual([
			'Imported',
			'Skipped',
			'Failed'
		]);
	});

	it('points a note at its line when it has one', () => {
		expect(warningLabel({ line: 4, message: 'Port must be a number.' })).toBe(
			'Line 4: Port must be a number.'
		);
		expect(warningLabel({ line: null, message: 'A saved host already has this address.' })).toBe(
			'A saved host already has this address.'
		);
	});

	it('promises exactly what the import does, before it happens', () => {
		const lines = confirmationLines(2);

		expect(lines[0]).toContain('2 hosts');
		expect(confirmationLines(1)[0]).toContain('1 host');
		expect(confirmationLines(1)[0]).not.toContain('1 hosts');
		expect(lines.join(' ')).toContain('no Credential');
		expect(lines.join(' ')).toContain('Connect button stays disabled');
		expect(lines.join(' ')).toContain('does not read');
	});

	it('reports the three counts and keeps every entry that needs reading', () => {
		const done = summary({
			imported: 2,
			skipped: 1,
			failed: 1,
			results: [
				{ index: 0, label: 'one', outcome: 'imported', hostId: 'host-1' },
				{ index: 1, label: 'two', outcome: 'imported', hostId: 'host-2' },
				{ index: 2, label: 'three', outcome: 'skipped', reason: 'Already saved.' },
				{ index: 3, label: 'four', outcome: 'failed', reason: 'Label is too long.' }
			]
		});

		expect(resultLine(done)).toBe('2 imported, 1 skipped, 1 failed.');
		expect(problemResults(done).map((result) => result.label)).toEqual(['three', 'four']);
		expect(problemResults(summary()).length).toBe(0);
	});

	it('collects the file-level notes before the entries they belong to', () => {
		const listed = preview({
			warnings: [{ line: 1, message: 'outside a Host block' }],
			entries: [
				entry({ index: 0, warnings: [{ line: 3, message: 'skipped' }] }),
				entry({ index: 1 })
			]
		});

		expect(previewWarnings(listed).map(warningLabel)).toEqual([
			'Line 1: outside a Host block',
			'Line 3: skipped'
		]);
	});
});

describe('openssh IPC', () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
	});

	it('previews with the config text and nothing else', async () => {
		vi.mocked(invoke).mockResolvedValue(preview());

		await previewOpenSshConfig('Host prod\n');

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(vi.mocked(invoke).mock.calls[0][0]).toBe('preview_openssh_config');
		expect(vi.mocked(invoke).mock.calls[0][1]).toEqual({ request: { content: 'Host prod\n' } });
	});

	it('imports with the config text and the selected indexes, and nothing else', async () => {
		vi.mocked(invoke).mockResolvedValue(summary());

		await importOpenSshHosts(`Host ${ALIAS_MARKER}\n  User deploy\n`, [2, 0]);

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(vi.mocked(invoke).mock.calls[0][0]).toBe('import_openssh_hosts');

		const payload = vi.mocked(invoke).mock.calls[0][1] as { request: Record<string, unknown> };
		expect(Object.keys(payload).sort()).toEqual(['request']);
		expect(Object.keys(payload.request).sort()).toEqual(['content', 'selectedEntryIndexes']);
		expect(payload.request.selectedEntryIndexes).toEqual([2, 0]);
		// No path, no host body, no credential: an import cannot name any of them.
		expect(JSON.stringify(payload)).not.toContain('credential');
		expect(JSON.stringify(payload)).not.toContain('path');
	});

	it('sends an empty selection as an empty list rather than nothing', async () => {
		vi.mocked(invoke).mockResolvedValue(summary());

		await importOpenSshHosts('Host prod\n', []);

		expect(vi.mocked(invoke).mock.calls[0][1]).toEqual({
			request: { content: 'Host prod\n', selectedEntryIndexes: [] }
		});
	});
});

describe('ImportEntry', () => {
	it('carries no credential and no secret of its own', () => {
		const listed = entry();

		expect(Object.keys(listed).sort()).toEqual([
			'address',
			'index',
			'label',
			'port',
			'status',
			'username',
			'warnings'
		]);
	});

	it('keeps an HTML-like alias as plain text', () => {
		// Nothing in this module builds markup out of a value, so an alias like
		// this one reaches the view as the characters it is made of.
		const status = entryStatusLabel('ready');

		expect(status).not.toContain('<');
		expect(entry({ label: ALIAS_MARKER }).label).toBe(ALIAS_MARKER);
		expect(warningLabel({ line: null, message: ALIAS_MARKER })).toBe(ALIAS_MARKER);
	});
});
