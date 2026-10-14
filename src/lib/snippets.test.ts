import { beforeEach, describe, expect, it, vi } from 'vitest';

// The IPC boundary is the only thing mocked here: these tests pin the command
// names and the argument names the backend has to answer to.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';

import {
	MAX_SNIPPET_CONTENT_BYTES,
	SNIPPET_SECRET_NOTE,
	createSnippet,
	deleteSnippet,
	emptySnippetForm,
	listSnippets,
	runEnabled,
	runSnippet,
	runSnippetRequest,
	snippetByteLength,
	snippetFormFor,
	snippetFormProblem,
	snippetInput,
	snippetPreview,
	snippetsVisible,
	updateSnippet,
	type Snippet
} from './snippets';
import type { ConnectionStatus } from './types';

const CONTENT_MARKER = 'marker-snippet-content-3d7a';

function snippet(overrides: Partial<Snippet> = {}): Snippet {
	return {
		id: 'snippet-1',
		name: 'Deploy',
		content: 'git pull',
		createdAt: 1,
		updatedAt: 2,
		...overrides
	};
}

describe('snippet IPC', () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
	});

	it('lists snippets with no arguments', async () => {
		vi.mocked(invoke).mockResolvedValue([]);

		await listSnippets();

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(vi.mocked(invoke).mock.calls[0][0]).toBe('list_snippets');
		expect(vi.mocked(invoke).mock.calls[0][1]).toBeUndefined();
	});

	it('creates a snippet from its name and content', async () => {
		vi.mocked(invoke).mockResolvedValue(snippet());

		await createSnippet({ name: 'Deploy', content: 'git pull' });

		expect(invoke).toHaveBeenCalledWith('create_snippet', {
			snippet: { name: 'Deploy', content: 'git pull' }
		});
	});

	it('updates a snippet by id', async () => {
		vi.mocked(invoke).mockResolvedValue(snippet());

		await updateSnippet('snippet-1', { name: 'Deploy', content: 'make' });

		expect(invoke).toHaveBeenCalledWith('update_snippet', {
			id: 'snippet-1',
			snippet: { name: 'Deploy', content: 'make' }
		});
	});

	it('deletes a snippet by id', async () => {
		vi.mocked(invoke).mockResolvedValue(null);

		await deleteSnippet('snippet-1');

		expect(invoke).toHaveBeenCalledWith('delete_snippet', { id: 'snippet-1' });
	});
});

describe('runSnippetRequest', () => {
	it('carries the session id and the snippet id, and nothing else', () => {
		const request = runSnippetRequest('session-1', 'snippet-1');

		expect(request).toEqual({ sessionId: 'session-1', snippetId: 'snippet-1' });
		expect(Object.keys(request).sort()).toEqual(['sessionId', 'snippetId']);

		// The content is deliberately not part of this payload: the backend reads
		// it from the vault, so the frontend cannot decide what a session receives.
		const text = JSON.stringify(request).toLowerCase();
		for (const forbidden of ['content', 'command', 'data', 'password', 'name']) {
			expect(text).not.toContain(forbidden);
		}
	});

	it('is what run_snippet is called with', async () => {
		vi.mocked(invoke).mockReset();
		vi.mocked(invoke).mockResolvedValue(null);

		await runSnippet(runSnippetRequest('session-1', 'snippet-1'));

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(vi.mocked(invoke).mock.calls[0][0]).toBe('run_snippet');
		expect(vi.mocked(invoke).mock.calls[0][1]).toEqual({
			request: { sessionId: 'session-1', snippetId: 'snippet-1' }
		});
	});

	it('keeps a rejected run a failure of its own', async () => {
		vi.mocked(invoke).mockReset();
		vi.mocked(invoke).mockRejectedValue('This session is not connected.');

		await expect(runSnippet(runSnippetRequest('session-1', 'snippet-1'))).rejects.toBe(
			'This session is not connected.'
		);
	});

	it('is never reached by listing a snippet or by looking at one', async () => {
		vi.mocked(invoke).mockReset();
		vi.mocked(invoke).mockResolvedValue([snippet({ content: CONTENT_MARKER })]);

		// Everything the UI does before the confirmation button: nothing here is
		// allowed to start a run.
		const listed = await listSnippets();
		snippetPreview(listed[0].content);
		snippetFormFor(listed[0]);
		runSnippetRequest('session-1', listed[0].id);

		expect(vi.mocked(invoke).mock.calls.map((call) => call[0])).toEqual(['list_snippets']);
	});
});

