import { invoke } from '@tauri-apps/api/core';

import type { VaultStatus } from './vault';

/**
 * The records the backend returns. They mirror `src-tauri/src/inventory/model.rs`
 * as camelCase; each payload also carries a `version` the UI never needs.
 */

export interface Group {
	id: string;
	name: string;
	createdAt: number;
	updatedAt: number;
}

export interface Host {
	id: string;
	label: string;
	address: string;
	port: number;
	username: string;
	groupId: string | null;
	credentialId: string | null;
	createdAt: number;
	updatedAt: number;
}

export type CredentialKind = 'password' | 'private_key';

/** What the frontend may know about a credential: deliberately no secret. */
export interface CredentialSummary {
	id: string;
	name: string;
	kind: CredentialKind;
	createdAt: number;
	updatedAt: number;
}

export interface Inventory {
	hosts: Host[];
	groups: Group[];
	credentials: CredentialSummary[];
}

/**
 * Freshly emptied lists. The panel assigns these when the vault locks, so no
 * decrypted record is left in the component's state; a new object each time
 * means no two places can end up sharing one array.
 */
export function emptyInventory(): Inventory {
	return { hosts: [], groups: [], credentials: [] };
}

// --- IPC ------------------------------------------------------------------

export function listGroups(): Promise<Group[]> {
	return invoke('list_groups');
}

export function createGroup(name: string): Promise<Group> {
	return invoke('create_group', { name });
}

export function renameGroup(id: string, name: string): Promise<Group> {
	return invoke('rename_group', { id, name });
}

export function deleteGroup(id: string): Promise<void> {
	return invoke('delete_group', { id });
}

export function listHosts(): Promise<Host[]> {
	return invoke('list_hosts');
}

export function getHost(id: string): Promise<Host> {
	return invoke('get_host', { id });
}

export function createHost(host: HostInput): Promise<Host> {
	return invoke('create_host', { host });
}

export function updateHost(id: string, host: HostInput): Promise<Host> {
	return invoke('update_host', { id, host });
}

export function deleteHost(id: string): Promise<void> {
	return invoke('delete_host', { id });
}

export function listCredentials(): Promise<CredentialSummary[]> {
	return invoke('list_credentials');
}

export function createPasswordCredential(
	name: string,
	password: string
): Promise<CredentialSummary> {
	return invoke('create_password_credential', { name, password });
}

export function createPrivateKeyCredential(
	name: string,
	privateKey: string,
	passphrase: string | null
): Promise<CredentialSummary> {
	return invoke('create_private_key_credential', { name, privateKey, passphrase });
}

export function renameCredential(id: string, name: string): Promise<CredentialSummary> {
	return invoke('rename_credential', { id, name });
}

export function replacePassword(id: string, password: string): Promise<void> {
	return invoke('replace_password', { id, password });
}

export function replacePrivateKey(
	id: string,
	privateKey: string,
	passphrase: string | null
): Promise<void> {
	return invoke('replace_private_key', { id, privateKey, passphrase });
}

export function deleteCredential(id: string): Promise<void> {
	return invoke('delete_credential', { id });
}

// --- host form ------------------------------------------------------------

/** The backend's `HostInput`. A null id means "no relationship". */
export interface HostInput {
	label: string;
	address: string;
	port: number;
	username: string;
	groupId: string | null;
	credentialId: string | null;
}

/**
 * A `Select` needs a real value for "no relationship", so the form uses a
 * sentinel that becomes `null` on the way to the backend.
 */
export const NO_RELATION = '__none__';

export interface HostFormValues {
	label: string;
	address: string;
	port: string;
	username: string;
	groupId: string;
	credentialId: string;
}

export function emptyHostForm(): HostFormValues {
	return {
		label: '',
		address: '',
		port: '22',
		username: '',
		groupId: NO_RELATION,
		credentialId: NO_RELATION
	};
}

/** Checks the host form before anything is sent. Never mentions a secret. */
export function hostFormProblem(form: HostFormValues): string | null {
	if (form.label.trim() === '') {
		return 'Enter a label for this host.';
	}
	if (form.address.trim() === '') {
		return 'Enter a hostname or IP address.';
	}
	const port = Number(form.port);
	if (!Number.isInteger(port) || port < 1 || port > 65535) {
		return 'Port must be between 1 and 65535.';
	}
	if (form.username.trim() === '') {
		return 'Enter a username.';
	}
	return null;
}

/** Builds the exact request the backend expects, with the chosen relationships. */
export function hostInput(form: HostFormValues): HostInput {
	return {
		label: form.label.trim(),
		address: form.address.trim(),
		port: Number(form.port),
		username: form.username.trim(),
		groupId: relationId(form.groupId),
		credentialId: relationId(form.credentialId)
	};
}

function relationId(value: string): string | null {
	return value === '' || value === NO_RELATION ? null : value;
}

// --- credential form ------------------------------------------------------

export type CredentialFormMode = 'create' | 'rename' | 'replace';

export interface CredentialFormValues {
	name: string;
	kind: CredentialKind;
	/** The password, the private key, or nothing at all. Never read back. */
	secret: string;
	passphrase: string;
}

export function emptyCredentialForm(): CredentialFormValues {
	return { name: '', kind: 'password', secret: '', passphrase: '' };
}

/**
 * Checks the credential form. The message never repeats the secret: only
 * whether it is missing.
 */
export function credentialFormProblem(
	mode: CredentialFormMode,
	form: CredentialFormValues
): string | null {
	if (mode !== 'replace' && form.name.trim() === '') {
		return 'Enter a name for this credential.';
	}
	if (mode !== 'rename' && form.secret === '') {
		return form.kind === 'password' ? 'Enter a password.' : 'Paste the private key.';
	}
	return null;
}

// --- visibility -----------------------------------------------------------

/**
 * The inventory is only ever on screen while the vault is open. Locking takes
 * it away instead of hiding it, and the panel throws its contents away.
 */
export function inventoryVisible(status: VaultStatus | undefined): boolean {
	return status === 'unlocked';
}
