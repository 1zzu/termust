<script lang="ts">
	import ArchiveRestoreIcon from '@lucide/svelte/icons/archive-restore';
	import CodeXmlIcon from '@lucide/svelte/icons/code-xml';
	import FingerprintIcon from '@lucide/svelte/icons/fingerprint';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import ServerIcon from '@lucide/svelte/icons/server';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import TerminalIcon from '@lucide/svelte/icons/terminal';

	import { Button } from '#lib/components/ui/button';
	import type { InventoryCounts, InventorySection } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';

	/**
	 * Desktop app sidebar navigation.
	 *
	 * Provides collapsible navigation between Terminal and Inventory sections
	 * (Hosts, Credentials, Groups, Known hosts, Snippets, Backup),
	 * along with vault status and interactive actions at the bottom.
	 */
	let {
		activeWorkspace = 'inventory',
		activeSection = 'hosts',
		collapsed = false,
		sessionCount = 0,
		vaultStatus,
		inventoryCounts = {
			hosts: 0,
			credentials: 0,
			groups: 0,
			knownHosts: 0,
			snippets: 0
		},
		onnavigate,
		onlockvault,
		onopenvault
	}: {
		activeWorkspace?: 'inventory' | 'terminal';
		activeSection?: InventorySection;
		collapsed?: boolean;
		sessionCount?: number;
		vaultStatus?: VaultStatus;
		inventoryCounts?: InventoryCounts;
		onnavigate: (workspace: 'inventory' | 'terminal', section?: InventorySection) => void;
		onlockvault?: () => void;
		onopenvault?: () => void;
	} = $props();

	interface InventoryMenuItem {
		id: InventorySection;
		label: string;
		icon: typeof ServerIcon;
		count?: number;
		requiresUnlock: boolean;
	}

	const inventoryMenuItems = $derived<InventoryMenuItem[]>([
		{
			id: 'hosts',
			label: 'Hosts',
			icon: ServerIcon,
			count: inventoryCounts.hosts,
			requiresUnlock: false
		},
		{
			id: 'credentials',
			label: 'Credentials',
			icon: KeyRoundIcon,
			count: inventoryCounts.credentials,
			requiresUnlock: true
		},
		{
			id: 'groups',
			label: 'Groups',
			icon: FolderIcon,
			count: inventoryCounts.groups,
			requiresUnlock: true
		},
		{
			id: 'known_hosts',
			label: 'Known hosts',
			icon: FingerprintIcon,
			count: inventoryCounts.knownHosts,
			requiresUnlock: true
		},
		{
			id: 'snippets',
			label: 'Snippets',
			icon: CodeXmlIcon,
			count: inventoryCounts.snippets,
			requiresUnlock: true
		},
		{
			id: 'backup',
			label: 'Backup & Restore',
			icon: ArchiveRestoreIcon,
			requiresUnlock: true
		}
	]);
</script>

<aside
	class="flex shrink-0 flex-col border-r border-border bg-background transition-all duration-200 motion-reduce:transition-none"
	class:w-sidebar={!collapsed}
	class:w-sidebar-collapsed={collapsed}