describe('snippetPreview', () => {
	it('shows one line, and the first two when there are more', () => {
		expect(snippetPreview('git pull')).toBe('git pull');
		expect(snippetPreview('cd /srv\ngit pull\nmake')).toBe('cd /srv ↵ git pull');
	});

	it('collapses indentation so a one-liner stays readable', () => {
		expect(snippetPreview('    echo    hello   ')).toBe('echo hello');
	});

	it('truncates a long command', () => {
		const preview = snippetPreview('a'.repeat(200));

		expect(preview.length).toBeLessThanOrEqual(81);
		expect(preview.endsWith('…')).toBe(true);
	});

	it('returns text unchanged, so markup is shown as characters', () => {
		// The component interpolates this into a `<pre>`; nothing here turns it
		// into HTML, and the UI never uses `{@html}`.
		expect(snippetPreview('<script>alert(1)</script>')).toBe('<script>alert(1)</script>');
		expect(snippetPreview('echo "a & b | c"')).toBe('echo "a & b | c"');
	});
});

describe('snippetByteLength', () => {
	it('measures bytes rather than characters, the way the backend does', () => {
		expect(snippetByteLength('abc')).toBe(3);
		expect(snippetByteLength('é')).toBe(2);
		expect(snippetByteLength('😀')).toBe(4);
	});
});

describe('snippetFormProblem', () => {
	it('requires a name and a command', () => {
		expect(snippetFormProblem(emptySnippetForm())).toBe('Enter a name for this snippet.');
		expect(snippetFormProblem({ name: 'Deploy', content: '' })).toBe(
			'Enter the command this snippet should run.'
		);
		expect(snippetFormProblem({ name: ' Deploy ', content: 'git pull' })).toBeNull();
	});

	it('refuses content over the byte limit, and accepts exactly the limit', () => {
		expect(
			snippetFormProblem({ name: 'Deploy', content: 'a'.repeat(MAX_SNIPPET_CONTENT_BYTES) })
		).toBeNull();
		expect(
			snippetFormProblem({ name: 'Deploy', content: 'a'.repeat(MAX_SNIPPET_CONTENT_BYTES + 1) })
		).toContain(String(MAX_SNIPPET_CONTENT_BYTES));

		// A blank line is still content: the command may legitimately be one.
		expect(snippetFormProblem({ name: 'Deploy', content: '\n' })).toBeNull();
	});

	it('never repeats the command in a validation message', () => {
		const problem = snippetFormProblem({ name: '', content: CONTENT_MARKER });

		expect(problem).not.toBeNull();
		expect(problem).not.toContain(CONTENT_MARKER);
	});
});

describe('snippetInput', () => {
	it('trims the name and passes the command through untouched', () => {
		const content = '  echo "  spaced  "\n\tmake\n';

		expect(snippetInput({ name: '  Deploy  ', content })).toEqual({
			name: 'Deploy',
			content
		});
	});
});

describe('snippetFormFor', () => {
	it('loads the stored snippet into the edit form unchanged', () => {
		const form = snippetFormFor(snippet({ name: 'Deploy', content: 'git pull\nmake' }));

		expect(form).toEqual({ name: 'Deploy', content: 'git pull\nmake' });
	});
});

describe('snippetsVisible', () => {
	it('shows snippets only while the vault is open', () => {
		expect(snippetsVisible('unlocked')).toBe(true);
		expect(snippetsVisible('locked')).toBe(false);
		expect(snippetsVisible('uninitialized')).toBe(false);
		expect(snippetsVisible(undefined)).toBe(false);
	});
});

describe('runEnabled', () => {
	it('is off until a session is really connected', () => {
		const statuses: ConnectionStatus[] = [
			'idle',
			'connecting',
			'verifying_host_key',
			'authenticating',
			'disconnected',
			'error'
		];

		for (const status of statuses) {
			expect(runEnabled(status, 'session-1', false)).toBe(false);
		}
		expect(runEnabled('connected', 'session-1', false)).toBe(true);
	});

	it('is off without a session, even while connected', () => {
		expect(runEnabled('connected', undefined, false)).toBe(false);
		expect(runEnabled('connected', '', false)).toBe(false);
	});

	it('is off while a run is already in flight, so a second click cannot start one', () => {
		expect(runEnabled('connected', 'session-1', true)).toBe(false);
	});

	it('turns off by itself when the session ends with the dialog open', () => {
		// The dialog stays open and the button goes dead instead of sending the
		// snippet to a session that is gone.
		expect(runEnabled('connected', 'session-1', false)).toBe(true);
		expect(runEnabled('disconnected', 'session-1', false)).toBe(false);
		expect(runEnabled('error', undefined, false)).toBe(false);
	});
});

describe('SNIPPET_SECRET_NOTE', () => {
	it('says that a snippet is not a place for a secret', () => {
		for (const word of ['passwords', 'tokens', 'private keys', 'Credentials']) {
			expect(SNIPPET_SECRET_NOTE).toContain(word);
		}
	});
});
