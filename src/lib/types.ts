/**
 * Types shared between the UI and the Rust backend. The field names match the
 * serde payloads in `src-tauri/src/ssh.rs`, so keep both sides in step.
 */

export type AuthMethod = 'password' | 'privateKey';

/** Arguments for the `connect_ssh` command. Secrets only ever live in here. */
export interface ConnectRequest {
	sessionId: string;
	host: string;
	port: number;
	username: string;
	auth: AuthMethod;
	cols: number;
	rows: number;
	password?: string;
	privateKey?: string;
	passphrase?: string;
}

/** `idle` only exists in the UI, before the first connection attempt. */
export type ConnectionStatus =
	| 'idle'
	| 'connecting'
	| 'verifying_host_key'
	| 'authenticating'
	| 'connected'
	| 'disconnected'
	| 'error';

/** Terminal output, base64 encoded so the bytes survive the JSON hop. */
export interface DataEvent {
	sessionId: string;
	data: string;
}

export interface StatusEvent {
	sessionId: string;
	status: ConnectionStatus;
	message: string | null;
}

/** Sent before the first connection to a host, to ask the user to trust it. */
export interface HostKeyEvent {
	sessionId: string;
	host: string;
	port: number;
	algorithm: string;
	fingerprint: string;
}

export interface ErrorEvent {
	sessionId: string;
	message: string;
}
