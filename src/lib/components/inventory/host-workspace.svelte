<script lang="ts">
	import {
		NO_RELATION,
		createHost,
		deleteHost,
		emptyHostForm,
		hostFormProblem,
		hostInput,
		updateHost,
		type CredentialSummary,
		type Group,
		type Host,
		type HostFormValues
	} from '#lib/inventory';
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
	import type { InventoryPageProps } from './page-props';
	import HostGrid from './host-grid.svelte';
	import HostDetailPanel from './host-detail-panel.svelte';
	import HostFormPanel from './host-form-panel.svelte';
	import ListDetail from './list-detail.svelte';
	import QuickConnectPanel from './quick-connect-panel.svelte';
	import ImportOpensshDialog from '#lib/components/import-openssh-dialog.svelte';
	import { Button } from '#lib/components/ui/button';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue
	} from '#lib/components/ui/select';
	import { Input } from '#lib/components/ui/input';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ZapIcon from '@lucide/svelte/icons/zap';

	/**
	 * Hosts page: the saved hosts, their search and group filter, and the right
	 * column — the selected host, Quick Connect, or the host form itself.
	 *
	 * The page owns the host changes it makes; the workspace owns the decrypted
	 * lists it is handed and re-reads them after a change.
	 */
	let {
		hosts,
		groups,
		credentials,
		busy,
		vaultStatus,
		hostConnecting,
		onconnect,
		onmutate,
		onerror,
		onrefresh,
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
	}: InventoryPageProps & {
		hosts: Host[];
		groups: Group[];
		credentials: CredentialSummary[];
		vaultStatus?: VaultStatus;
		hostConnecting: (hostId: string) => boolean;
		onconnect: (host: Host) => void;
		/** Re-reads every list after something else changed the vault. */
		onrefresh: () => void | Promise<void>;
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

	// Host form: `editingHost` is null while creating a new one, and the form
	// takes over the right column while it is open.
	let formOpen = $state(false);
	let editingHost = $state<string | null>(null);
	let hostForm = $state<HostFormValues>(emptyHostForm());

	$effect(() => {
		if (openQuickConnect) {
			contextState = { kind: 'quick-connect' };
			openQuickConnect = false;
		}
	});

	$effect(() => {
		// Anything other than an open vault means no host form may stay on
		// screen — and its half-typed values go with it.
		if (vaultStatus !== 'unlocked' && formOpen) {
			formOpen = false;
			editingHost = null;
			hostForm = emptyHostForm();
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
		// Selecting a host closes the form: the right column shows one thing.
		closeHostForm();
		contextState = selectHostContext(host.id);
	}

	function handleToggleQuickConnect() {
		const next = toggleQuickConnectContext(contextState);
		if (contextState?.kind === 'quick-connect' && next === null) {
			onquickclear?.();
		}
		closeHostForm();
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
		void removeHost(host);
	}

	// --- the host form ----------------------------------------------------

	function closeHostForm() {
		if (!formOpen) return;
		formOpen = false;
		editingHost = null;
		hostForm = emptyHostForm();
	}

	function openCreateHost() {
		editingHost = null;
		hostForm = emptyHostForm();
		formOpen = true;
	}

	function openEditHost(host: Host) {
		editingHost = host.id;
		hostForm = {
			label: host.label,
			address: host.address,
			port: String(host.port),
			username: host.username,
			groupId: host.groupId ?? NO_RELATION,
			credentialId: host.credentialId ?? NO_RELATION
		};
		formOpen = true;
	}

	async function submitHost(event: SubmitEvent) {
		event.preventDefault();
		const problem = hostFormProblem(hostForm);
		if (problem) {
			onerror(problem);
			return;
		}

		const input = hostInput(hostForm);
		const editing = editingHost;
		await onmutate(async () => {
			if (editing === null) {
				await createHost(input);
			} else {
				await updateHost(editing, input);
			}
			closeHostForm();
		});
	}

	async function removeHost(host: Host) {
		await onmutate(() => deleteHost(host.id));
	}
</script>

<ListDetail open={formOpen || contextState !== null}>
	{#snippet header()}
		<!-- Hosts Toolbar: seamless directly in workspace, no outer card box -->
		<div class="flex flex-col gap-2.5">
			<!-- Search: primary control full width on top -->
			<div class="relative w-full">
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
				<!-- <span
				class="rounded border border-border/70 bg-muted/50 px-2 py-0.5 font-mono text-[11px] text-muted-foreground"
				title="Host count"
			>
				{#if searchQuery.trim() !== '' || selectedGroupId !== ALL_GROUPS_FILTER}
					{filteredHosts.length} / {hosts.length}
				{:else}
					{hosts.length}
				{/if}
			</span> -->

				<!-- New Host -->
				<Button
					size="sm"
					class="h-8 text-xs"
					onclick={openCreateHost}
					disabled={busy || vaultStatus !== 'unlocked'}
				>
					<PlusIcon data-icon="inline-start" />
					New host
				</Button>

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
				<ImportOpensshDialog {vaultStatus} onimported={onrefresh} />

				<!-- Group filter -->
				<Select
					type="single"
					bind:value={selectedGroupId}
					items={groupFilterOptions}
					disabled={busy || vaultStatus !== 'unlocked'}
				>
					<SelectTrigger id="group-filter" class="h-8 w-35 text-xs" aria-label="Filter by group">
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
			</div>
		</div>
	{/snippet}

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

	{#snippet panel()}
		{#if formOpen}
			<HostFormPanel
				mode={editingHost === null ? 'create' : 'edit'}
				bind:form={hostForm}
				{groups}
				{credentials}
				{busy}
				onsubmit={submitHost}
				onclose={closeHostForm}
			/>
		{:else if contextState?.kind === 'host' && selectedHost}
			<HostDetailPanel
				host={selectedHost}
				groupName={selectedHostGroup}
				credential={selectedHostCredential}
				{busy}
				connecting={hostConnecting(selectedHost.id)}
				{onconnect}
				onedit={openEditHost}
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
	{/snippet}
</ListDetail>
