import { decodeOutput, isConnected } from './ssh';
import type { ConnectionStatus, HostKeyEvent } from './types';

/**
 * One SSH session as the window knows it: a tab in the strip, and the terminal
 * behind it.
 *
 * This is the descriptor only. It deliberately carries no credential — no
 * password, no private key and no passphrase — so a tab cannot keep a secret
 * alive after the request that used it was handed to the backend. The xterm
 * instance itself lives in the tab component, keyed by `id`.
 */
export interface TerminalSession {
	id: string;
	title: string;
	source: 'quick' | 'saved';
	/** The saved host this session came from; absent for Quick Connect. */
	savedHostId?: string;
	status: ConnectionStatus;
	/** What the backend last said about this session, if anything. */
	statusMessage: string;
	/** The failure this session ended with, shown on its own tab only. */
	errorMessage: string;
	/** The host key this session is waiting on an answer for. */
	hostKeyPrompt: HostKeyEvent | null;
	/**
	 * Set while the connection is being stopped but the tab stays: the session
	 * counts as live until the backend has really finished with it, so input and
	 * a restore are both still held back.
	 */
	disconnecting: boolean;
	/** Set while the tab is being closed, so a second close click is ignored. */
	closing: boolean;
}

/**
 * What the page can ask of one session's terminal.
 *
 * The terminal belongs to the tab component; this is only the handle the page
 * needs to deliver output and to fit a tab that came forward.
 */
export interface TerminalRuntime {
	write(bytes: Uint8Array): void;
	fit(): void;
	focus(): void;
	size(): { cols: number; rows: number };
}

export interface NewSession {
	title: string;
	source: 'quick' | 'saved';
	savedHostId?: string;
}

/** The title of a Quick Connect session that cannot name a user and a host. */
export const QUICK_CONNECT_TITLE = 'Quick Connect';

/** A tab's own element ids, used for the tab/tabpanel relationship. */
export function tabElementId(sessionId: string): string {
	return `session-tab-${sessionId}`;
}

export function panelElementId(sessionId: string): string {
	return `session-panel-${sessionId}`;
}

// --- creating -------------------------------------------------------------

/** A new tab, with an id of its own and no connection yet. */
export function createSession(init: NewSession): TerminalSession {
	return {
		id: crypto.randomUUID(),
		title: init.title,
		source: init.source,
		savedHostId: init.savedHostId,
		status: 'connecting',
		statusMessage: '',
		errorMessage: '',
		hostKeyPrompt: null,
		disconnecting: false,
		closing: false
	};
}

/**
 * What a Quick Connect tab is called: `username@host`, which is enough to tell
 * two of them apart and says nothing secret. Without a username there is
 * nothing useful to show, so the form's own name is used.
 */
export function quickConnectTitle(username: string, host: string): string {
	const user = username.trim();
	const target = host.trim();
	return user !== '' && target !== '' ? `${user}@${target}` : QUICK_CONNECT_TITLE;
}

// --- looking up -----------------------------------------------------------

export function findSession(
	sessions: readonly TerminalSession[],
	id: string | null | undefined
): TerminalSession | undefined {
	if (id === null || id === undefined) return undefined;
	return sessions.find((session) => session.id === id);
}

/** Replaces one session and leaves every other tab exactly as it was. */
function mapSession(
	sessions: readonly TerminalSession[],
	id: string,
	change: (session: TerminalSession) => TerminalSession
): TerminalSession[] {
	return sessions.map((session) => (session.id === id ? change(session) : session));
}

export function addSession(
	sessions: readonly TerminalSession[],
	session: TerminalSession
): TerminalSession[] {
	return [...sessions, session];
}

export function removeSession(sessions: readonly TerminalSession[], id: string): TerminalSession[] {
	return sessions.filter((session) => session.id !== id);
}

// --- the tab strip --------------------------------------------------------

/**
 * The tab to select after `closedId` goes away.
 *
 * It is the tab that takes the same place in the strip, or the one before it
 * when the closed tab was last. `null` means no tab is left.
 */
