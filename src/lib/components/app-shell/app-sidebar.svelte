<script lang="ts">
	import PanelLeftCloseIcon from '@lucide/svelte/icons/panel-left-close';
	import PanelLeftOpenIcon from '@lucide/svelte/icons/panel-left-open';
	import ServerIcon from '@lucide/svelte/icons/server';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import TerminalIcon from '@lucide/svelte/icons/terminal';

	import { Button } from '#lib/components/ui/button';
	import type { VaultStatus } from '#lib/vault';

	/**
	 * Desktop app sidebar navigation.
	 *
	 * Provides collapsible navigation between Hosts (Inventory) and Terminal workspaces,
	 * along with a passive vault status summary at the bottom.
	 */
	let {
		activeWorkspace = 'inventory',
		collapsed = false,
		sessionCount = 0,
		vaultStatus,
		ontogglecollapse,
		onnavigate
	}: {
		activeWorkspace?: 'inventory' | 'terminal';
		collapsed?: boolean;
		sessionCount?: number;
		vaultStatus?: VaultStatus;
		ontogglecollapse: () => void;
		onnavigate: (workspace: 'inventory' | 'terminal') => void;
	} = $props();
</script>

<aside
	class="flex shrink-0 flex-col border-r border-border bg-background transition-all duration-200 motion-reduce:transition-none"
	class:w-sidebar={!collapsed}
	class:w-sidebar-collapsed={collapsed}
>
	<!-- Sidebar header with collapse toggle -->
	<div
		class="flex h-10 items-center border-b border-border/50 px-2.5"
		class:justify-between={!collapsed}
		class:justify-center={collapsed}
	>
		{#if !collapsed}
			<span class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
				Termust
			</span>
		{/if}
		<Button
			variant="ghost"
			size="icon-xs"
			class="size-7 text-muted-foreground hover:text-foreground"
			onclick={ontogglecollapse}
			aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
			aria-expanded={!collapsed}
			title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
		>
			{#if collapsed}
				<PanelLeftOpenIcon class="size-4" aria-hidden="true" />
			{:else}
				<PanelLeftCloseIcon class="size-4" aria-hidden="true" />
			{/if}
		</Button>
	</div>

	<!-- Main navigation -->
	<nav aria-label="Main Navigation" class="flex flex-1 flex-col gap-1 p-2">
		<button
			type="button"
			class="flex items-center gap-2.5 rounded-md px-2.5 py-2 text-xs font-medium transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:outline-none"
			class:bg-accent={activeWorkspace === 'inventory'}
			class:text-accent-foreground={activeWorkspace === 'inventory'}
			class:text-muted-foreground={activeWorkspace !== 'inventory'}
			class:justify-center={collapsed}
			aria-current={activeWorkspace === 'inventory' ? 'page' : undefined}
			onclick={() => onnavigate('inventory')}
			title="Hosts inventory"
		>
			<ServerIcon class="size-4 shrink-0" aria-hidden="true" />
			<span class={collapsed ? 'sr-only' : 'truncate'}>Hosts</span>
		</button>

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
	</nav>

	<!-- Passive vault status display -->
	{#if vaultStatus}
		<div
			class="flex items-center border-t border-border/50 p-2 text-xs text-muted-foreground"
			class:justify-center={collapsed}
			class:gap-2={!collapsed}
			title={vaultStatus === 'unlocked'
				? 'Vault unlocked'
				: vaultStatus === 'locked'
					? 'Vault locked'
					: 'No vault initialized'}
		>
			{#if vaultStatus === 'unlocked'}
				<ShieldCheckIcon class="size-4 shrink-0 text-status-connected" aria-hidden="true" />
				<span class={collapsed ? 'sr-only' : 'truncate text-[11px]'}>Vault unlocked</span>
			{:else if vaultStatus === 'locked'}
				<ShieldAlertIcon class="size-4 shrink-0 text-status-connecting" aria-hidden="true" />
				<span class={collapsed ? 'sr-only' : 'truncate text-[11px]'}>Vault locked</span>
			{:else}
				<ShieldAlertIcon class="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
				<span class={collapsed ? 'sr-only' : 'truncate text-[11px]'}>No vault</span>
			{/if}
		</div>
	{/if}
</aside>
