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

	import * as Sidebar from '#lib/components/ui/sidebar';
	import { Button } from '#lib/components/ui/button';
	import { page } from '$app/state';
	import { vaultStore } from '#lib/stores/vault.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';

	interface MenuItem {
		href: string;
		label: string;
		icon: typeof ServerIcon;
		count?: number;
		requiresUnlock: boolean;
	}

	const inventoryItems = $derived<MenuItem[]>([
		{
			href: '/hosts',
			label: 'Hosts',
			icon: ServerIcon,
			count: inventoryStore.counts.hosts,
			requiresUnlock: false
		},
		{
			href: '/credentials',
			label: 'Credentials',
			icon: KeyRoundIcon,
			count: inventoryStore.counts.credentials,
			requiresUnlock: true
		},
		{
			href: '/groups',
			label: 'Groups',
			icon: FolderIcon,
			count: inventoryStore.counts.groups,
			requiresUnlock: true
		},
		{
			href: '/known-hosts',
			label: 'Known hosts',
			icon: FingerprintIcon,
			count: inventoryStore.counts.knownHosts,
			requiresUnlock: true
		},
		{
			href: '/snippets',
			label: 'Snippets',
			icon: CodeXmlIcon,
			count: inventoryStore.counts.snippets,
			requiresUnlock: true
		},
		{
			href: '/backup',
			label: 'Backup & Restore',
			icon: ArchiveRestoreIcon,
			requiresUnlock: true
		}
	]);

	function isItemActive(href: string, currentPath: string): boolean {
		if (href === '/hosts') {
			return currentPath === '/hosts' || currentPath === '/';
		}
		return currentPath === href || currentPath.startsWith(href + '/');
	}
</script>

