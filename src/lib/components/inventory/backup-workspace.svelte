<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LockIcon from '@lucide/svelte/icons/lock';
	import ServerIcon from '@lucide/svelte/icons/server';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import TerminalIcon from '@lucide/svelte/icons/terminal';
	import UploadIcon from '@lucide/svelte/icons/upload';

	import { Button } from '#lib/components/ui/button';
	import { Card } from '#lib/components/ui/card';
	import type { CredentialSummary, Group, Host, KnownHost } from '#lib/inventory';
	import type { Snippet } from '#lib/snippets';
	import type { VaultStatus } from '#lib/vault';
	import BackupExportPanel from './backup-export-panel.svelte';
	import BackupRestorePanel from './backup-restore-panel.svelte';
	import ListDetail from './list-detail.svelte';

	let {
		vaultStatus,
		hasLiveSessions = false,
		hosts = [],
		credentials = [],
		groups = [],
		snippets = [],
		knownHosts = [],
		busy = false,
		onrestored,
		onopenvault
	}: {
		vaultStatus?: VaultStatus;
		hasLiveSessions?: boolean;
		hosts?: Host[];
		credentials?: CredentialSummary[];
		groups?: Group[];
		snippets?: Snippet[];
		knownHosts?: KnownHost[];
		busy?: boolean;
		onrestored?: () => void | Promise<void>;
		onopenvault?: () => void;
	} = $props();

	let activePanel = $state<'export' | 'restore' | null>(null);

	$effect(() => {
		// Close panel if vault locks
		if (vaultStatus !== 'unlocked' && activePanel !== null) {
			activePanel = null;
		}
	});

	function handleToggleExport() {
		activePanel = activePanel === 'export' ? null : 'export';
	}

	function handleToggleRestore() {
		activePanel = activePanel === 'restore' ? null : 'restore';
	}
</script>

