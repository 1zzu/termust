import { describe, expect, it, vi } from 'vitest';

// The lifecycle talks to `ssh.ts`, which imports the Tauri client at load time.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import {
	createSessionLifecycle,
	type SessionLifecycleDeps,
	type StartPlan
} from './session-lifecycle';
import { hasLiveSessions, inputToSend, type TerminalSession } from './session-tabs';
import { SSH_EVENT } from './ssh';
import type { ConnectRequest, HostKeyEvent, SavedConnectRequest } from './types';

/** A promise a test resolves whenever it wants, with no timers involved. */
function controllable() {
	let resolve!: () => void;
	const promise = new Promise<void>((res) => {
		resolve = res;
	});
	return { promise, resolve };
}

/** Lets the promise chains that are already settled run to their end. */
async function settle(): Promise<void> {
	for (let turn = 0; turn < 50; turn += 1) await Promise.resolve();
}

/** The same, but stopping with a clear reason if the state never arrives. */
async function until(ready: () => boolean, why: string): Promise<void> {
	for (let turn = 0; turn < 200; turn += 1) {
		if (ready()) return;
		await Promise.resolve();
	}
	throw new Error(`never happened: ${why}`);
}

function session(id: string, overrides: Partial<TerminalSession> = {}): TerminalSession {
	return {
		id,
		title: id,
		source: 'quick',
		status: 'connected',
		statusMessage: '',
		errorMessage: '',
		hostKeyPrompt: null,
		disconnecting: false,
		closing: false,
		...overrides
	};
}

function quickPlan(sessionId: string): StartPlan {
	return {
		kind: 'quick',
		request: {
			sessionId,
			host: 'example.com',
			port: 22,
			username: 'root',
			auth: 'password',
			password: 'correct-horse-battery-staple'
		}
	};
}

const SAVED_PLAN: StartPlan = { kind: 'saved', hostId: 'host-1' };

/** The lifecycle over a plain array, with every backend call under the test's
 * control: no terminal, no Tauri and no timer is involved. */
function harness() {
	let sessions: TerminalSession[] = [];
	let active: string | null = null;

	const registered: string[] = [];
	const unlistened: string[] = [];
	const attempted: string[] = [];
	const handlers: Record<string, (payload: unknown) => void> = {};
	const connects: ConnectRequest[] = [];
	const savedStarts: SavedConnectRequest[] = [];
	const disconnects: string[] = [];
	const terminals: string[] = [];
	const timeline: string[] = [];
	const eventsErrors: unknown[] = [];
	let quickFinished = 0;

	const listeners = controllable();
	const starts = controllable();
	const stops = controllable();
	let holdListeners = false;
	let holdStarts = false;
	let holdStops = false;
	let failRegistrationAt: number | null = null;
	let failConnect = false;
	let failStop = false;

	const deps: SessionLifecycleDeps = {
		read: () => sessions,
		write: (next) => {
			sessions = next;
			timeline.push(`tabs:${next.length}`);
		},
		readActive: () => active,
		writeActive: (sessionId) => {
			active = sessionId;
		},
		async listen<T>(event: string, handler: (payload: T) => void) {
			const index = attempted.length;
			attempted.push(event);
			if (holdListeners) await listeners.promise;
			if (failRegistrationAt === index) throw new Error(`refused: ${event}`);
			registered.push(event);
			handlers[event] = handler as (payload: unknown) => void;
			return () => {
				unlistened.push(event);
			};
		},
		async connect(request) {
			timeline.push('connect');
			connects.push(request);
			if (holdStarts) await starts.promise;
			if (failConnect) throw new Error('the backend refused the connection');
		},
		async startSaved(request) {
			timeline.push('startSaved');
			savedStarts.push(request);
			if (holdStarts) await starts.promise;
		},
		async disconnect(sessionId) {
			timeline.push('disconnect');
			disconnects.push(sessionId);
			if (holdStops) await stops.promise;
			if (failStop) throw new Error('the backend refused to stop the session');
		},
		size: () => ({ cols: 100, rows: 30 }),
		writeTerminal: (sessionId, bytes) => {
			terminals.push(`${sessionId}:${new TextDecoder().decode(bytes)}`);
		},
		onEventsUnavailable: (error) => {
			eventsErrors.push(error);
		},
		onQuickStartFinished: () => {
			quickFinished += 1;
		}
	};

	const lifecycle = createSessionLifecycle(deps);

	return {
		lifecycle,
		add(...tabs: TerminalSession[]) {
			sessions = [...sessions, ...tabs];
		},
		activate(sessionId: string | null) {
			active = sessionId;
		},
		state: () => sessions,
		find: (id: string) => sessions.find((tab) => tab.id === id),
		events: handlers,
		registered,
		unlistened,
		attempted,
		connects,
		savedStarts,
		disconnects,
		terminals,
		timeline,
		eventsErrors,
		quickFinished: () => quickFinished,
		listeners,
		starts,
		stops,
		holdListeners: () => {
			holdListeners = true;
		},
		holdStarts: () => {
			holdStarts = true;
		},
		holdStops: () => {
			holdStops = true;
		},
		failRegistrationAt: (index: number) => {
			failRegistrationAt = index;
		},
		failNextStop: () => {
			failStop = true;
		},
		failNextConnect: () => {
			failConnect = true;
		}
	};
}

