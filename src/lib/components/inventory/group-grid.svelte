<script lang="ts">
	import FolderIcon from '@lucide/svelte/icons/folder';
	import LockIcon from '@lucide/svelte/icons/lock';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';

	import { Button } from '#lib/components/ui/button';
	import type { Group, Host } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import GroupCard from './group-card.svelte';

	let {
		groups,
		totalCount,
		hosts = [],
		selectedGroupId = null,
		busy = false,
		vaultStatus,
		onselect,
		onopenvault
	}: {
		groups: Group[];
		totalCount: number;
		hosts?: Host[];
		selectedGroupId?: string | null;
		busy?: boolean;
		vaultStatus?: VaultStatus;
		onselect: (group: Group) => void;
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
			Unlock the local vault to access saved hosts and groups.
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
			Create a vault to save encrypted hosts, groups, and credentials.
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
		<FolderIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No groups yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Groups help organize your saved hosts into categories like production, staging, or
			environments.
		</p>
	</div>
{:else if groups.length === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<SearchIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No matching groups</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			No groups match your search query. Try adjusting your search query.
		</p>
	</div>
{:else}
	<div class="@container">
		<ul
			class="grid grid-cols-1 gap-3 @lg:grid-cols-2 @3xl:grid-cols-3 @5xl:grid-cols-4"
			role="list"
		>
			{#each groups as group (group.id)}
				<GroupCard {group} {hosts} selected={group.id === selectedGroupId} {busy} {onselect} />
			{/each}
		</ul>
	</div>
{/if}
