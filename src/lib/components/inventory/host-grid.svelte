<script lang="ts">
	import type { CredentialSummary, Group, Host } from '#lib/inventory';
	import HostCard from './host-card.svelte';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ServerIcon from '@lucide/svelte/icons/server';

	let {
		hosts,
		totalHostCount,
		groups,
		credentials,
		selectedHostId = null,
		busy = false,
		hostConnecting,
		onselect,
		onconnect
	}: {
		hosts: Host[];
		totalHostCount: number;
		groups: Group[];
		credentials: CredentialSummary[];
		selectedHostId?: string | null;
		busy?: boolean;
		hostConnecting: (hostId: string) => boolean;
		onselect: (host: Host) => void;
		onconnect: (host: Host) => void;
	} = $props();

	function getGroupName(groupId: string | null): string {
		return groups.find((g) => g.id === groupId)?.name ?? '';
	}

	function getCredentialName(credentialId: string | null): string {
		return credentials.find((c) => c.id === credentialId)?.name ?? '';
	}
</script>

{#if totalHostCount === 0}
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
	<ul class="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3" role="list">
		{#each hosts as host (host.id)}
			<HostCard
				{host}
				groupName={getGroupName(host.groupId)}
				credentialName={getCredentialName(host.credentialId)}
				selected={host.id === selectedHostId}
				{busy}
				connecting={hostConnecting(host.id)}
				{onselect}
				{onconnect}
			/>
		{/each}
	</ul>
{/if}