>
	<!-- Main navigation -->
	<nav aria-label="Main Navigation" class="flex flex-1 flex-col gap-1 overflow-y-auto p-2">
		<button
			type="button"
			class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-xs font-medium transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:outline-none"
			class:bg-accent={activeWorkspace === 'terminal'}
			class:text-accent-foreground={activeWorkspace === 'terminal'}
			class:text-muted-foreground={activeWorkspace !== 'terminal'}
			class:justify-center={collapsed}
			aria-current={activeWorkspace === 'terminal' ? 'page' : undefined}
			onclick={() => onnavigate('terminal')}
			title="Terminal workspace"
		>
			<TerminalIcon class="size-4 shrink-0" aria-hidden="true" />
			<span class={collapsed ? 'sr-only' : 'truncate'}>Terminal</span>
			{#if sessionCount > 0}
				{#if collapsed}
					<span class="sr-only">({sessionCount} active sessions)</span>
				{:else}
					<span
						class="ml-auto rounded-full bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
					>
						{sessionCount}
					</span>
				{/if}
			{/if}
		</button>

		<div class="my-1 border-t border-border/50" aria-hidden="true"></div>

		<!-- {#if !collapsed}
			<div
				class="px-2.5 py-1 text-[10px] font-semibold tracking-wider text-muted-foreground uppercase"
			>
				Inventory
			</div>
		{/if} -->

		{#each inventoryMenuItems as item (item.id)}
			{@const isLocked = item.requiresUnlock && vaultStatus !== 'unlocked'}
			{@const isActive = activeWorkspace === 'inventory' && activeSection === item.id}
			{@const ItemIcon = item.icon}
			<button
				type="button"
				class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-xs font-medium transition-colors focus-visible:ring-2 focus-visible:outline-none"
				class:bg-accent={isActive}
				class:text-accent-foreground={isActive}
				class:text-muted-foreground={!isActive && !isLocked}
				class:hover:bg-accent={!isLocked}
				class:hover:text-accent-foreground={!isLocked}
				class:opacity-50={isLocked}
				class:cursor-not-allowed={isLocked}
				class:justify-center={collapsed}
				aria-current={isActive ? 'page' : undefined}
				disabled={isLocked}
				onclick={() => onnavigate('inventory', item.id)}
				title={isLocked
					? `${item.label} (Vault locked)`
					: item.count !== undefined
						? `${item.label} (${item.count})`
						: item.label}
			>
				<ItemIcon class="size-4 shrink-0" aria-hidden="true" />
				<span class={collapsed ? 'sr-only' : 'truncate'}>{item.label}</span>
				{#if item.count !== undefined && !isLocked}
					{#if collapsed}
						<span class="sr-only">({item.count})</span>
					{:else}
						<span
							class="ml-auto rounded-full bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
						>
							{item.count}
						</span>
					{/if}
				{/if}
			</button>
		{/each}
	</nav>

	<!-- Interactive vault status and actions -->
	{#if !collapsed}
		<div
			class="flex items-center justify-between border-t border-border/50 p-2 text-xs text-muted-foreground"
		>
			{#if vaultStatus === 'unlocked'}
				<div class="flex min-w-0 items-center gap-1.5" title="Vault unlocked">
					<ShieldCheckIcon class="size-4 shrink-0 text-status-connected" aria-hidden="true" />
					<span class="truncate text-[11px] font-medium text-foreground">Vault unlocked</span>
				</div>
				<Button
					variant="ghost"
					size="sm"
					class="h-6 px-2 text-[11px] font-medium text-muted-foreground hover:text-foreground"
					onclick={onlockvault}
					title="Lock vault"
					aria-label="Lock vault"
				>
					Lock
				</Button>
			{:else if vaultStatus === 'locked'}
				<div class="flex min-w-0 items-center gap-1.5" title="Vault locked">
					<ShieldAlertIcon class="size-4 shrink-0 text-status-connecting" aria-hidden="true" />
					<span class="truncate text-[11px] font-medium text-foreground">Vault locked</span>
				</div>
				<Button
					variant="outline"
					size="sm"
					class="h-6 px-2 text-[11px] font-medium"
					onclick={onopenvault}
					title="Unlock vault"
					aria-label="Unlock vault"
				>
					Unlock
				</Button>
			{:else if vaultStatus === 'uninitialized'}
				<div class="flex min-w-0 items-center gap-1.5" title="No vault">
					<ShieldAlertIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
					<span class="truncate text-[11px] font-medium text-foreground">No vault</span>
				</div>
				<Button
					variant="outline"
					size="sm"
					class="h-6 px-2 text-[11px] font-medium"
					onclick={onopenvault}
					title="Create vault"
					aria-label="Create vault"
				>
					Create
				</Button>
			{:else}
				<div class="flex min-w-0 items-center gap-1.5" title="Checking vault status">
					<Loader2Icon
						class="size-4 shrink-0 animate-spin text-muted-foreground"
						aria-hidden="true"
					/>
					<span class="truncate text-[11px]">Checking…</span>
				</div>
				<Button
					variant="ghost"
					size="sm"
					class="h-6 px-2 text-[11px] text-muted-foreground"
					disabled
					title="Checking vault status"
					aria-label="Checking vault status"
				>
					Check
				</Button>
			{/if}
		</div>
	{:else}
		<div
			class="flex items-center justify-center border-t border-border/50 p-2 text-xs text-muted-foreground"
		>
			{#if vaultStatus === 'unlocked'}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-status-connected hover:text-foreground"
					onclick={onlockvault}
					aria-label="Lock vault"
					title="Lock vault"
				>
					<ShieldCheckIcon class="size-4" aria-hidden="true" />
				</Button>
			{:else if vaultStatus === 'locked'}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-status-connecting hover:text-foreground"
					onclick={onopenvault}
					aria-label="Unlock vault"
					title="Unlock vault"
				>
					<ShieldAlertIcon class="size-4" aria-hidden="true" />
				</Button>
			{:else if vaultStatus === 'uninitialized'}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-muted-foreground hover:text-foreground"
					onclick={onopenvault}
					aria-label="Create vault"
					title="Create vault"
				>
					<ShieldAlertIcon class="size-4" aria-hidden="true" />
				</Button>
			{:else}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-muted-foreground"
					disabled
					aria-label="Checking vault status"
					title="Checking vault status"
				>
					<Loader2Icon class="size-4 animate-spin" aria-hidden="true" />
				</Button>
			{/if}
		</div>
	{/if}
</aside>