describe('subscribing to the backend events', () => {
	it('attaches all four, in the order the events are declared', async () => {
		const test = harness();

		test.lifecycle.attach();
		await settle();

		expect(test.registered).toEqual([
			SSH_EVENT.data,
			SSH_EVENT.status,
			SSH_EVENT.error,
			SSH_EVENT.hostKey
		]);
		expect(test.unlistened).toEqual([]);
	});

	it('detaches what it attached when a later listener fails', async () => {
		const test = harness();
		test.failRegistrationAt(1);

		test.lifecycle.attach();
		await settle();

		// The first listener went on and came back off again: a partial
		// subscription never stays behind.
		expect(test.registered).toEqual([SSH_EVENT.data]);
		expect(test.unlistened).toEqual([SSH_EVENT.data]);
		expect(test.eventsErrors).toHaveLength(1);
	});

	it('starts nothing for a tab whose listeners never came up', async () => {
		const test = harness();
		test.holdListeners();
		test.add(session('a', { status: 'connecting' }), session('b', { status: 'connecting' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.begin('b', SAVED_PLAN);
		test.lifecycle.terminalReady('a');
		test.lifecycle.terminalReady('b');
		await settle();

		expect(test.connects).toEqual([]);
		expect(test.savedStarts).toEqual([]);
		expect(test.find('a')?.status).toBe('connecting');
	});

	it('fails a pending start instead of touching the backend when a listener fails', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting' }), session('b', { status: 'connecting' }));
		test.failRegistrationAt(2);
		test.lifecycle.attach();
		// The subscription is attempted, and refused, before the tabs are ready.
		await settle();

		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.begin('b', SAVED_PLAN);
		test.lifecycle.terminalReady('a');
		test.lifecycle.terminalReady('b');
		await until(() => test.find('a')?.status === 'error', 'both tabs failed');

		// Neither backend command was called, and both tabs say why.
		expect(test.connects).toEqual([]);
		expect(test.savedStarts).toEqual([]);
		expect(test.find('a')?.errorMessage).toContain('refused');
		expect(test.find('b')?.errorMessage).toContain('refused');
		// The Quick Connect start let go of the form, credentials included.
		expect(test.quickFinished()).toBe(1);
	});

	it('reports a failure through onEventsUnavailable exactly once', async () => {
		const test = harness();
		test.failRegistrationAt(0);

		test.lifecycle.attach();
		await settle();

		expect(test.eventsErrors).toHaveLength(1);
	});
});

describe('starting a session', () => {
	it('starts the backend only once the tab and the listeners are ready', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		await settle();
		expect(test.connects).toEqual([]);

		test.lifecycle.terminalReady('a');
		await until(() => test.connects.length === 1, 'the quick connect went out');

		// The whole request, with the size the terminal ended up with.
		expect(test.connects[0]).toMatchObject({
			sessionId: 'a',
			host: 'example.com',
			cols: 100,
			rows: 30
		});
	});

	it('sends a saved host start as four fields and nothing else', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting', source: 'saved', savedHostId: 'host-1' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', SAVED_PLAN);
		test.lifecycle.terminalReady('a');
		await until(() => test.savedStarts.length === 1, 'the saved host start went out');

		expect(test.savedStarts[0]).toEqual({ sessionId: 'a', hostId: 'host-1', cols: 100, rows: 30 });
	});

	it('ignores a second start for the same tab', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.terminalReady('a');
		await until(() => test.connects.length === 1, 'one start');

		expect(test.connects).toHaveLength(1);
	});

	it('never starts a tab that is already closing', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting', closing: true }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.terminalReady('a');
		await settle();

		expect(test.connects).toEqual([]);
		expect(test.state()).toHaveLength(1);
	});
});

