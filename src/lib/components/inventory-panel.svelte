<script lang="ts">
	import { untrack } from 'svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import BackupPanel from '#lib/components/inventory/backup-panel.svelte';
	import CredentialsPanel from '#lib/components/inventory/credentials-panel.svelte';
	import GroupsPanel from '#lib/components/inventory/groups-panel.svelte';
	import HostWorkspace from '#lib/components/inventory/host-workspace.svelte';
	import KnownHostsPanel from '#lib/components/inventory/known-hosts-panel.svelte';
	import SnippetsPanel from '#lib/components/inventory/snippets-panel.svelte';
	import { errorMessage } from '#lib/errors';
	import {
		emptyInventory,
		inventoryVisible,
		listCredentials,
		listGroups,
		listHosts,
		listKnownHosts,
		type CredentialSummary,
		type Group,
		type Host,
		type InventoryCounts,
		type InventorySection,
		type KnownHost
	} from '#lib/inventory';
	import { listSnippets, type Snippet } from '#lib/snippets';
	import type { AuthMethod, ConnectionStatus } from '#lib/types';
	import type { VaultStatus } from '#lib/vault';

	/**
	 * Inventory workspace.
	 *
	 * Owns the decrypted lists, the busy flag and the one alert failures are
	 * shown in, and renders the page the active section names. A page owns its
	 * own dialogs and forms: it changes the vault through `onmutate`, reports
	 * what it found itself through `onerror`, and is handed the lists it shows.
	 *
	 * The root owns the vault status and every session, so the workspace is told
	 * when the vault opens or closes, which tab is on screen (the snippets page
	 * follows that one), whether any session is still alive at all, and which
	 * saved hosts are already being connected.
	 */
	let {
		vaultStatus,
		sessionStatus = 'idle',
		sessionId,
		sessionTarget = '',
		hasLiveSessions = false,
		hostConnecting = () => false,
		onconnect,
		section = $bindable<InventorySection>('hosts'),
		oncountschange,
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
		vaultStatus: VaultStatus | undefined;
		/** The status of the tab on screen. */
		sessionStatus?: ConnectionStatus;
		/** The id of the tab on screen, if there is one. */
		sessionId?: string;
		/** What that tab is called. */
		sessionTarget?: string;
		/** Whether any tab is still talking to a host, closing included. */
		hasLiveSessions?: boolean;
		/** Whether a tab for this saved host is already being opened. */
		hostConnecting?: (hostId: string) => boolean;
		onconnect?: (host: Host) => void;
		section?: InventorySection;
		oncountschange?: (counts: InventoryCounts) => void;
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

	// The decrypted lists, one copy each: the badges and the pages that are
	// handed them both read from here, and a change re-reads them through here.
	let hosts = $state<Host[]>([]);
	let groups = $state<Group[]>([]);
	let credentials = $state<CredentialSummary[]>([]);
	let knownHosts = $state<KnownHost[]>([]);
	let snippets = $state<Snippet[]>([]);
	let errorText = $state('');
	let busy = $state(false);

	const visible = $derived(inventoryVisible(vaultStatus));
	// Not reactive on purpose: it only remembers whether the lists are loaded.
	let holding = false;
	let previousSection = section;

	$effect(() => {
		const nextCounts: InventoryCounts = {
			hosts: hosts.length,
			credentials: credentials.length,
			groups: groups.length,
			knownHosts: knownHosts.length,
			snippets: snippets.length
		};
		untrack(() => {
			oncountschange?.(nextCounts);
		});
	});

	$effect(() => {
		if (vaultStatus !== 'unlocked' && section !== 'hosts') {
			section = 'hosts';
		}
	});

	$effect(() => {
		const current = section;
		if (current !== previousSection) {
			if (previousSection === 'hosts') {
				onquickclear?.();
			}
			previousSection = current;
			if (visible) {
				void refresh();
			}
		}
	});

	$effect(() => {
		if (visible && !holding) {
			holding = true;
			void refresh();
		} else if (!visible && holding) {
			holding = false;
			clear();
		}
	});

	async function refresh() {
		try {
			groups = await listGroups();
			credentials = await listCredentials();
			hosts = await listHosts();
			knownHosts = await listKnownHosts();
			snippets = await listSnippets();
		} catch (error) {
			errorText = errorMessage(error);
		}
	}

	/**
	 * Runs one change against the vault: busy on, the alert cleared, every list
	 * re-read, and a failure reported there instead of thrown. Returns whether
	 * the change itself went through.
	 */
	async function mutate(work: () => Promise<void>): Promise<boolean> {
		busy = true;
		errorText = '';
		try {
			await work();
			await refresh();
			return true;
		} catch (error) {
			errorText = errorMessage(error);
			return false;
		} finally {
			busy = false;
		}
	}

	/** Reports a problem a page found itself. An empty message clears the alert. */
	function reportError(message: string) {
		errorText = message;
	}

	/**
	 * Throws away everything this workspace holds: used when the vault locks.
	 * The page that was on screen goes with the lists by itself, and the host
	 * form closes on its own vault watch; nothing decrypted is left behind.
	 */
	function clear() {
		const empty = emptyInventory();
		hosts = empty.hosts;
		groups = empty.groups;
		credentials = empty.credentials;
		knownHosts = empty.knownHosts;
		snippets = empty.snippets;
		errorText = '';
		onquickclear?.();
	}
</script>

<div class="flex min-h-0 flex-1 flex-col gap-4">
	{#if errorText}
		<Alert variant="destructive" class="shrink-0">
			<AlertTitle>Inventory error</AlertTitle>
			<AlertDescription>{errorText}</AlertDescription>
		</Alert>
	{/if}

	{#if section === 'hosts'}
		<HostWorkspace
			{hosts}
			{groups}
			{credentials}
			{busy}
			{vaultStatus}
			{hostConnecting}
			onconnect={(host) => onconnect?.(host)}
			onmutate={mutate}
			onerror={reportError}
			onrefresh={refresh}
			bind:quickHost
			bind:quickPort
			bind:quickUsername
			bind:quickAuth
			bind:quickPassword
			bind:quickPrivateKey
			bind:quickPassphrase
			{quickSubmitting}
			{onquicksubmit}
			{onquickclear}
			bind:openQuickConnect
			{onopenvault}
		/>
	{:else if section === 'credentials'}
		<CredentialsPanel {credentials} {busy} onmutate={mutate} onerror={reportError} />
	{:else if section === 'groups'}
		<GroupsPanel {groups} {busy} onmutate={mutate} onerror={reportError} />
	{:else if section === 'known_hosts'}
		<KnownHostsPanel {knownHosts} {busy} onmutate={mutate} onerror={reportError} />
	{:else if section === 'snippets'}
		<SnippetsPanel
			{snippets}
			{busy}
			{sessionStatus}
			{sessionId}
			{sessionTarget}
			onmutate={mutate}
			onerror={reportError}
		/>
	{:else if section === 'backup'}
		<BackupPanel {vaultStatus} {hasLiveSessions} onrestored={refresh} />
	{/if}
</div>
