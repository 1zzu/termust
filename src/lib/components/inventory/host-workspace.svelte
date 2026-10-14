<script lang="ts">
	import type { CredentialSummary, Group, Host } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import { ALL_GROUPS_FILTER, UNGROUPED_FILTER, filterHosts } from './host-filter';
	import HostGrid from './host-grid.svelte';
	import HostDetailPanel from './host-detail-panel.svelte';
	import ImportOpensshDialog from '#lib/components/import-openssh-dialog.svelte';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue
	} from '#lib/components/ui/select';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';

	let {
		hosts,
		groups,
		credentials,
		busy = false,
		vaultStatus,
		hostConnecting,
		onconnect,
		onedit,
		ondelete,
		oncreate,
		onimported
	}: {
		hosts: Host[];
		groups: Group[];
		credentials: CredentialSummary[];
		busy?: boolean;
		vaultStatus?: VaultStatus;
		hostConnecting: (hostId: string) => boolean;
		onconnect: (host: Host) => void;
		onedit: (host: Host) => void;
		ondelete: (host: Host) => void;
		oncreate: () => void;
		onimported: () => void;
	} = $props();

	let searchQuery = $state('');
	let selectedGroupId = $state<string>(ALL_GROUPS_FILTER);
	let selectedHostId = $state<string | null>(null);

	const groupFilterOptions = $derived([
		{ value: ALL_GROUPS_FILTER, label: 'All groups' },
		{ value: UNGROUPED_FILTER, label: 'Ungrouped' },
		...groups.map((group) => ({ value: group.id, label: group.name }))
	]);

	const filteredHosts = $derived(
		filterHosts(hosts, {
			query: searchQuery,
			groupId: selectedGroupId
		})
	);

	// If group was removed, revert filter to All groups
	$effect(() => {
		if (
			selectedGroupId !== ALL_GROUPS_FILTER &&
			selectedGroupId !== UNGROUPED_FILTER &&
			!groups.some((group) => group.id === selectedGroupId)
		) {
			selectedGroupId = ALL_GROUPS_FILTER;
		}
	});

	// If selected host is no longer in filtered results (e.g. search, group filter change, deletion, refresh), clear selection
	$effect(() => {
		if (selectedHostId !== null && !filteredHosts.some((h) => h.id === selectedHostId)) {
			selectedHostId = null;
		}
	});

	const selectedHost = $derived(filteredHosts.find((h) => h.id === selectedHostId) ?? null);

	const selectedHostGroup = $derived(
		selectedHost?.groupId
			? (groups.find((group) => group.id === selectedHost.groupId)?.name ?? '')
			: ''
	);

	const selectedHostCredential = $derived(
		selectedHost?.credentialId
			? (credentials.find((credential) => credential.id === selectedHost.credentialId) ?? null)
			: null
	);

	function handleDelete(host: Host) {
		selectedHostId = null;
		ondelete(host);
	}
</script>

<div class="flex flex-col gap-4">
	<!-- Hosts Toolbar -->
	<div
		class="flex flex-col gap-3 rounded-lg border border-border/60 bg-muted/20 p-3 sm:flex-row sm:items-center sm:justify-between"
	>
		<div class="flex items-center gap-2.5">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Hosts</h3>
			<span
				class="rounded-full border border-border/80 bg-muted/60 px-2 py-0.5 font-mono text-[11px] text-muted-foreground"
			>
				{#if searchQuery.trim() !== '' || selectedGroupId !== ALL_GROUPS_FILTER}
					{filteredHosts.length} of {hosts.length}
				{:else}
					{hosts.length}
				{/if}
			</span>
		</div>

		<div class="flex flex-wrap items-center gap-2">
			<!-- Search input -->
			<div class="relative max-w-xs min-w-[150px] flex-1">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					placeholder="Search hosts…"
					bind:value={searchQuery}
					class="h-8 pl-8 text-xs"
					aria-label="Search hosts"
				/>
			</div>

			<!-- Group filter -->
			<Select type="single" bind:value={selectedGroupId} items={groupFilterOptions} disabled={busy}>
				<SelectTrigger id="group-filter" class="h-8 w-[140px] text-xs" aria-label="Filter by group">
					<SelectValue placeholder="All groups" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value={ALL_GROUPS_FILTER}>All groups</SelectItem>
					<SelectItem value={UNGROUPED_FILTER}>Ungrouped</SelectItem>
					{#each groups as group (group.id)}
						<SelectItem value={group.id}>{group.name}</SelectItem>
					{/each}
				</SelectContent>
			</Select>

			<!-- Action buttons -->
			<div class="flex shrink-0 items-center gap-2">
				<ImportOpensshDialog {vaultStatus} {onimported} />
				<Button size="sm" class="h-8 text-xs" onclick={oncreate} disabled={busy}>
					<PlusIcon data-icon="inline-start" />
					New host
				</Button>
			</div>
		</div>
	</div>

	<!-- Workspace Content: Grid + Detail Context Panel -->
	<div class="flex flex-col gap-4 lg:flex-row lg:items-start">
		<div class="min-w-0 flex-1">
			<HostGrid
				hosts={filteredHosts}
				totalHostCount={hosts.length}
				{groups}
				{credentials}
				{selectedHostId}
				{busy}
				{hostConnecting}
				onselect={(host) => (selectedHostId = host.id)}
				{onconnect}
			/>
		</div>

		{#if selectedHost}
			<HostDetailPanel
				host={selectedHost}
				groupName={selectedHostGroup}
				credential={selectedHostCredential}
				{busy}
				connecting={hostConnecting(selectedHost.id)}
				{onconnect}
				{onedit}
				ondelete={handleDelete}
				onclose={() => (selectedHostId = null)}
			/>
		{/if}
	</div>
</div>