<ListDetail open={activePanel !== null}>
	<!-- {#snippet header()}
		<div class="flex flex-col gap-1">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Backup & Restore</h3>
			<p class="text-xs text-muted-foreground">
				A backup is one encrypted file locked with a password you choose. It preserves your entire
				vault data.
			</p>
		</div>
	{/snippet} -->

	<div class="flex flex-col gap-4">
		{#if vaultStatus === 'locked'}
			<div
				class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
				role="status"
			>
				<LockIcon class="size-10 text-muted-foreground/50" />
				<h4 class="mt-3 text-sm font-medium text-foreground">Saved inventory is locked</h4>
				<p class="mt-1 max-w-sm text-xs text-muted-foreground">
					Unlock the local vault to export or restore backups.
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
					Create a vault first to start saving hosts and generating backups.
				</p>
				{#if onopenvault}
					<Button variant="outline" size="sm" class="mt-4 text-xs" onclick={onopenvault}>
						Create Vault
					</Button>
				{/if}
			</div>
		{:else}
			<!-- Vault inventory overview pills -->
			<div class="flex flex-col gap-2 rounded-lg border border-border/60 bg-muted/20 p-3.5">
				<span class="text-xs font-medium text-muted-foreground"
					>Current Protected Vault Records</span
				>
				<div class="flex flex-wrap items-center gap-2 text-xs">
					<div
						class="flex items-center gap-1.5 rounded-md border border-border/70 bg-card px-2.5 py-1 text-foreground shadow-xs"
					>
						<ServerIcon class="size-3.5 text-muted-foreground" />
						<span class="font-semibold">{hosts.length}</span>
						<span class="text-muted-foreground">{hosts.length === 1 ? 'host' : 'hosts'}</span>
					</div>
					<div
						class="flex items-center gap-1.5 rounded-md border border-border/70 bg-card px-2.5 py-1 text-foreground shadow-xs"
					>
						<KeyRoundIcon class="size-3.5 text-muted-foreground" />
						<span class="font-semibold">{credentials.length}</span>
						<span class="text-muted-foreground"
							>{credentials.length === 1 ? 'credential' : 'credentials'}</span
						>
					</div>
					<div
						class="flex items-center gap-1.5 rounded-md border border-border/70 bg-card px-2.5 py-1 text-foreground shadow-xs"
					>
						<FolderIcon class="size-3.5 text-muted-foreground" />
						<span class="font-semibold">{groups.length}</span>
						<span class="text-muted-foreground">{groups.length === 1 ? 'group' : 'groups'}</span>
					</div>
					<div
						class="flex items-center gap-1.5 rounded-md border border-border/70 bg-card px-2.5 py-1 text-foreground shadow-xs"
					>
						<TerminalIcon class="size-3.5 text-muted-foreground" />
						<span class="font-semibold">{snippets.length}</span>
						<span class="text-muted-foreground"
							>{snippets.length === 1 ? 'snippet' : 'snippets'}</span
						>
					</div>
					<div
						class="flex items-center gap-1.5 rounded-md border border-border/70 bg-card px-2.5 py-1 text-foreground shadow-xs"
					>
						<ShieldCheckIcon class="size-3.5 text-muted-foreground" />
						<span class="font-semibold">{knownHosts.length}</span>
						<span class="text-muted-foreground">trusted keys</span>
					</div>
				</div>
			</div>

			<!-- Action Cards Grid -->
			<div class="@container">
				<div class="grid grid-cols-1 gap-3.5 @xl:grid-cols-2">
					<!-- Export Card -->
					<Card
						role="button"
						tabindex={0}
						onclick={handleToggleExport}
						onkeydown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') {
								e.preventDefault();
								handleToggleExport();
							}
						}}
						aria-pressed={activePanel === 'export'}
						class="cursor-pointer flex-col justify-between gap-4 p-4 transition-all duration-150 outline-none {activePanel ===
						'export'
							? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
							: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
					>
						<div class="flex items-start gap-3.5">
							<div
								class="flex size-10 shrink-0 items-center justify-center rounded-full bg-blue-600 text-white shadow-xs"
								aria-hidden="true"
							>
								<DownloadIcon class="size-5" />
							</div>
							<div class="flex flex-col gap-1">
								<h4 class="text-sm font-semibold text-foreground">Export Encrypted Backup</h4>
								<p class="text-xs leading-relaxed text-muted-foreground">
									Package all hosts, credentials, groups, snippets, and trusted keys into a secure
									AES-encrypted file.
								</p>
							</div>
						</div>

						<div class="flex items-center justify-between border-t border-border/50 pt-3">
							<span class="text-[11px] text-muted-foreground"
								>Password protected (.termust-backup)</span
							>
							<Button
								size="sm"
								class="h-8 gap-1.5 text-xs font-medium"
								onclick={(e) => {
									e.stopPropagation();
									activePanel = 'export';
								}}
							>
								<DownloadIcon class="size-3.5" />
								<span>Export</span>
							</Button>
						</div>
					</Card>

					<!-- Restore Card -->
					<Card
						role="button"
						tabindex={0}
						onclick={handleToggleRestore}
						onkeydown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') {
								e.preventDefault();
								handleToggleRestore();
							}
						}}
						aria-pressed={activePanel === 'restore'}
						class="cursor-pointer flex-col justify-between gap-4 p-4 transition-all duration-150 outline-none {activePanel ===
						'restore'
							? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
							: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
					>
						<div class="flex items-start gap-3.5">
							<div
								class="flex size-10 shrink-0 items-center justify-center rounded-full bg-amber-600 text-white shadow-xs dark:bg-amber-500"
								aria-hidden="true"
							>
								<UploadIcon class="size-5" />
							</div>
							<div class="flex flex-col gap-1">
								<h4 class="text-sm font-semibold text-foreground">Restore from Backup</h4>
								<p class="text-xs leading-relaxed text-muted-foreground">
									Replace the records in this vault with data from a previously created encrypted
									backup file.
								</p>
							</div>
						</div>

						<div class="flex items-center justify-between border-t border-border/50 pt-3">
							{#if hasLiveSessions}
								<span class="text-[11px] text-amber-500">Disconnect SSH sessions first</span>
							{:else}
								<span class="text-[11px] text-muted-foreground">Replaces existing vault data</span>
							{/if}
							<Button
								variant="outline"
								size="sm"
								class="h-8 gap-1.5 text-xs font-medium"
								onclick={(e) => {
									e.stopPropagation();
									if (!hasLiveSessions) activePanel = 'restore';
								}}
								disabled={hasLiveSessions}
							>
								<UploadIcon class="size-3.5" />
								<span>Restore</span>
							</Button>
						</div>
					</Card>
				</div>
			</div>
		{/if}
	</div>

	{#snippet panel()}
		{#if activePanel === 'export'}
			<BackupExportPanel {vaultStatus} {busy} onclose={() => (activePanel = null)} />
		{:else if activePanel === 'restore'}
			<BackupRestorePanel
				{vaultStatus}
				{hasLiveSessions}
				{busy}
				{onrestored}
				onclose={() => (activePanel = null)}
			/>
		{/if}
	{/snippet}
</ListDetail>
