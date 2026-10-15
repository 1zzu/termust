<script lang="ts">
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LockIcon from '@lucide/svelte/icons/lock';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';

	import { Button } from '#lib/components/ui/button';
	import type { CredentialSummary, Host } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import CredentialCard from './credential-card.svelte';

	let {
		credentials,
		totalCount,
		hosts = [],
		selectedCredentialId = null,
		busy = false,
		vaultStatus,
		onselect,
		onopenvault
	}: {
		credentials: CredentialSummary[];
		totalCount: number;
		hosts?: Host[];
		selectedCredentialId?: string | null;
		busy?: boolean;
		vaultStatus?: VaultStatus;
		onselect: (credential: CredentialSummary) => void;
		onopenvault?: () => void;
	} = $props();
</script>

{#if vaultStatus === 'locked'}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<LockIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">Saved inventory is locked</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Unlock the local vault to access saved hosts and credentials.
		</p>
		{#if onopenvault}
			<Button variant="outline" size="sm" class="mt-4 text-xs" onclick={onopenvault}>
				Unlock Vault
			</Button>
		{/if}
	</div>
{:else if vaultStatus === 'uninitialized'}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<ShieldAlertIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No local vault yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Create a vault to save encrypted hosts and credentials.
		</p>
		{#if onopenvault}
			<Button variant="outline" size="sm" class="mt-4 text-xs" onclick={onopenvault}>
				Create Vault
			</Button>
		{/if}
	</div>
{:else if totalCount === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<KeyRoundIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No saved credentials yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Saved credentials live encrypted in this vault. Add passwords or SSH private keys to connect
			to your hosts securely.
		</p>
	</div>
{:else if credentials.length === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<SearchIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No matching credentials</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			No credentials match your search query or filter. Try adjusting your query or selecting "All
			types".
		</p>
	</div>
{:else}
	<div class="@container">
		<ul
			class="grid grid-cols-1 gap-3 @lg:grid-cols-2 @3xl:grid-cols-3 @5xl:grid-cols-4"
			role="list"
		>
			{#each credentials as credential (credential.id)}
				<CredentialCard
					{credential}
					{hosts}
					selected={credential.id === selectedCredentialId}
					{busy}
					{onselect}
				/>
			{/each}
		</ul>
	</div>
{/if}
