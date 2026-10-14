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

/**
 * Arguments for the `start_saved_ssh` command: which saved host to connect, and
 * the size of the terminal.
 *
 * There is deliberately no address, port, username, credential id or secret
 * here. The backend reads all of that from the vault, so nothing the frontend
 * sends can point the connection at another server or another credential.
 */
export interface SavedConnectRequest {
	sessionId: string;
	hostId: string;
	cols: number;
	rows: number;
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
	/**
	 * Whether accepting will still be trusted after a restart. The backend
	 * decides this, because only it knows whether the vault is unlocked.
	 */
	persistent: boolean;
}

export interface ErrorEvent {
	sessionId: string;
	message: string;
}
