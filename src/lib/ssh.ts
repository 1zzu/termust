import { invoke } from '@tauri-apps/api/core';
import type { ConnectRequest, ConnectionStatus } from './types';

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

// `errorMessage` is shared with the vault UI, so it lives in `errors.ts`.
// Re-exported here so the SSH call sites keep their existing import.
export { errorMessage } from './errors';