export function nextActiveId(
	sessions: readonly TerminalSession[],
	closedId: string
): string | null {
	const index = sessions.findIndex((session) => session.id === closedId);
	if (index === -1) return null;
	return sessions[index + 1]?.id ?? sessions[index - 1]?.id ?? null;
}

/**
 * Which tab a key press on the strip moves to, or `null` when the key is not
 * one this strip handles. Arrow keys stay inside the strip instead of wrapping:
 * a tab list that jumps from the last tab to the first is easy to overshoot.
 */
export function tabKeyboardTarget(key: string, index: number, count: number): number | null {
	if (count === 0 || index < 0 || index >= count) return null;
	if (key === 'Home') return 0;
	if (key === 'End') return count - 1;
	if (key === 'ArrowLeft') return Math.max(0, index - 1);
	if (key === 'ArrowRight') return Math.min(count - 1, index + 1);
	return null;
}

// --- status ---------------------------------------------------------------

/** A session that may still be talking to a host, connecting included. */
export function isLiveStatus(status: ConnectionStatus): boolean {
	return (
		status === 'connecting' ||
		status === 'verifying_host_key' ||
		status === 'authenticating' ||
		status === 'connected'
	);
}

/**
 * Only a live shell takes input, and never one that is closing or whose
 * connection is already being stopped.
 */
export function sessionAcceptsInput(session: TerminalSession): boolean {
	return !session.closing && !session.disconnecting && isConnected(session.status);
}

/**
 * Whether the Disconnect button of this tab may be used at all.
 *
 * A session that is already stopping is left alone: the first click owns the
 * stop, and the button comes back if that stop turns out to have failed.
 */
export function sessionCanDisconnect(session: TerminalSession): boolean {
	return isLiveStatus(session.status) && !session.disconnecting && !session.closing;
}

/**
 * Records what the backend reported. A tab that ended keeps its place and its
 * scrollback: only its status changes, so the user can read what happened and
 * close it when they are done.
 */
export function applyStatus(
	sessions: readonly TerminalSession[],
	id: string,
	status: ConnectionStatus,
	message: string
): TerminalSession[] {
	return mapSession(sessions, id, (session) => ({
		...session,
		status,
		statusMessage: message,
		hostKeyPrompt: isLiveStatus(status) ? session.hostKeyPrompt : null
	}));
}

/** Stores a failure on the tab it happened to, and nowhere else. */
export function setSessionError(
	sessions: readonly TerminalSession[],
	id: string,
	message: string
): TerminalSession[] {
	return mapSession(sessions, id, (session) => ({ ...session, errorMessage: message }));
}

export function setHostKeyPrompt(
	sessions: readonly TerminalSession[],
	id: string,
	prompt: HostKeyEvent | null
): TerminalSession[] {
	return mapSession(sessions, id, (session) => ({ ...session, hostKeyPrompt: prompt }));
}

export function clearHostKeyPrompt(
	sessions: readonly TerminalSession[],
	id: string
): TerminalSession[] {
	return setHostKeyPrompt(sessions, id, null);
}

/** The first tab still waiting for an answer, in strip order. */
export function firstPromptSessionId(sessions: readonly TerminalSession[]): string | null {
	return sessions.find((session) => session.hostKeyPrompt !== null)?.id ?? null;
}

// --- closing and cleanup --------------------------------------------------

/** Marks a tab as closing, which is what makes a second close click a no-op. */
export function beginClosing(sessions: readonly TerminalSession[], id: string): TerminalSession[] {
	return mapSession(sessions, id, (session) => ({ ...session, closing: true }));
}

/**
 * Takes the closing mark back, for a close that could not stop the session: the
 * tab stays and is live, so the user can try again.
 */
export function cancelClosing(sessions: readonly TerminalSession[], id: string): TerminalSession[] {
	return mapSession(sessions, id, (session) => ({ ...session, closing: false }));
}

/** Marks a session as being stopped, or takes that mark back. */
export function setDisconnecting(
	sessions: readonly TerminalSession[],
	id: string,
	disconnecting: boolean
): TerminalSession[] {
	return mapSession(sessions, id, (session) => ({ ...session, disconnecting }));
}

