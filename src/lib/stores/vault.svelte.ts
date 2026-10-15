import { lockVault, vaultStatus as getVaultStatus, type VaultStatus } from '#lib/vault';
import { isVaultGateVisible } from '#lib/components/vault/vault-gate-state';
import { errorMessage } from '#lib/errors';

class VaultStore {
	status = $state<VaultStatus | undefined>(undefined);
	gateDismissed = $state(false);
	error = $state('');

	showGate = $derived(isVaultGateVisible(this.status, this.gateDismissed));

	async checkStatus() {
		this.error = '';
		try {
			this.status = await getVaultStatus();
		} catch (error) {
			this.error = errorMessage(error);
		}
	}

	setUnlocked(nextStatus: VaultStatus) {
		this.status = nextStatus;
		this.gateDismissed = false;
		this.error = '';
	}

	dismissGate() {
		this.gateDismissed = true;
	}

	openGate() {
		this.gateDismissed = false;
	}

	async lock() {
		try {
			const next = await lockVault();
			this.status = next;
			this.gateDismissed = false;
			return next;
		} catch (error) {
			this.error = errorMessage(error);
			throw error;
		}
	}
}

export const vaultStore = new VaultStore();
