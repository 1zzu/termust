<script lang="ts">
	import LockIcon from '@lucide/svelte/icons/lock';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ServerIcon from '@lucide/svelte/icons/server';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';

	import { Button } from '#lib/components/ui/button';
	import type { CredentialSummary, Group, Host } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import HostCard from './host-card.svelte';

	let {
		hosts,
		totalHostCount,
		groups,
		credentials,
		selectedHostId = null,
		busy = false,
		vaultStatus,
		hostConnecting,
		onselect,
		onedit,
		onconnect,
		onopenvault
	}: {
		hosts: Host[];
		totalHostCount: number;
		groups: Group[];
		credentials: CredentialSummary[];
		selectedHostId?: string | null;
		busy?: boolean;
		vaultStatus?: VaultStatus;
		hostConnecting: (hostId: string) => boolean;
		onselect: (host: Host) => void;
		onedit?: (host: Host) => void;
		onconnect: (host: Host) => void;
		onopenvault?: () => void;
	} = $props();

	function getGroupName(groupId: string | null): string {
		return groups.find((g) => g.id === groupId)?.name ?? '';
	}

	function getCredentialName(credentialId: string | null): string {
		return credentials.find((c) => c.id === credentialId)?.name ?? '';
	}
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
{:else if totalHostCount === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<ServerIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No saved hosts yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Saved hosts live encrypted in this vault. Add your first host or import from an OpenSSH
			config.
		</p>
	</div>
{:else if hosts.length === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<SearchIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No matching hosts</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			No hosts match your search query or group filter. Try adjusting your search query or selecting
			"All groups".
		</p>
	</div>
{:else}
	<div class="@container">
		<ul
			class="grid grid-cols-1 gap-3 @lg:grid-cols-2 @3xl:grid-cols-3 @5xl:grid-cols-4"
			role="list"
		>
			{#each hosts as host (host.id)}
				<HostCard
					{host}
					groupName={getGroupName(host.groupId)}
					credentialName={getCredentialName(host.credentialId)}
					selected={host.id === selectedHostId}
					{busy}
					connecting={hostConnecting(host.id)}
					{onselect}
					{onedit}
					{onconnect}
				/>
			{/each}
		</ul>
	</div>
{/if}