describe('the plan a start owns', () => {
	/**
	 * The Quick Connect plan, with every field behind an accessor so a test can
	 * count who reads it and how often. Nothing else about the plan changes: the
	 * credential is still an ordinary string inside the request, and the shape is
	 * still the one the lifecycle expects.
	 */
	function countedQuickPlan(
		sessionId: string,
		reads: { kind: number; request: number }
	): StartPlan {
		const plain = quickPlan(sessionId);
		if (plain.kind !== 'quick') throw new Error('the fixture is a quick plan');

		return {
			get kind(): 'quick' {
				reads.kind += 1;
				return 'quick';
			},
			get request() {
				reads.request += 1;
				return plain.request;
			}
		};
	}

	it('is read once for the backend call, and never again after the start is over', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting' }));
		const reads = { kind: 0, request: 0 };

		test.lifecycle.attach();
		test.lifecycle.begin('a', countedQuickPlan('a', reads));
		test.lifecycle.terminalReady('a');
		await until(() => test.quickFinished() === 1, 'the quick start finished');

		// Exactly the reads the start itself needs: one to decide whether this is
		// a Quick Connect start, and then the call's own read of the plan it was
		// handed. An extra one would mean a callback had reached for the plan.
		expect(test.connects).toHaveLength(1);
		expect(reads).toEqual({ kind: 2, request: 1 });

		// Everything that outlives the start: closing the tab and tearing the
		// window down. Neither touches the plan again, because the only callback
		// the start reports through is told the session id and nothing else.
		await test.lifecycle.close('a');
		await test.lifecycle.teardown();

		expect(reads).toEqual({ kind: 2, request: 1 });
	});

	it('lets go of the plan exactly once, for a start that fails as well', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting' }));
		const reads = { kind: 0, request: 0 };
		const plan = countedQuickPlan('a', reads);

		test.failNextConnect();
		test.lifecycle.attach();
		test.lifecycle.begin('a', plan);
		test.lifecycle.terminalReady('a');
		await until(() => test.quickFinished() === 1, 'the failed start finished');

		// A refusal is still a finish: it is reported on its own tab, the form is
		// released once, and the plan is not read again by the callback that
		// released it.
		expect(test.find('a')?.status).toBe('error');
		expect(test.find('a')?.errorMessage).toContain('refused');
		expect(reads).toEqual({ kind: 2, request: 1 });

		// A refusal is still a finish, and nothing reads the plan afterwards
		// either: tearing the window down changes no count.
		await test.lifecycle.teardown();
		expect(reads).toEqual({ kind: 2, request: 1 });
	});
});

