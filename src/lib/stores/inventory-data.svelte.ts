import { errorMessage } from '#lib/errors';
import {
	emptyInventory,
	listCredentials,
	listGroups,
	listHosts,
	listKnownHosts,
	type CredentialSummary,
	type Group,
	type Host,
	type InventoryCounts,
	type KnownHost
} from '#lib/inventory';
import { listSnippets, type Snippet } from '#lib/snippets';

class InventoryStore {
	hosts = $state<Host[]>([]);
	groups = $state<Group[]>([]);
	credentials = $state<CredentialSummary[]>([]);
	knownHosts = $state<KnownHost[]>([]);
	snippets = $state<Snippet[]>([]);
	busy = $state(false);
	errorText = $state('');

	counts = $derived<InventoryCounts>({
		hosts: this.hosts.length,
		credentials: this.credentials.length,
		groups: this.groups.length,
		knownHosts: this.knownHosts.length,
		snippets: this.snippets.length
	});

	async refresh() {
		try {
			this.groups = await listGroups();
			this.credentials = await listCredentials();
			this.hosts = await listHosts();
			this.knownHosts = await listKnownHosts();
			this.snippets = await listSnippets();
		} catch (error) {
			this.errorText = errorMessage(error);
		}
	}

	async mutate(work: () => Promise<void>): Promise<boolean> {
		this.busy = true;
		this.errorText = '';
		try {
			await work();
			await this.refresh();
			return true;
		} catch (error) {
			this.errorText = errorMessage(error);
			return false;
		} finally {
			this.busy = false;
		}
	}

	reportError(message: string) {
		this.errorText = message;
	}

	clear() {
		const empty = emptyInventory();
		this.hosts = empty.hosts;
		this.groups = empty.groups;
		this.credentials = empty.credentials;
		this.knownHosts = empty.knownHosts;
		this.snippets = empty.snippets;
		this.errorText = '';
	}
}

export const inventoryStore = new InventoryStore();
