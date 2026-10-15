<script lang="ts">
	import LockIcon from '@lucide/svelte/icons/lock';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';

	import { Button } from '#lib/components/ui/button';
	import type { KnownHost } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import KnownHostCard from './known-host-card.svelte';

	let {
		knownHosts,
		totalCount,
		selectedHostId = null,
		busy = false,
		vaultStatus,
		onselect,
		onopenvault
	}: {
		knownHosts: KnownHost[];
		totalCount: number;
		selectedHostId?: string | null;
		busy?: boolean;
		vaultStatus?: VaultStatus;
		onselect: (knownHost: KnownHost) => void;
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
			Unlock the local vault to access trusted host keys.
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
			Create a vault to save trusted host keys and encrypted inventory.
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
		<ShieldCheckIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No host keys trusted yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Approving a host fingerprint during your first SSH connection will add it here automatically.
		</p>
	</div>
{:else if knownHosts.length === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<SearchIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No matching host keys</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			No host keys match your search query. Try adjusting your search.
		</p>
	</div>
{:else}
	<div class="@container">
		<ul
			class="grid grid-cols-1 gap-3 @lg:grid-cols-2 @3xl:grid-cols-3 @5xl:grid-cols-4"
			role="list"
		>
			{#each knownHosts as knownHost (knownHost.id)}
				<KnownHostCard {knownHost} selected={knownHost.id === selectedHostId} {busy} {onselect} />
			{/each}
		</ul>
	</div>
{/if}
