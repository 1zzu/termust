import { describe, expect, it, vi } from 'vitest';

// Only the IPC boundary is mocked: this module talks to `ssh.ts`, which imports
// the Tauri client at load time.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import {
	QUICK_CONNECT_TITLE,
	addSession,
	applyStatus,
	beginClosing,
	cancelClosing,
	clearHostKeyPrompt,
	createSession,
	findSession,
	firstPromptSessionId,
	hasLiveSessions,
	inputToSend,
	isLiveStatus,
	liveSessionIds,
	nextActiveId,
	quickConnectTitle,
	resizeToSend,
	routeEvent,
	savedHostConnecting,
	sessionAcceptsInput,
	sessionCanDisconnect,
	setDisconnecting,
	setHostKeyPrompt,
	setSessionError,
	snippetTarget,
	snippetTargetReady,
	tabKeyboardTarget,
	writeOutput,
	type TerminalSession
} from './session-tabs';
import type { HostKeyEvent } from './types';

/** A tab as the strip holds it, for tests that do not create one for real. */
function session(id: string, overrides: Partial<TerminalSession> = {}): TerminalSession {
	return {
		id,
		title: id,
		source: 'quick',
		status: 'connecting',
		statusMessage: '',
		errorMessage: '',
		hostKeyPrompt: null,
		disconnecting: false,
		closing: false,
		...overrides
	};
}

function savedSession(id: string, hostId: string, overrides: Partial<TerminalSession> = {}) {
	return session(id, { source: 'saved', savedHostId: hostId, ...overrides });
}

function prompt(sessionId: string): HostKeyEvent {
	return {
		sessionId,
		host: 'example.com',
		port: 22,
		algorithm: 'ssh-ed25519',
		fingerprint: 'SHA256:abc',
		persistent: true
	};
}

/** Base64 of `text`, as the backend sends terminal output. */
function base64(text: string): string {
	return btoa(text);
}

describe('createSession', () => {
	it('gives every tab a session id of its own', () => {
		const first = createSession({ title: 'one', source: 'quick' });
		const second = createSession({ title: 'two', source: 'quick' });

		expect(first.id).not.toBe(second.id);
		expect(first.id).toMatch(/^[0-9a-f-]{36}$/);
		expect(first.status).toBe('connecting');
		expect(first.hostKeyPrompt).toBeNull();
		expect(first.closing).toBe(false);
	});

	it('keeps the saved host a tab came from, and nothing secret', () => {
		const quick = createSession({ title: 'Quick Connect', source: 'quick' });
		const saved = createSession({ title: 'Prod web', source: 'saved', savedHostId: 'host-1' });

		expect(quick.savedHostId).toBeUndefined();
		expect(saved.savedHostId).toBe('host-1');
		expect(saved.source).toBe('saved');

		// A descriptor is not a place for a credential: a tab must not be able to
		// keep a secret alive after the request that used it.
		for (const descriptor of [quick, saved]) {
			const text = JSON.stringify(descriptor).toLowerCase();
			for (const forbidden of ['password', 'privatekey', 'private_key', 'passphrase', 'secret']) {
				expect(text).not.toContain(forbidden);
			}
		}
	});
});

describe('quickConnectTitle', () => {
	it('names the target with what the user typed, trimmed', () => {
		expect(quickConnectTitle(' root ', ' example.com ')).toBe('root@example.com');
	});

	it('falls back to the form name when there is no username to show', () => {
		expect(quickConnectTitle('', 'example.com')).toBe(QUICK_CONNECT_TITLE);
		expect(quickConnectTitle('   ', 'example.com')).toBe(QUICK_CONNECT_TITLE);
		expect(quickConnectTitle('root', '')).toBe(QUICK_CONNECT_TITLE);
	});

	it('never puts a port or a secret in the title', () => {
		const title = quickConnectTitle('root', 'example.com');

		expect(title).not.toContain(':22');
		expect(title.toLowerCase()).not.toContain('password');
	});
});