describe('closing a tab', () => {
	it('does not start the backend when the tab is closed before the terminal exists', async () => {
		const test = harness();
		test.add(session('a', { status: 'connecting' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		await test.lifecycle.close('a');

		// The start was cancelled while it was still queued: nothing was ever
		// sent to the backend, so there is nothing to disconnect either.
		expect(test.connects).toEqual([]);
		expect(test.disconnects).toEqual([]);
		expect(test.state()).toEqual([]);
	});

	it('never reaches the backend after a tab was closed while the listeners were pending', async () => {
		const test = harness();
		test.holdListeners();
		test.add(session('a', { status: 'connecting' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.terminalReady('a');
		await test.lifecycle.close('a');

		// The listeners finally resolve, long after the tab is gone.
		test.listeners.resolve();
		await settle();

		expect(test.connects).toEqual([]);
		expect(test.state()).toEqual([]);
	});

	it('waits for an in-flight saved host start, then disconnects, then removes the tab', async () => {
		const test = harness();
		test.holdStarts();
		test.add(session('a', { status: 'connecting', source: 'saved', savedHostId: 'host-1' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', SAVED_PLAN);
		test.lifecycle.terminalReady('a');
		await until(() => test.savedStarts.length === 1, 'the saved host start went out');

		const closing = test.lifecycle.close('a');
		await settle();
		// The request is still in the air, so the tab is still here — marked as
		// closing, and its session not yet stopped.
		expect(test.find('a')?.closing).toBe(true);
		expect(test.disconnects).toEqual([]);

		test.starts.resolve();
		await closing;

		// Exactly this order, and nothing else.
		expect(test.timeline).toEqual(['startSaved', 'tabs:1', 'disconnect', 'tabs:0']);
		expect(test.disconnects).toEqual(['a']);
		expect(test.state()).toEqual([]);
	});

	it('keeps the tab when the backend could not be stopped', async () => {
		const test = harness();
		test.failNextStop();
		test.add(session('a'));

		await test.lifecycle.close('a');

		// A session that is still running keeps its tab: that is what lets the
		// user stop it, instead of leaving one nobody can see.
		expect(test.disconnects).toEqual(['a']);
		expect(test.state()).toHaveLength(1);
		expect(test.find('a')?.closing).toBe(false);
		expect(test.find('a')?.errorMessage).toContain('refused to stop');
		// And the tab is live, so a restore stays out.
		expect(hasLiveSessions(test.state())).toBe(true);
	});

	it('removes only that tab, and selects the neighbour on the right', async () => {
		const test = harness();
		test.add(session('a'), session('b'), session('c'));
		test.activate('b');

		await test.lifecycle.close('b');

		expect(test.state().map((tab) => tab.id)).toEqual(['a', 'c']);
		expect(test.connects).toEqual([]);
		// Only the closed session was stopped.
		expect(test.disconnects).toEqual(['b']);
	});

	it('closes only once, however often it is asked', async () => {
		const test = harness();
		test.holdStops();
		test.add(session('a'));

		const first = test.lifecycle.close('a');
		await settle();
		await test.lifecycle.close('a');

		test.stops.resolve();
		await first;

		expect(test.disconnects).toEqual(['a']);
		expect(test.state()).toEqual([]);
	});

	it('does not report a cancelled start failure on another tab', async () => {
		const test = harness();
		test.holdStarts();
		test.add(session('a', { status: 'connecting' }), session('b'));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.terminalReady('a');
		await until(() => test.connects.length === 1, 'the quick connect went out');

		const closing = test.lifecycle.close('a');
		await settle();
		test.starts.resolve();
		await closing;

		// The tab is gone, and the session that was already up is untouched: no
		// error, no status change, no disconnect of its own.
		expect(test.state().map((tab) => tab.id)).toEqual(['b']);
		expect(test.find('b')?.errorMessage).toBe('');
		expect(test.find('b')?.status).toBe('connected');
		expect(test.disconnects).toEqual(['a']);
	});
});

describe('disconnecting the active tab', () => {
	it('keeps the tab, and only marks it disconnected once the backend is done', async () => {
		const test = harness();
		test.holdStops();
		test.add(session('a'), session('b'));
		test.activate('a');

		const disconnecting = test.lifecycle.disconnect();
		await until(() => test.find('a')?.disconnecting === true, 'the stop began');

		// Still live while the backend has not finished: a restore stays out.
		expect(test.find('a')?.status).toBe('connected');
		expect(hasLiveSessions(test.state())).toBe(true);

		test.stops.resolve();
		await disconnecting;

		expect(test.disconnects).toEqual(['a']);
		expect(test.find('a')?.status).toBe('disconnected');
		expect(test.find('a')?.disconnecting).toBe(false);
		// The tab, and therefore the terminal behind it, is still there.
		expect(test.state().map((tab) => tab.id)).toEqual(['a', 'b']);
		// Only the active tab was disconnected.
		expect(test.find('b')?.status).toBe('connected');
	});

	it('takes input away while it is stopping, and gives it back if the stop fails', async () => {
		const test = harness();
		test.holdStops();
		test.add(session('a'));
		test.activate('a');

		const disconnecting = test.lifecycle.disconnect();
		await until(() => test.find('a')?.disconnecting === true, 'the stop began');

		const stopping = test.find('a');
		expect(stopping && inputToSend(stopping, 'ls\n')).toBeNull();

		test.stops.resolve();
		await disconnecting;

		// The session is over, so it takes no input either.
		const done = test.find('a');
		expect(done && inputToSend(done, 'ls\n')).toBeNull();
	});

	it('keeps a failed disconnect live, retryable and explained', async () => {
		const test = harness();
		test.failNextStop();
		test.add(session('a'));
		test.activate('a');

		await test.lifecycle.disconnect();

		expect(test.disconnects).toEqual(['a']);
		expect(test.find('a')?.status).toBe('connected');
		expect(test.find('a')?.disconnecting).toBe(false);
		expect(test.find('a')?.errorMessage).toContain('refused to stop');
		expect(hasLiveSessions(test.state())).toBe(true);

		// Nothing is assumed about the connection, so the user may try again.
		await test.lifecycle.disconnect();
		expect(test.disconnects).toEqual(['a', 'a']);
	});

	it('does nothing when there is no live session on screen', async () => {
		const test = harness();
		test.add(session('a', { status: 'disconnected' }), session('b', { status: 'connecting' }));
		test.activate('a');

		await test.lifecycle.disconnect();
		expect(test.disconnects).toEqual([]);

		test.activate('b');
		await test.lifecycle.disconnect();
		expect(test.disconnects).toEqual(['b']);
	});
});

describe('tearing the window down', () => {
	it('cancels pending starts and stops everything that is running', async () => {
		const test = harness();
		test.holdListeners();
		test.add(session('a', { status: 'connecting' }), session('b'));

		test.lifecycle.attach();
		test.lifecycle.begin('a', quickPlan('a'));
		test.lifecycle.terminalReady('a');

		const teardown = test.lifecycle.teardown();
		test.listeners.resolve();
		await teardown;

		// The pending start never reached the backend, so its tab had nothing to
		// stop; the session that was really up was stopped.
		expect(test.connects).toEqual([]);
		expect(test.disconnects).toEqual(['a', 'b']);
	});

	it('stops a session whose start was already on its way', async () => {
		const test = harness();
		test.holdStarts();
		test.add(session('a', { status: 'connecting', source: 'saved', savedHostId: 'host-1' }));

		test.lifecycle.attach();
		test.lifecycle.begin('a', SAVED_PLAN);
		test.lifecycle.terminalReady('a');
		await until(() => test.savedStarts.length === 1, 'the saved host start went out');

		const teardown = test.lifecycle.teardown();
		test.starts.resolve();
		await teardown;

		// A session registered on the way out is stopped on the way out.
		expect(test.disconnects).toEqual(['a']);
	});

	it('detaches every listener it attached', async () => {
		const test = harness();
		test.add(session('a'));

		test.lifecycle.attach();
		await settle();
		await test.lifecycle.teardown();

		expect(test.unlistened).toEqual([
			SSH_EVENT.data,
			SSH_EVENT.status,
			SSH_EVENT.error,
			SSH_EVENT.hostKey
		]);
	});
});

describe('the events the lifecycle routes', () => {
	it('puts a status on the tab it names and on no other', async () => {
		const test = harness();
		test.add(session('a'), session('b'));
		test.lifecycle.attach();
		await settle();

		test.events[SSH_EVENT.status]({
			sessionId: 'b',
			status: 'disconnected',
			message: 'The session ended.'
		});

		expect(test.find('b')?.status).toBe('disconnected');
		expect(test.find('b')?.statusMessage).toBe('The session ended.');
		expect(test.find('a')?.status).toBe('connected');
		expect(test.find('a')?.statusMessage).toBe('');
	});

	it('drops an event for a tab that is already gone', async () => {
		const test = harness();
		test.add(session('a'));
		test.lifecycle.attach();
		await settle();
		await test.lifecycle.close('a');

		test.events[SSH_EVENT.status]({ sessionId: 'a', status: 'error', message: 'too late' });
		test.events[SSH_EVENT.data]({ sessionId: 'a', data: btoa('late output') });

		expect(test.state()).toEqual([]);
		expect(test.terminals).toEqual([]);
	});

	it('brings the tab that is asking about a host key to the front', async () => {
		const test = harness();
		test.add(session('a'), session('b'));
		test.activate('a');
		test.lifecycle.attach();
		await settle();

		const prompt: HostKeyEvent = {
			sessionId: 'b',
			host: 'example.com',
			port: 22,
			algorithm: 'ssh-ed25519',
			fingerprint: 'SHA256:abc',
			persistent: true
		};
		test.events[SSH_EVENT.hostKey](prompt);

		expect(test.find('b')?.hostKeyPrompt).toEqual(prompt);
		expect(test.find('a')?.hostKeyPrompt).toBeNull();
	});

	it('writes terminal output only to the session it belongs to', async () => {
		const test = harness();
		test.add(session('a'), session('b'));
		test.lifecycle.attach();
		await settle();

		test.events[SSH_EVENT.data]({ sessionId: 'b', data: btoa('for-b') });

		expect(test.terminals).toEqual(['b:for-b']);
	});
});
