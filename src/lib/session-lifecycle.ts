import type { UnlistenFn } from '@tauri-apps/api/event';

import { errorMessage } from './errors';
import { beginSessionStart, type SessionStart } from './session-start';
import {
	applyStatus,
	beginClosing,
	cancelClosing,
	clearHostKeyPrompt,
	findSession,
	isLiveStatus,
	liveSessionIds,
	nextActiveId,
	removeSession,
	routeEvent,
	sessionCanDisconnect,
	setDisconnecting,
	setHostKeyPrompt,
	setSessionError,
	writeOutput,
	type TerminalSession
} from './session-tabs';
import { SSH_EVENT, savedConnectRequest } from './ssh';
import type {
	ConnectRequest,
	DataEvent,
	ErrorEvent,
	HostKeyEvent,
	SavedConnectRequest,
	StatusEvent
} from './types';

/**
 * What one tab's session is started from.
 *
 * A Quick Connect plan carries the credentials the user typed — and only for as
 * long as its start runs. A saved host plan carries nothing: the backend reads
 * the address and the secret from the vault.
 */
export type StartPlan =
	| { kind: 'quick'; request: Omit<ConnectRequest, 'cols' | 'rows'> }
	| { kind: 'saved'; hostId: string };

/**
 * Everything the lifecycle needs from the page.
 *
 * The tabs themselves stay the page's Svelte state: `read`/`write` are how this
 * module changes them, which is what keeps one source of truth for the list and
 * makes the whole lifecycle drivable in a test with a plain array.
 */
export interface SessionLifecycleDeps {
	read(): TerminalSession[];
	write(sessions: TerminalSession[]): void;
	readActive(): string | null;
	writeActive(sessionId: string | null): void;
	/** Attaches one of the window's four backend events. */
	listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn>;
	/** `connect_ssh`: the whole request, credentials included. */
	connect(request: ConnectRequest): Promise<void>;
	/** `start_saved_ssh`: a host id and a terminal size, and nothing else. */
	startSaved(request: SavedConnectRequest): Promise<void>;
	/** `disconnect_ssh`. */
	disconnect(sessionId: string): Promise<void>;
	/** The size the session should open with. */
	size(sessionId: string): { cols: number; rows: number };
	/** Delivers decoded output to one session's terminal. */
	writeTerminal(sessionId: string, bytes: Uint8Array): void;
	/** The listeners could not all be attached, so no session may start. */
	onEventsUnavailable(error: unknown): void;
	/** A Quick Connect start let go of what it held, so the form can be reused. */
	onQuickStartFinished(): void;
}

export interface SessionLifecycle {
	/** Attaches the window's event listeners. */
	attach(): void;
	/** Starts the session for a tab that already exists. */
	begin(sessionId: string, plan: StartPlan): void;
	/** The terminal for this tab exists now. */
	terminalReady(sessionId: string): void;
	/** Stops the session of the tab on screen, keeping its tab. */
	disconnect(): Promise<void>;
	/** Stops one session and removes only its tab. */
	close(sessionId: string): Promise<void>;
	/** Window teardown: no pending start may reach the backend, and nothing that
	 * is running may outlive the window. */
	teardown(): Promise<void>;
}

/**
 * The window's session lifecycle: subscribing to the backend's events, starting
 * a tab's session, stopping one, and closing a tab.
 *
 * Everything here is about *when* the backend may be touched and in what order,
 * which is exactly the part a race can break:
 *
 * - a start is cancellable and never reaches the backend after its tab is gone;
 * - no session starts at all unless all four listeners are in place;
 * - a close waits for an in-flight start before it disconnects and removes the
 *   tab, so nothing can register a session after the tab is gone;
 * - a session counts as live until the backend has really stopped it, so a
 *   restore is refused for as long as that takes.
 */
