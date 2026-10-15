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

	import * as Sidebar from '#lib/components/ui/sidebar';
	import { Button } from '#lib/components/ui/button';
	import { page } from '$app/state';
	import { vaultStore } from '#lib/stores/vault.svelte';
	import { LockIcon } from '@lucide/svelte';
	import VaultSelector from '#lib/components/app-shell/vault-selector.svelte';

	interface MenuItem {
		href: string;
		label: string;
		icon: typeof ServerIcon;
		requiresUnlock: boolean;
	}

	const inventoryItems: MenuItem[] = [
		{
			href: '/hosts',
			label: 'Hosts',
			icon: ServerIcon,
			requiresUnlock: false
		},
		{
			href: '/credentials',
			label: 'Credentials',
			icon: KeyRoundIcon,
			requiresUnlock: true
		},
		{
			href: '/groups',
			label: 'Groups',
			icon: FolderIcon,
			requiresUnlock: true
		},
		{
			href: '/known-hosts',
			label: 'Known hosts',
			icon: FingerprintIcon,
			requiresUnlock: true
		},
		{
			href: '/snippets',
			label: 'Snippets',
			icon: CodeXmlIcon,
			requiresUnlock: true
		},
		{
			href: '/backup',
			label: 'Backup & Restore',
			icon: ArchiveRestoreIcon,
			requiresUnlock: true
		}
	];

	function isItemActive(href: string, currentPath: string): boolean {
		if (href === '/hosts') {
			return currentPath === '/hosts' || currentPath === '/';
		}
		return currentPath === href || currentPath.startsWith(href + '/');
	}
</script>

<Sidebar.Root collapsible="none">
	<!-- Safe area for macOS traffic lights with integrated Vault Selector -->
	<Sidebar.Header
		class="flex h-titlebar flex-row items-center border-b border-border p-0 px-2 select-none"
		data-tauri-drag-region
	>
		<!-- Non-interactive drag spacer matching macOS traffic light bounds (70px) -->
		<div class="h-full w-17.5 shrink-0" data-tauri-drag-region aria-hidden="true"></div>

		<!-- Interactive Vault Selector dropdown -->
		<div class="flex min-w-0 items-center">
			<VaultSelector />
		</div>

		<!-- Trailing drag region -->
		<div class="h-full min-w-2 flex-1" data-tauri-drag-region aria-hidden="true"></div>
	</Sidebar.Header>

	<Sidebar.Content class="gap-1.5 p-2">
		<!-- Terminal Workspace Item -->
		<Sidebar.Group class="p-0">
			<Sidebar.Menu class="gap-1">
				{@const isTerminalActive = page.url.pathname === '/terminal'}
				<Sidebar.MenuItem>
					<Sidebar.MenuButton
						isActive={isTerminalActive}
						tooltipContent="Terminal workspace"
						size="default"
					>
						{#snippet child({ props })}
							<a href="/terminal" {...props} aria-current={isTerminalActive ? 'page' : undefined}>
								<!-- <img
									src="/512x512.png"
									alt="Termust"
									class="size-6 shrink-0 rounded-xs object-contain"
									aria-hidden="true"
								/> -->
								<span>Termust</span>
							</a>
						{/snippet}
					</Sidebar.MenuButton>
				</Sidebar.MenuItem>
			</Sidebar.Menu>
		</Sidebar.Group>

		<!-- Inventory Workspace Navigation Group -->
		<Sidebar.Group class="p-0">
			<Sidebar.GroupLabel
				class="px-3 text-[11px] font-semibold tracking-wider text-muted-foreground/70 uppercase"
			>
				Manage
			</Sidebar.GroupLabel>
			<Sidebar.GroupContent>
				<Sidebar.Menu class="gap-1">
					{#each inventoryItems as item (item.href)}
						{@const isLocked = item.requiresUnlock && vaultStore.status !== 'unlocked'}
						{@const isActive = isItemActive(item.href, page.url.pathname)}
						{@const ItemIcon = item.icon}
						<Sidebar.MenuItem>
							{#if isLocked}
								<Sidebar.MenuButton
									isActive={false}
									tooltipContent={`${item.label} (Vault locked)`}
									size="default"
									class="cursor-not-allowed opacity-50"
									aria-disabled={true}
								>
									<ItemIcon class="size-4 shrink-0" aria-hidden="true" />
									<span>{item.label}</span>
								</Sidebar.MenuButton>
							{:else}
								<Sidebar.MenuButton {isActive} tooltipContent={item.label} size="default">
									{#snippet child({ props })}
										<a href={item.href} {...props} aria-current={isActive ? 'page' : undefined}>
											<ItemIcon class="size-4 shrink-0" aria-hidden="true" />
											<span class="text-[13px]">{item.label}</span>
										</a>
									{/snippet}
								</Sidebar.MenuButton>
							{/if}
						</Sidebar.MenuItem>
					{/each}
				</Sidebar.Menu>
			</Sidebar.GroupContent>
		</Sidebar.Group>
	</Sidebar.Content>

	<!-- Vault Status and Actions Footer -->
	<Sidebar.Footer class="border-t border-border p-2">
		{#if vaultStore.status === 'unlocked'}
			<div
				class="flex h-9 w-full items-center justify-between rounded-lg px-2.5 text-[13px] transition-colors hover:bg-sidebar-accent/40"
			>
				<div class="flex min-w-0 items-center gap-2.5">
					<ShieldCheckIcon class="size-4 shrink-0 text-status-connected" aria-hidden="true" />
					<span class="truncate font-medium text-foreground">Vault unlocked</span>
				</div>
				<Button
					variant="ghost"
					size="icon-xs"
					class="size-6 text-muted-foreground hover:text-foreground"
					onclick={() => void vaultStore.lock()}
					title="Lock vault"
					aria-label="Lock vault"
				>
					<LockIcon class="size-3.5" aria-hidden="true" />
				</Button>
			</div>
		{:else if vaultStore.status === 'locked'}
			<button
				type="button"
				class="flex h-9 w-full items-center justify-between rounded-lg bg-sidebar-accent/30 px-2.5 text-[13px] text-sidebar-foreground transition-colors hover:bg-sidebar-accent/60"
				onclick={() => vaultStore.openGate()}
				title="Click to unlock vault"
			>
				<div class="flex min-w-0 items-center gap-2.5">
					<ShieldAlertIcon class="size-4 shrink-0 text-status-connecting" aria-hidden="true" />
					<span class="truncate font-medium">Unlock vault</span>
				</div>
				<span
					class="rounded bg-background/50 px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground"
					>Locked</span
				>
			</button>
		{/if}
	</Sidebar.Footer>
</Sidebar.Root>
