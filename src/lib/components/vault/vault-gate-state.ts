import type { VaultStatus } from '#lib/vault';

/**
 * Determines whether the blocking vault gate dialog should be shown.
 *
 * Gate is shown whenever the vault is not unlocked and has not been dismissed
 * (via Continue with Quick Connect).
 */
export function isVaultGateVisible(status: VaultStatus | undefined, dismissed: boolean): boolean {
	return status !== 'unlocked' && !dismissed;
}

/**
 * Maps the internal active session ID to the visually selected session ID.
 *
 * When the Hosts (inventory) workspace is active, no terminal tab appears selected
 * to prevent the visual bug where both Hosts and a terminal session look active simultaneously.
 * The underlying activeSessionId is completely preserved.
 */
export function getVisualActiveSessionId(
	activeWorkspace: 'inventory' | 'terminal',
	activeSessionId: string | null
): string | null {
	return activeWorkspace === 'terminal' ? activeSessionId : null;
}