/**
 * Every session the backend may still be running, whatever tab is on screen.
 *
 * Used to disconnect the whole window when it is torn down, and to keep a
 * restore out while any of them is alive.
 */
export function liveSessionIds(sessions: readonly TerminalSession[]): string[] {
	return sessions.filter((session) => isLiveStatus(session.status)).map((session) => session.id);
}

/**
 * Whether anything in the strip is still talking to a host, or on its way out.
 *
 * A session counts while it is connecting, while its connection is being
 * stopped, and while its tab is being closed: in all three the backend may
 * still have a task, which is what a restore has to stay away from.
 */
export function hasLiveSessions(sessions: readonly TerminalSession[]): boolean {
	return sessions.some(
		(session) => isLiveStatus(session.status) || session.disconnecting || session.closing
	);
}

/**
 * Whether a tab for this saved host is already being opened.
 *
 * It is true only while that tab is still connecting, so a double click cannot
 * start two connections at once, while connecting a saved host that is already
 * up stays allowed — a duplicate session is a feature, not a mistake.
 */
export function savedHostConnecting(sessions: readonly TerminalSession[], hostId: string): boolean {
	return sessions.some(
		(session) =>
			session.source === 'saved' &&
			session.savedHostId === hostId &&
			!session.closing &&
			session.status === 'connecting'
	);
}

// --- input and resize -----------------------------------------------------

export interface OutgoingInput {
	sessionId: string;
	data: string;
}

/** The keystroke to send for a session, or `null` when it cannot take input. */
export function inputToSend(session: TerminalSession, data: string): OutgoingInput | null {
	if (data === '' || !sessionAcceptsInput(session)) return null;
	return { sessionId: session.id, data };
}

export interface OutgoingResize {
	sessionId: string;
	cols: number;
	rows: number;
}

/** The resize to send for a session, or `null` when it has no live PTY. */
export function resizeToSend(
	session: TerminalSession,
	cols: number,
	rows: number
): OutgoingResize | null {
	if (!sessionAcceptsInput(session)) return null;
	return { sessionId: session.id, cols, rows };
}

// --- events ---------------------------------------------------------------

/** Just enough of a backend payload to find its session. */
export interface SessionPayload {
	sessionId?: unknown;
}

/**
 * Finds the session an event belongs to, using the session id it carries and
 * nothing else: not the title, not the host, and not the tab that happens to be
 * on screen. An event for a tab that was closed matches nothing and is dropped.
 */
export function routeEvent(
	sessions: readonly TerminalSession[],
	payload: SessionPayload
): TerminalSession | undefined {
	const id = payload.sessionId;
	return typeof id === 'string' ? findSession(sessions, id) : undefined;
}

/**
 * Delivers terminal output to the session that owns it.
 *
 * The writer is called with the routed session id, so output can only ever
 * reach the terminal of the session the event names. A late event for a closed
 * tab is reported as not delivered.
 */
export function writeOutput(
	sessions: readonly TerminalSession[],
	payload: { sessionId: string; data: string },
	write: (sessionId: string, bytes: Uint8Array) => void
): boolean {
	const session = routeEvent(sessions, payload);
	if (!session) return false;
	write(session.id, decodeOutput(payload.data));
	return true;
}

// --- snippets -------------------------------------------------------------

/** The session a snippet preview was opened for. */
export interface SnippetTarget {
	sessionId: string;
	title: string;
}

/**
 * Binds a preview to one session when it opens, so a later tab switch can never
 * change where the command would go.
 */
export function snippetTarget(sessionId: string | undefined, title: string): SnippetTarget | null {
	if (sessionId === undefined || sessionId === '') return null;
	return { sessionId, title };
}

/**
 * Whether the command may still be sent: the bound session has to be the one on
 * screen and still connected. Switching tabs disables the confirmation instead
 * of quietly aiming it at another session.
 */
export function snippetTargetReady(
	target: SnippetTarget | null,
	activeSessionId: string | null,
	activeStatus: ConnectionStatus,
	running: boolean
): boolean {
	return (
		!running && target !== null && target.sessionId === activeSessionId && isConnected(activeStatus)
	);
}
