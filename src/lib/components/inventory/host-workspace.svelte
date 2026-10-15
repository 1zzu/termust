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
	import HostSectionPanel from './host-section-panel.svelte';
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
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import TerminalIcon from '@lucide/svelte/icons/terminal';
	import RadioIcon from '@lucide/svelte/icons/radio';
	import LayoutGridIcon from '@lucide/svelte/icons/layout-grid';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import { goto } from '$app/navigation';

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
		contextState = selectHostContext(host.id);
		if (formOpen) {
			openEditHost(host);
		}
	}

	function handleEditHost(host: Host) {
		if (contextState?.kind === 'quick-connect') {
			onquickclear?.();
		}
		openEditHost(host);
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
		closeHostForm();
	}

	function handleCloseQuickConnect() {
		contextState = closeContext();
		onquickclear?.();
	}

	function handleDelete(host: Host) {
		closeHostForm();
		void removeHost(host);
	}

	async function handleDeleteSelected() {
		const target = selectedHost ?? hosts.find((h) => h.id === editingHost);
		if (!target) return;
		closeHostForm();
		await removeHost(target);
	}

	// --- the host form ----------------------------------------------------

	function closeHostForm() {
		if (!formOpen && contextState === null) return;
		formOpen = false;
		editingHost = null;
		hostForm = emptyHostForm();
		contextState = closeContext();
	}

	function openCreateHost() {
		editingHost = null;
		hostForm = emptyHostForm();
		formOpen = true;
		contextState = null;
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

	async function submitHost(event?: SubmitEvent) {
		event?.preventDefault();
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

	async function handleConnectFromPanel() {
		const problem = hostFormProblem(hostForm);
		if (problem) {
			onerror(problem);
			return;
		}

		const input = hostInput(hostForm);
		const editing = editingHost;

		await onmutate(async () => {
			let targetHost: Host;
			if (editing === null) {
				targetHost = await createHost(input);
			} else {
				targetHost = await updateHost(editing, input);
			}
			onconnect(targetHost);
		});
	}

	function handleSearchConnect() {
		const target = searchQuery.trim();
		if (!target) return;

		// 1. Check if exact match to existing host label or address
		const match = hosts.find(
			(h) =>
				h.label.toLowerCase() === target.toLowerCase() ||
				h.address.toLowerCase() === target.toLowerCase()
		);
		if (match) {
			onconnect(match);
			return;
		}

		// 2. Parse target as SSH connection string (user@host:port or host:port or host)
		let username = '';
		let address = target;
		let port = 22;

		if (address.includes('@')) {
			const atIndex = address.indexOf('@');
			username = address.slice(0, atIndex);
			address = address.slice(atIndex + 1);
		}

		if (address.includes(':')) {
			const colonIndex = address.lastIndexOf(':');
			const portStr = address.slice(colonIndex + 1);
			const parsed = parseInt(portStr, 10);
			if (!isNaN(parsed) && parsed > 0 && parsed <= 65535) {
				port = parsed;
				address = address.slice(0, colonIndex);
			}
		}

		quickHost = address;
		quickPort = port;
		if (username) quickUsername = username;

		closeHostForm();
		contextState = { kind: 'quick-connect' };
	}

	async function removeHost(host: Host) {
		await onmutate(() => deleteHost(host.id));
	}
</script>

<ListDetail open={formOpen || contextState !== null}>
	{#snippet header()}
		<!-- Hosts Toolbar: search on top, actions row below -->
		<div class="flex flex-col gap-3">
			<!-- Top Search Bar with embedded Connect button -->
			<div class="relative w-full">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					placeholder="Find a host or ssh user@hostname..."
					bind:value={searchQuery}
					onkeydown={(e) => {
						if (e.key === 'Enter') handleSearchConnect();
					}}
					class="h-9 w-full rounded-lg border-border/70 bg-card/60 pr-24 pl-9 text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-primary/40"
					aria-label="Find a host or ssh address"
					disabled={busy}
				/>
				<Button
					size="sm"
					variant={searchQuery.trim() ? 'default' : 'secondary'}
					class="absolute top-1/2 right-1 h-7 -translate-y-1/2 px-3 text-xs font-medium"
					onclick={handleSearchConnect}
					disabled={!searchQuery.trim() || busy}
				>
					Connect
				</Button>
			</div>

			<!-- Secondary Action Bar (Single line) -->
			<div class="flex items-center justify-between gap-2">
				<div class="flex items-center gap-2">
					<Button
						size="sm"
						class="h-8 shrink-0 gap-1.5 text-xs font-medium"
						onclick={openCreateHost}
						disabled={busy || vaultStatus !== 'unlocked'}
					>
						<PlusIcon class="size-3.5" />
						<span>New host</span>
					</Button>
				</div>

				<div class="flex shrink-0 items-center gap-1.5 sm:gap-2">
					<ImportOpensshDialog {vaultStatus} onimported={onrefresh} />
					<Select
						type="single"
						bind:value={selectedGroupId}
						items={groupFilterOptions}
						disabled={busy || vaultStatus !== 'unlocked'}
					>
						<SelectTrigger
							id="group-filter"
							class="h-8 w-36 border-border/70 bg-card/60 text-xs"
							aria-label="Filter by group"
						>
							<div class="flex items-center gap-1.5 truncate">
								<FolderIcon class="size-3.5 shrink-0 text-muted-foreground" />
								<SelectValue placeholder="All groups" />
							</div>
						</SelectTrigger>
						<SelectContent>
							<SelectItem value={ALL_GROUPS_FILTER}>All groups</SelectItem>
							<SelectItem value={UNGROUPED_FILTER}>Ungrouped</SelectItem>
							{#each groups as group (group.id)}
								<SelectItem value={group.id}>{group.name}</SelectItem>
							{/each}
						</SelectContent>
					</Select>

					<!-- <Button
						variant="ghost"
						size="icon"
						class="size-8 shrink-0 text-muted-foreground hover:text-foreground"
						aria-label="Grid layout"
						title="Grid layout active"
					>
						<LayoutGridIcon class="size-3.5" />
					</Button> -->
				</div>
			</div>
		</div>
	{/snippet}

	<div class="flex flex-col gap-2">
		<div class="flex items-center justify-between px-0.5">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Hosts</h3>
			{#if filteredHosts.length > 0}
				<span class="text-[11px] text-muted-foreground">{filteredHosts.length} hosts</span>
			{/if}
		</div>

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
			onedit={handleEditHost}
			{onconnect}
			{onopenvault}
		/>
	</div>

	{#snippet panel()}
		{#if formOpen}
			<HostSectionPanel
				mode={editingHost === null ? 'create' : 'edit'}
				host={selectedHost}
				bind:form={hostForm}
				{groups}
				{credentials}
				{busy}
				connecting={selectedHost ? hostConnecting(selectedHost.id) : false}
				{vaultStatus}
				onsubmit={submitHost}
				onconnect={handleConnectFromPanel}
				ondelete={handleDeleteSelected}
				onclose={closeHostForm}
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
