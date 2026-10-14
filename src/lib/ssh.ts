import { invoke } from '@tauri-apps/api/core';
import type { ConnectRequest, ConnectionStatus, SavedConnectRequest } from './types';

/** Event names emitted by `src-tauri/src/ssh.rs`. Keep both sides in step. */
export const SSH_EVENT = {
	data: 'ssh://data',
	status: 'ssh://status',
	hostKey: 'ssh://host-key',
	error: 'ssh://error'
} as const;

export function connectSsh(request: ConnectRequest): Promise<void> {
	return invoke('connect_ssh', { request });
}

export function writeSsh(sessionId: string, data: string): Promise<void> {
	return invoke('write_ssh', { sessionId, data });
}

export function resizeSsh(sessionId: string, cols: number, rows: number): Promise<void> {
	return invoke('resize_ssh', { sessionId, cols, rows });
}

/**
 * Connects a saved host. The backend looks its address, username and credential
 * up in the vault; this command only names the host.
 */
export function startSavedSsh(request: SavedConnectRequest): Promise<void> {
	return invoke('start_saved_ssh', { request });
}

/**
 * The payload `start_saved_ssh` takes, built in one place so that these four
 * fields, and only these four, are what leaves the frontend.
 */
export function savedConnectRequest(
	sessionId: string,
	hostId: string,
	cols: number,
	rows: number
): SavedConnectRequest {
	return { sessionId, hostId, cols, rows };
}

/**
 * What the strip above the terminal says about the session on screen: a saved
 * host by name, or the ad-hoc form that started it.
 */
export function sessionLabel(savedHostLabel: string | null): string {
	return savedHostLabel === null ? 'Quick Connect' : `Saved host: ${savedHostLabel}`;
}

export function respondHostKey(sessionId: string, accepted: boolean): Promise<void> {
	return invoke('respond_host_key', { sessionId, accepted });
}

export function disconnectSsh(sessionId: string): Promise<void> {
	return invoke('disconnect_ssh', { sessionId });
}

/**
 * Every payload carries its session id, because a window can hold several
 * sessions and no terminal may ever show another session's output.
 */
export function belongsToSession(payload: unknown, sessionId: string): boolean {
	return (
		typeof payload === 'object' &&
		payload !== null &&
		'sessionId' in payload &&
		(payload as { sessionId: unknown }).sessionId === sessionId
	);
}

/**
 * Rust sends terminal bytes as base64. Decoding to bytes instead of text lets
 * xterm.js do its own UTF-8 handling, so split multi-byte characters survive.
 */
export function decodeOutput(base64: string): Uint8Array {
	const binary = atob(base64);
	const bytes = new Uint8Array(binary.length);
	for (let index = 0; index < binary.length; index += 1) {
		bytes[index] = binary.charCodeAt(index);
	}
	return bytes;
}

const STATUS_LABELS: Record<ConnectionStatus, string> = {
	idle: 'Not connected',
	connecting: 'Connecting',
	verifying_host_key: 'Checking host key',
	authenticating: 'Authenticating',
	connected: 'Connected',
	disconnected: 'Disconnected',
	error: 'Error'
};

export function statusLabel(status: ConnectionStatus): string {
	return STATUS_LABELS[status];
}

/** A session is usable for input only once the shell is running. */
export function isConnected(status: ConnectionStatus): boolean {
	return status === 'connected';
}

/**
 * What accepting a host key will mean, in the words the prompt shows.
 *
 * The backend decides whether the approval can be stored, so the prompt never
 * promises more than the vault can keep.
 */
export function hostKeyPromise(persistent: boolean): string {
	return persistent
		? 'This key is stored in your local vault, so this device will not ask again.'
		: 'The vault is locked, so this key is remembered only until the app closes. Unlock the vault to store it for good.';
}

// `errorMessage` is shared with the vault UI, so it lives in `errors.ts`.
// Re-exported here so the SSH call sites keep their existing import.
export { errorMessage } from './errors';