describe('the strip', () => {
	it('holds every tab it is given, in the order they were opened', () => {
		let sessions = addSession([], session('a'));
		sessions = addSession(sessions, session('b'));

		// Moving the active tab is only an id change: no tab leaves the list.
		expect(findSession(sessions, 'b')?.id).toBe('b');
		expect(sessions.map((tab) => tab.id)).toEqual(['a', 'b']);
	});

	it('lists live sessions across the whole strip, not just the active tab', () => {
		const sessions = [
			session('a', { status: 'connected' }),
			session('b', { status: 'connecting' }),
			session('c', { status: 'disconnected' })
		];

		expect(liveSessionIds(sessions)).toEqual(['a', 'b']);
		expect(hasLiveSessions(sessions)).toBe(true);
	});

	it('counts a session that is closing as still live, and a finished one as gone', () => {
		expect(hasLiveSessions([session('a', { status: 'error' })])).toBe(false);
		expect(hasLiveSessions([session('a', { status: 'disconnected', closing: true })])).toBe(true);
		expect(hasLiveSessions([])).toBe(false);
	});

	it('treats connecting, authenticating and connected as live states', () => {
		expect(isLiveStatus('connecting')).toBe(true);
		expect(isLiveStatus('verifying_host_key')).toBe(true);
		expect(isLiveStatus('authenticating')).toBe(true);
		expect(isLiveStatus('connected')).toBe(true);
		expect(isLiveStatus('idle')).toBe(false);
		expect(isLiveStatus('disconnected')).toBe(false);
		expect(isLiveStatus('error')).toBe(false);
	});

	it('finds no session for a null or unknown active id', () => {
		expect(findSession([session('a')], null)).toBeUndefined();
		expect(findSession([session('a')], 'missing')).toBeUndefined();
		expect(findSession([session('a')], 'a')?.id).toBe('a');
	});
});

describe('closing tabs', () => {
	const three = () => [session('a'), session('b'), session('c')];

	it('selects the tab on the right at the same position', () => {
		expect(nextActiveId(three(), 'b')).toBe('c');
		expect(nextActiveId(three(), 'a')).toBe('b');
	});

	it('falls back to the tab on the left when the closed one was last', () => {
		expect(nextActiveId(three(), 'c')).toBe('b');
	});

	it('leaves no active tab when the last tab goes', () => {
		expect(nextActiveId([session('a')], 'a')).toBeNull();
		expect(nextActiveId(three(), 'missing')).toBeNull();
	});

	it('marks a tab as closing so a second close click is ignored', () => {
		const closing = beginClosing(three(), 'b');

		expect(findSession(closing, 'b')?.closing).toBe(true);
		expect(findSession(closing, 'a')?.closing).toBe(false);
		// Closing a tab does not take it away: the page removes it once the
		// backend session has stopped.
		expect(closing).toHaveLength(3);
		expect(beginClosing(closing, 'b')).toHaveLength(3);
	});
});

describe('the difference between disconnecting and closing', () => {
	it('counts a session that is being stopped as live, all the way through', () => {
		const stopping = setDisconnecting([session('a')], 'a', true);

		// The backend may still have the task, so a restore has to stay out.
		expect(hasLiveSessions(stopping)).toBe(true);
		expect(findSession(stopping, 'a')?.disconnecting).toBe(true);
		expect(findSession(stopping, 'a')?.closing).toBe(false);
	});

	it('takes input and resize away while the connection is being stopped', () => {
		const stopping = findSession(setDisconnecting([session('a')], 'a', true), 'a');
		expect(stopping).toBeDefined();
		if (!stopping) return;

		expect(sessionAcceptsInput(stopping)).toBe(false);
		expect(inputToSend(stopping, 'ls\n')).toBeNull();
		expect(resizeToSend(stopping, 120, 40)).toBeNull();
	});

	it('offers Disconnect only for a live session that is not already stopping', () => {
		expect(sessionCanDisconnect(session('a'))).toBe(true);
		expect(sessionCanDisconnect(session('a', { status: 'connecting' }))).toBe(true);
		expect(sessionCanDisconnect(session('a', { disconnecting: true }))).toBe(false);
		expect(sessionCanDisconnect(session('a', { closing: true }))).toBe(false);
		expect(sessionCanDisconnect(session('a', { status: 'disconnected' }))).toBe(false);
		expect(sessionCanDisconnect(session('a', { status: 'error' }))).toBe(false);
	});

	it('takes the closing mark back when a close could not stop the session', () => {
		const closing = beginClosing([session('a')], 'a');
		const kept = cancelClosing(closing, 'a');

		expect(findSession(closing, 'a')?.closing).toBe(true);
		expect(findSession(kept, 'a')?.closing).toBe(false);
		// The tab is still there, with the session it belongs to.
		expect(kept).toHaveLength(1);
	});
});

