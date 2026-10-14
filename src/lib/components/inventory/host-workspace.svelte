<script lang="ts">
	import type { CredentialSummary, Group, Host } from '#lib/inventory';
	import type { AuthMethod } from '#lib/types';
	import type { VaultStatus } from '#lib/vault';
	import { ALL_GROUPS_FILTER, UNGROUPED_FILTER, filterHosts } from './host-filter';
	import {
		closeContext,
		pruneContextAfterFilter,
		selectHostContext,
		toggleQuickConnectContext,
		type InventoryContext
	} from './context-state';
	import HostGrid from './host-grid.svelte';
	import HostDetailPanel from './host-detail-panel.svelte';
	import QuickConnectPanel from './quick-connect-panel.svelte';
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
	import ZapIcon from '@lucide/svelte/icons/zap';

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
		onimported,
		quickHost = $bindable('localhost'),
		quickPort = $bindable(22),
		quickUsername = $bindable(''),
		quickAuth = $bindable<AuthMethod>('password'),
		quickPassword = $bindable(''),
		quickPrivateKey = $bindable(''),
		quickPassphrase = $bindable(''),
		quickSubmitting = false,
		onquicksubmit,
		onquickclear,
		openQuickConnect = $bindable(false),
		onopenvault
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
		quickHost?: string;
		quickPort?: number;
		quickUsername?: string;
		quickAuth?: AuthMethod;
		quickPassword?: string;
		quickPrivateKey?: string;
		quickPassphrase?: string;
		quickSubmitting?: boolean;
		onquicksubmit?: (event: SubmitEvent) => void;
		onquickclear?: () => void;
		openQuickConnect?: boolean;
		onopenvault?: () => void;
	} = $props();

	let searchQuery = $state('');
	let selectedGroupId = $state<string>(ALL_GROUPS_FILTER);
	let contextState = $state<InventoryContext>(null);

	$effect(() => {
		if (openQuickConnect) {
			contextState = { kind: 'quick-connect' };
			openQuickConnect = false;
		}
	});

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

	// If selected host is no longer in filtered results, prune context
	$effect(() => {
		const visibleIds = filteredHosts.map((h) => h.id);
		contextState = pruneContextAfterFilter(contextState, visibleIds);
	});

	const selectedHostId = $derived(contextState?.kind === 'host' ? contextState.hostId : null);

	const selectedHost = $derived(
		selectedHostId ? (filteredHosts.find((h) => h.id === selectedHostId) ?? null) : null
	);

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

	function handleSelectHost(host: Host) {
		if (contextState?.kind === 'quick-connect') {
			onquickclear?.();
		}
		contextState = selectHostContext(host.id);
	}

	function handleToggleQuickConnect() {
		const next = toggleQuickConnectContext(contextState);
		if (contextState?.kind === 'quick-connect' && next === null) {
			onquickclear?.();
		}
		contextState = next;
	}

	function handleCloseDetail() {
		contextState = closeContext();
	}

	function handleCloseQuickConnect() {
		contextState = closeContext();
		onquickclear?.();
	}

	function handleDelete(host: Host) {
		contextState = closeContext();
		ondelete(host);
	}
</script>

<div class="flex flex-col gap-4">
	<!-- Hosts Toolbar: seamless directly in workspace, no outer card box -->
	<div class="flex flex-col gap-2.5 sm:flex-row sm:items-center sm:justify-between">
		<!-- Search: primary control with largest space -->
		<div class="relative min-w-[200px] flex-1">
			<SearchIcon
				class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
			/>
			<Input
				type="search"
				placeholder="Search hosts…"
				bind:value={searchQuery}
				class="h-8 pl-8 text-xs"
				aria-label="Search hosts"
				disabled={busy || vaultStatus !== 'unlocked'}
			/>
		</div>

		<!-- Actions row: Filter, Quick Connect, Import OpenSSH, New Host -->
		<div class="flex flex-wrap items-center gap-2">
			<!-- Host count badge -->
			<span
				class="rounded border border-border/70 bg-muted/50 px-2 py-0.5 font-mono text-[11px] text-muted-foreground"
				title="Host count"
			>
				{#if searchQuery.trim() !== '' || selectedGroupId !== ALL_GROUPS_FILTER}
					{filteredHosts.length} / {hosts.length}
				{:else}
					{hosts.length}
				{/if}
			</span>

			<!-- Group filter -->
			<Select
				type="single"
				bind:value={selectedGroupId}
				items={groupFilterOptions}
				disabled={busy || vaultStatus !== 'unlocked'}
			>
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

			<!-- Quick Connect button -->
			<Button
				variant={contextState?.kind === 'quick-connect' ? 'default' : 'outline'}
				size="sm"
				class="h-8 text-xs"
				onclick={handleToggleQuickConnect}
			>
				<ZapIcon data-icon="inline-start" />
				Quick Connect
			</Button>

			<!-- Import OpenSSH -->
			<ImportOpensshDialog {vaultStatus} {onimported} />

			<!-- New Host -->
			<Button
				size="sm"
				class="h-8 text-xs"
				onclick={oncreate}
				disabled={busy || vaultStatus !== 'unlocked'}
			>
				<PlusIcon data-icon="inline-start" />
				New host
			</Button>
		</div>
	</div>

	<!-- Workspace Content: Grid + Unified Right Context Panel -->
	<div class="flex flex-col gap-4 lg:flex-row lg:items-start">
		<div class="min-w-0 flex-1">
			<HostGrid
				hosts={filteredHosts}
				totalHostCount={hosts.length}
				{groups}
				{credentials}
				{selectedHostId}
				{busy}
				{vaultStatus}
				{hostConnecting}
				onselect={handleSelectHost}
				{onconnect}
				{onopenvault}
			/>
		</div>

		{#if contextState?.kind === 'host' && selectedHost}
			<HostDetailPanel
				host={selectedHost}
				groupName={selectedHostGroup}
				credential={selectedHostCredential}
				{busy}
				connecting={hostConnecting(selectedHost.id)}
				{onconnect}
				{onedit}
				ondelete={handleDelete}
				onclose={handleCloseDetail}
			/>
		{:else if contextState?.kind === 'quick-connect'}
			<QuickConnectPanel
				bind:host={quickHost}
				bind:port={quickPort}
				bind:username={quickUsername}
				bind:auth={quickAuth}
				bind:password={quickPassword}
				bind:privateKey={quickPrivateKey}
				bind:passphrase={quickPassphrase}
				submitting={quickSubmitting}
				onsubmit={(e) => onquicksubmit?.(e)}
				onclose={handleCloseQuickConnect}
			/>
		{/if}
	</div>
</div>
