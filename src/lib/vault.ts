import { invoke } from '@tauri-apps/api/core';

/**
 * The three vault states, matching the serialized `VaultStatus` in
 * `src-tauri/src/vault/mod.rs`.
 */
export type VaultStatus = 'uninitialized' | 'locked' | 'unlocked';

/** Reports whether a vault exists and whether it is open. */
export function vaultStatus(): Promise<VaultStatus> {
	return invoke('vault_status');
}

/** Creates the vault and leaves it unlocked. The password never leaves this call. */
export function createVault(password: string): Promise<VaultStatus> {
	return invoke('create_vault', { password });
}

export function unlockVault(password: string): Promise<VaultStatus> {
	return invoke('unlock_vault', { password });
}

export function lockVault(): Promise<VaultStatus> {
	return invoke('lock_vault');
}

const STATUS_LABELS: Record<VaultStatus, string> = {
	uninitialized: 'Not created',
	locked: 'Locked',
	unlocked: 'Unlocked'
};

export function vaultStatusLabel(status: VaultStatus): string {
	return STATUS_LABELS[status];
}

/**
 * Checks the create form before anything is sent to Rust: a master password is
 * required and the confirmation must match. The backend enforces the first rule
 * as well; the confirmation only exists here.
 */
export function createPasswordProblem(password: string, confirmation: string): string | null {
	if (password.length === 0) {
		return 'Enter a master password.';
	}
	if (password !== confirmation) {
		return 'The master password and confirmation do not match.';
	}
	return null;
}