describe('status and errors', () => {
	it('changes only the tab the status belongs to', () => {
		const sessions = [session('a', { status: 'connected' }), session('b')];
		const next = applyStatus(sessions, 'a', 'disconnected', 'The session ended.');

		expect(findSession(next, 'a')?.status).toBe('disconnected');
		expect(findSession(next, 'a')?.statusMessage).toBe('The session ended.');
		expect(findSession(next, 'b')?.status).toBe('connecting');
		expect(findSession(next, 'b')?.statusMessage).toBe('');
		// The original list is not modified in place.
		expect(sessions[0].status).toBe('connected');
	});

	it('keeps a disconnected tab in the strip, with its status', () => {
		const sessions = [session('a', { status: 'connected' })];
		const next = applyStatus(sessions, 'a', 'disconnected', '');

		expect(next).toHaveLength(1);
		expect(next[0].status).toBe('disconnected');
	});

	it('stores an error on its own tab and leaves the other one alone', () => {
		const sessions = [session('a'), session('b', { status: 'connected' })];
		const next = setSessionError(sessions, 'a', 'Authentication failed.');

		expect(findSession(next, 'a')?.errorMessage).toBe('Authentication failed.');
		expect(findSession(next, 'b')?.errorMessage).toBe('');
	});

	it('ignores a status for a tab that is gone', () => {
		const sessions = [session('a')];
		const next = applyStatus(sessions, 'closed', 'connected', '');

		expect(next).toEqual(sessions);
	});
});

describe('host-key prompts', () => {
	it('keeps each prompt on its own tab', () => {
		const sessions = [session('a'), session('b')];
		const next = setHostKeyPrompt(sessions, 'a', prompt('a'));

		expect(findSession(next, 'a')?.hostKeyPrompt?.sessionId).toBe('a');
		expect(findSession(next, 'b')?.hostKeyPrompt).toBeNull();
	});

	it('never loses a prompt that has not been answered', () => {
		let sessions = setHostKeyPrompt([session('a'), session('b')], 'a', prompt('a'));
		sessions = setHostKeyPrompt(sessions, 'b', prompt('b'));

		// Answering the first one leaves the second one waiting.
		sessions = clearHostKeyPrompt(sessions, 'a');
		expect(firstPromptSessionId(sessions)).toBe('b');
		expect(sessions).toHaveLength(2);

		sessions = clearHostKeyPrompt(sessions, 'b');
		expect(firstPromptSessionId(sessions)).toBeNull();
	});

	it('forgets a pending prompt once the session is over', () => {
		const pending = setHostKeyPrompt([session('a')], 'a', prompt('a'));
		const ended = applyStatus(pending, 'a', 'error', '');

		expect(findSession(ended, 'a')?.hostKeyPrompt).toBeNull();
	});
});

describe('input and resize', () => {
	it('sends no input from a terminal that is not connected', () => {
		expect(inputToSend(session('a', { status: 'disconnected' }), 'x')).toBeNull();
		expect(inputToSend(session('a', { status: 'connecting' }), 'x')).toBeNull();
		expect(inputToSend(session('a', { status: 'error' }), 'x')).toBeNull();
		expect(inputToSend(session('a', { status: 'connected' }), '')).toBeNull();
	});

	it('sends input to the session it came from, and only that one', () => {
		const a = session('a', { status: 'connected' });
		const b = session('b', { status: 'connected' });

		expect(inputToSend(a, 'ls\n')).toEqual({ sessionId: 'a', data: 'ls\n' });
		expect(inputToSend(b, 'ls\n')).toEqual({ sessionId: 'b', data: 'ls\n' });
	});

	it('sends no input or resize from a tab that is closing', () => {
		const closing = session('a', { status: 'connected', closing: true });

		expect(sessionAcceptsInput(closing)).toBe(false);
		expect(inputToSend(closing, 'x')).toBeNull();
		expect(inputToSend(session('a', { status: 'connected' }), 'x')).not.toBeNull();
	});
});

describe('keyboard navigation on the strip', () => {
	it('moves one tab at a time and stays inside the strip', () => {
		expect(tabKeyboardTarget('ArrowRight', 0, 3)).toBe(1);
		expect(tabKeyboardTarget('ArrowLeft', 1, 3)).toBe(0);
		expect(tabKeyboardTarget('ArrowLeft', 0, 3)).toBe(0);
		expect(tabKeyboardTarget('ArrowRight', 2, 3)).toBe(2);
	});

	it('jumps to the first and last tab', () => {
		expect(tabKeyboardTarget('Home', 2, 3)).toBe(0);
		expect(tabKeyboardTarget('End', 0, 3)).toBe(2);
	});

	it('ignores anything else, including an empty strip', () => {
		expect(tabKeyboardTarget('Enter', 0, 3)).toBeNull();
		expect(tabKeyboardTarget('ArrowRight', 0, 0)).toBeNull();
		expect(tabKeyboardTarget('ArrowRight', 3, 3)).toBeNull();
	});
});