<Sidebar.Root collapsible="icon">
	<!-- Safe area for macOS traffic lights: completely empty, no content, draggable -->
	<Sidebar.Header
		class="h-titlebar border-b border-border/50 p-0 select-none"
		data-tauri-drag-region
	>
		<div class="h-full w-full" data-tauri-drag-region aria-hidden="true"></div>
	</Sidebar.Header>

	<Sidebar.Content>
		<!-- Terminal Workspace Item -->
		<Sidebar.Group class="py-1">
			<Sidebar.Menu>
				{@const isTerminalActive = page.url.pathname === '/terminal'}
				{@const sessionCount = terminalStore.sessions.length}
				<Sidebar.MenuItem>
					<Sidebar.MenuButton
						isActive={isTerminalActive}
						tooltipContent="Terminal workspace"
						size="sm"
					>
						{#snippet child({ props })}
							<a href="/terminal" {...props} aria-current={isTerminalActive ? 'page' : undefined}>
								<TerminalIcon class="size-4 shrink-0" aria-hidden="true" />
								<span>Terminal</span>
							</a>
						{/snippet}
					</Sidebar.MenuButton>
					{#if sessionCount > 0}
						<Sidebar.MenuBadge class="font-mono text-[10px]">
							{sessionCount}
						</Sidebar.MenuBadge>
					{/if}
				</Sidebar.MenuItem>
			</Sidebar.Menu>
		</Sidebar.Group>

		<Sidebar.Separator />

		<!-- Inventory Workspace Navigation Group -->
		<Sidebar.Group class="py-1">
			<Sidebar.GroupLabel class="text-[10px] font-semibold tracking-wider uppercase">
				Inventory
			</Sidebar.GroupLabel>
			<Sidebar.GroupContent>
				<Sidebar.Menu>
					{#each inventoryItems as item (item.href)}
						{@const isLocked = item.requiresUnlock && vaultStore.status !== 'unlocked'}
						{@const isActive = isItemActive(item.href, page.url.pathname)}
						{@const ItemIcon = item.icon}
						<Sidebar.MenuItem>
							{#if isLocked}
								<Sidebar.MenuButton
									isActive={false}
									tooltipContent={`${item.label} (Vault locked)`}
									size="sm"
									class="cursor-not-allowed opacity-50"
									aria-disabled={true}
								>
									<ItemIcon class="size-4 shrink-0" aria-hidden="true" />
									<span>{item.label}</span>
								</Sidebar.MenuButton>
							{:else}
								<Sidebar.MenuButton
									{isActive}
									tooltipContent={item.count !== undefined
										? `${item.label} (${item.count})`
										: item.label}
									size="sm"
								>
									{#snippet child({ props })}
										<a
											href={item.href}
											{...props}
											aria-current={isActive ? 'page' : undefined}
										>
											<ItemIcon class="size-4 shrink-0" aria-hidden="true" />
											<span>{item.label}</span>
										</a>
									{/snippet}
								</Sidebar.MenuButton>
								{#if item.count !== undefined}
									<Sidebar.MenuBadge class="font-mono text-[10px]">
										{item.count}
									</Sidebar.MenuBadge>
								{/if}
							{/if}
						</Sidebar.MenuItem>
					{/each}
				</Sidebar.Menu>
			</Sidebar.GroupContent>
		</Sidebar.Group>
	</Sidebar.Content>

	<!-- Vault Status and Actions Footer -->
	<Sidebar.Footer class="border-t border-border/50 p-2">
		<!-- Expanded layout view -->
		<div class="flex items-center justify-between gap-1 group-data-[collapsible=icon]:hidden">
			{#if vaultStore.status === 'unlocked'}
				<div class="flex min-w-0 items-center gap-1.5" title="Vault unlocked">
					<ShieldCheckIcon class="size-4 shrink-0 text-status-connected" aria-hidden="true" />
					<span class="truncate text-[11px] font-medium text-foreground">Vault unlocked</span>
				</div>
				<Button
					variant="ghost"
					size="sm"
					class="h-6 px-2 text-[11px] font-medium text-muted-foreground hover:text-foreground"
					onclick={() => void vaultStore.lock()}
					title="Lock vault"
					aria-label="Lock vault"
				>
					Lock
				</Button>
			{:else if vaultStore.status === 'locked'}
				<div class="flex min-w-0 items-center gap-1.5" title="Vault locked">
					<ShieldAlertIcon class="size-4 shrink-0 text-status-connecting" aria-hidden="true" />
					<span class="truncate text-[11px] font-medium text-foreground">Vault locked</span>
				</div>
				<Button
					variant="outline"
					size="sm"
					class="h-6 px-2 text-[11px] font-medium"
					onclick={() => vaultStore.openGate()}
					title="Unlock vault"
					aria-label="Unlock vault"
				>
					Unlock
				</Button>
			{:else if vaultStore.status === 'uninitialized'}
				<div class="flex min-w-0 items-center gap-1.5" title="No vault">
					<ShieldAlertIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
					<span class="truncate text-[11px] font-medium text-foreground">No vault</span>
				</div>
				<Button
					variant="outline"
					size="sm"
					class="h-6 px-2 text-[11px] font-medium"
					onclick={() => vaultStore.openGate()}
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
					<span class="truncate text-[11px] text-muted-foreground">Checking...</span>
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

		<!-- Collapsed icon layout view -->
		<div class="hidden items-center justify-center group-data-[collapsible=icon]:flex">
			{#if vaultStore.status === 'unlocked'}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-status-connected hover:text-foreground"
					onclick={() => void vaultStore.lock()}
					aria-label="Lock vault"
					title="Lock vault"
				>
					<ShieldCheckIcon class="size-4" aria-hidden="true" />
				</Button>
			{:else if vaultStore.status === 'locked'}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-status-connecting hover:text-foreground"
					onclick={() => vaultStore.openGate()}
					aria-label="Unlock vault"
					title="Unlock vault"
				>
					<ShieldAlertIcon class="size-4" aria-hidden="true" />
				</Button>
			{:else if vaultStore.status === 'uninitialized'}
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-7 text-muted-foreground hover:text-foreground"
					onclick={() => vaultStore.openGate()}
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
	</Sidebar.Footer>
	<Sidebar.Rail />
</Sidebar.Root>