export function createSessionLifecycle(deps: SessionLifecycleDeps): SessionLifecycle {
	const starts: Record<string, SessionStart<StartPlan>> = {};
	let unlisteners: UnlistenFn[] = [];
	let attached = false;
	let disposed = false;

	// The one promise every start races against. It is rejected when the
	// listeners are not all in place, and that rejection is what keeps a session
	// from running with nowhere for its events to go.
	let resolveListeners!: () => void;
	let rejectListeners!: (error: unknown) => void;
	const listenersReady = new Promise<void>((resolve, reject) => {
		resolveListeners = resolve;
		rejectListeners = reject;
	});
	// Reported to the user here, without swallowing it: the promise stays
	// rejected, so every start still sees the failure.
	listenersReady.catch(deps.onEventsUnavailable);

	/**
	 * Attaches the window's four listeners, one at a time.
	 *
	 * All or nothing: a partial subscription would silently drop one kind of
	 * event, which is worse than not starting a session at all, so whatever was
	 * attached before the failure is detached again.
	 */
	async function subscribeAll(): Promise<UnlistenFn[]> {
		const registered: UnlistenFn[] = [];
		try {
			registered.push(await deps.listen<DataEvent>(SSH_EVENT.data, onData));
			registered.push(await deps.listen<StatusEvent>(SSH_EVENT.status, onStatus));
			registered.push(await deps.listen<ErrorEvent>(SSH_EVENT.error, onError));
			registered.push(await deps.listen<HostKeyEvent>(SSH_EVENT.hostKey, onHostKey));
			return registered;
		} catch (error) {
			registered.forEach((unlisten) => unlisten());
			throw error;
		}
	}

	// Every event is routed by the session id it carries and nothing else, so no
	// terminal, status, error or prompt can land on another session — and an
	// event for a tab that is already gone matches nothing and is dropped.

	function onData(payload: DataEvent) {
		writeOutput(deps.read(), payload, deps.writeTerminal);
	}

	function onStatus(payload: StatusEvent) {
		const session = routeEvent(deps.read(), payload);
		if (!session) return;
		deps.write(applyStatus(deps.read(), session.id, payload.status, payload.message ?? ''));
	}

	function onError(payload: ErrorEvent) {
		const session = routeEvent(deps.read(), payload);
		if (!session) return;
		deps.write(setSessionError(deps.read(), session.id, payload.message));
	}

	function onHostKey(payload: HostKeyEvent) {
		const session = routeEvent(deps.read(), payload);
		if (!session) return;
		deps.write(setHostKeyPrompt(deps.read(), session.id, payload));
		// The prompt has to be seen, so its tab comes forward — unless the tab on
		// screen is already asking about another host.
		if (findSession(deps.read(), deps.readActive())?.hostKeyPrompt == null) {
			deps.writeActive(session.id);
		}
	}

	function attach(): void {
		if (attached) return;
		attached = true;

		void subscribeAll().then(
			(registered) => {
				if (disposed) {
					registered.forEach((unlisten) => unlisten());
					return;
				}
				unlisteners = registered;
				resolveListeners();
			},
			(error) => {
				// Reported by the handler on `listenersReady`; the point here is
				// that the promise every start races against stays rejected.
				rejectListeners(error);
			}
		);
	}

	function begin(sessionId: string, plan: StartPlan): void {
		// One tab has one start.
		if (starts[sessionId]) return;

		// Decided once, here, and the only thing the callbacks below ever know
		// about the plan. They must not close over the plan itself: for Quick
		// Connect it carries the credentials the user typed, and a callback that
		// outlives the start would keep them alive with it.
		const isQuickStart = plan.kind === 'quick';

		const control = beginSessionStart<StartPlan>(plan, {
			sessionId,
			listenersReady,
			// A tab that is closing, or is already gone, must not be started.
			tabExists: (id) => {
				const session = findSession(deps.read(), id);
				return session !== undefined && !session.closing;
			},
			// The parameter is the plan the start hands over, not the one this
			// function was given: this closure holds no plan of its own.
			invoke: (request) => {
				// The size is read here, once the terminal exists, so the session
				// opens with the geometry it will actually be shown at.
				const size = deps.size(sessionId);
				return request.kind === 'quick'
					? deps.connect({ ...request.request, cols: size.cols, rows: size.rows })
					: deps.startSaved(savedConnectRequest(sessionId, request.hostId, size.cols, size.rows));
			},
			onError: (id, error) => {
				// The failure belongs to this tab: every other session is
				// untouched, and a start that was cancelled or closed never gets
				// here at all.
				deps.write(setSessionError(deps.read(), id, errorMessage(error)));
				deps.write(applyStatus(deps.read(), id, 'error', ''));
			},
			onFinished: (id) => {
				delete starts[id];
				// Only ever the start that owns the form, so a cleanup can never
				// clear what a later submission is holding. The decision is a plain
				// boolean, so this callback needs no plan at all.
				if (isQuickStart) deps.onQuickStartFinished();
			}
		});

		starts[sessionId] = control;
	}

	function terminalReady(sessionId: string): void {
		starts[sessionId]?.ready();
	}

	async function disconnect(): Promise<void> {
		const session = findSession(deps.read(), deps.readActive());
		if (!session || !sessionCanDisconnect(session)) return;

		// The session stays live until the backend has finished stopping it: that
		// is what keeps input, and a restore, held back meanwhile.
		deps.write(setDisconnecting(deps.read(), session.id, true));
		try {
			await deps.disconnect(session.id);
		} catch (error) {
			// Nothing is assumed about the connection. The tab stays live with its
			// scrollback, and the user can try again.
			deps.write(setDisconnecting(deps.read(), session.id, false));
			deps.write(setSessionError(deps.read(), session.id, errorMessage(error)));
			return;
		}

		deps.write(clearHostKeyPrompt(deps.read(), session.id));
		deps.write(setDisconnecting(deps.read(), session.id, false));
		deps.write(applyStatus(deps.read(), session.id, 'disconnected', ''));
	}

	async function close(sessionId: string): Promise<void> {
		const session = findSession(deps.read(), sessionId);
		// Closing twice does nothing: the first close owns the tab already, and a
		// second call cannot pick a different neighbour for it.
		if (!session || session.closing) return;
		deps.write(beginClosing(deps.read(), sessionId));

		// A start that is still on its way is ended first. Otherwise the backend
		// can register a session after the tab is gone — no terminal, no
		// controls, and nothing left that could ever stop it.
		const start = starts[sessionId];
		if (start) {
			start.cancel();
			await start.done;
		}

		// Whether the backend may already have a session for this id. A start that
		// never reached it has nothing to disconnect, so a tab closed before its
		// session existed is simply taken away.
		const registered = start ? start.invoked : isLiveStatus(session.status);
		if (registered) {
			try {
				await deps.disconnect(sessionId);
			} catch (error) {
				// The session is still running, so its tab stays: that is what
				// lets the user stop it, instead of hiding a live session.
				deps.write(cancelClosing(deps.read(), sessionId));
				deps.write(setSessionError(deps.read(), sessionId, errorMessage(error)));
				return;
			}
		}

		// Another close may have removed it in the meantime.
		if (!findSession(deps.read(), sessionId)) return;

		// Chosen while the closing tab is still in the list, so the neighbour it
		// leaves behind is the one that takes its place.
		const next = nextActiveId(deps.read(), sessionId);
		deps.write(removeSession(deps.read(), sessionId));
		if (deps.readActive() === sessionId) deps.writeActive(next);
	}

	async function teardown(): Promise<void> {
		disposed = true;
		unlisteners.forEach((unlisten) => unlisten());
		unlisteners = [];

		// Every start that has not reached the backend yet is stopped here, so a
		// window that is closing cannot open a new connection on its way out.
		const pending = Object.values(starts);
		pending.forEach((start) => start.cancel());
		await Promise.all(pending.map((start) => start.done));

		// Read again once they have settled: a start that was already on its way
		// can have registered a session in the meantime, and a session that
		// outlives the window is exactly what this is here to prevent.
		const live = liveSessionIds(deps.read());
		await Promise.all(
			live.map((sessionId) =>
				deps.disconnect(sessionId).catch(() => {
					// The backend cleans up on its own when the window is gone.
				})
			)
		);
	}

	return { attach, begin, terminalReady, disconnect, close, teardown };
}