describe('routing events by session id', () => {
	it('finds the session the event names', () => {
		const sessions = [session('a'), session('b')];

		expect(routeEvent(sessions, { sessionId: 'b' })?.id).toBe('b');
		expect(routeEvent(sessions, { sessionId: 'a' })?.id).toBe('a');
	});

	it('drops an event that names no tab at all, including a closed one', () => {
		const sessions = [session('a')];

		expect(routeEvent(sessions, { sessionId: 'closed' })).toBeUndefined();
		expect(routeEvent(sessions, {})).toBeUndefined();
		expect(routeEvent(sessions, { sessionId: 7 })).toBeUndefined();
	});

	it('writes output to the terminal of the session it belongs to', () => {
		const sessions = [session('a', { status: 'connected' }), session('b', { status: 'connected' })];
		const written = new Map<string, string>();
		const write = (sessionId: string, bytes: Uint8Array) => {
			written.set(sessionId, new TextDecoder().decode(bytes));
		};

		expect(writeOutput(sessions, { sessionId: 'a', data: base64('from-a') }, write)).toBe(true);
		expect(written.get('a')).toBe('from-a');
		// Output for one session never reaches the other terminal.
		expect(written.has('b')).toBe(false);

		expect(writeOutput(sessions, { sessionId: 'b', data: base64('from-b') }, write)).toBe(true);
		expect(written.get('b')).toBe('from-b');
		expect(written.get('a')).toBe('from-a');
	});

	it('ignores output for a tab that was closed, even after the fact', () => {
		const sessions = [session('a')];
		const write = vi.fn();

		expect(writeOutput(sessions, { sessionId: 'gone', data: base64('late') }, write)).toBe(false);
		expect(write).not.toHaveBeenCalled();
	});
});

describe('saved host connections', () => {
	it('refuses a second connection for a host whose tab is still being opened', () => {
		const sessions = [savedSession('a', 'host-1', { status: 'connecting' })];

		expect(savedHostConnecting(sessions, 'host-1')).toBe(true);
		expect(savedHostConnecting(sessions, 'host-2')).toBe(false);
	});

	it('allows another connection once the first request has been accepted', () => {
		const accepted = [savedSession('a', 'host-1', { status: 'verifying_host_key' })];
		const connected = [savedSession('a', 'host-1', { status: 'connected' })];

		expect(savedHostConnecting(accepted, 'host-1')).toBe(false);
		expect(savedHostConnecting(connected, 'host-1')).toBe(false);
	});

	it('does not count a closing tab, and ignores Quick Connect tabs', () => {
		const closing = [savedSession('a', 'host-1', { status: 'connecting', closing: true })];
		const quick = [session('a', { status: 'connecting' })];

		expect(savedHostConnecting(closing, 'host-1')).toBe(false);
		expect(savedHostConnecting(quick, 'host-1')).toBe(false);
	});
});

describe('snippets bound to one tab', () => {
	it('binds the preview to the session that was active when it opened', () => {
		const target = snippetTarget('a', 'root@example.com');

		expect(target).toEqual({ sessionId: 'a', title: 'root@example.com' });
		expect(snippetTargetReady(target, 'a', 'connected', false)).toBe(true);
	});

	it('cannot send anything after the user switches to another tab', () => {
		const target = snippetTarget('a', 'root@example.com');

		// The active tab is now b: the confirmation is refused instead of being
		// quietly aimed at another session.
		expect(snippetTargetReady(target, 'b', 'connected', false)).toBe(false);
		expect(snippetTargetReady(target, 'a', 'disconnected', false)).toBe(false);
		expect(snippetTargetReady(target, 'a', 'connected', true)).toBe(false);
		expect(snippetTargetReady(null, 'a', 'connected', false)).toBe(false);
	});

	it('has no target without a session', () => {
		expect(snippetTarget(undefined, 'x')).toBeNull();
		expect(snippetTarget('', 'x')).toBeNull();
	});
});
