<script lang="ts">
	import ServerIcon from '@lucide/svelte/icons/server';
	import type { Host } from '#lib/inventory';
	import { canConnectHost, hostConnectionHint } from '#lib/inventory';
	import { canTriggerDoubleClickConnect } from './context-state';

	let {
		host,
		groupName = '',
		credentialName = '',
		selected = false,
		busy = false,
		connecting = false,
		onselect,
		onconnect
	}: {
		host: Host;
		groupName?: string;
		credentialName?: string;
		selected?: boolean;
		busy?: boolean;
		connecting?: boolean;
		onselect: (host: Host) => void;
		onconnect: (host: Host) => void;
	} = $props();

	let contentHeight = $state(0);

	const connectable = $derived(canConnectHost(host));
	const hint = $derived(hostConnectionHint(host));

	function handleClick() {
		onselect(host);
	}

	function handleDblClick() {
		onselect(host);
		if (canTriggerDoubleClickConnect(busy, connecting, connectable)) {
			onconnect(host);
		}
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onselect(host);
		}
	}
</script>

<li class="list-none">
	<div
		role="button"
		tabindex="0"
		class="group flex cursor-pointer items-center gap-2.5 rounded-lg border p-2.5 transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring {selected
			? 'border-primary bg-accent/40 ring-1 ring-primary'
			: 'border-border hover:border-muted-foreground/30 hover:bg-muted/30'}"
		onclick={handleClick}
		ondblclick={handleDblClick}
		onkeydown={handleKeyDown}
		aria-pressed={selected}
		aria-label="{host.label}{selected ? ' (selected)' : ''}. Double-click to connect."
		title="Click to view details, double-click to connect"
	>
		<!-- Avatar icon di kiri sendiri (square 1:1) -->
		<div
			class="flex shrink-0 items-center justify-center rounded-md border transition-colors {selected
				? 'border-primary/40 bg-primary/10 text-primary'
				: 'border-border/70 bg-muted/40 text-muted-foreground group-hover:border-border group-hover:bg-muted/70 group-hover:text-foreground'}"
			style={contentHeight > 0
				? `width: ${contentHeight}px; height: ${contentHeight}px;`
				: 'width: 2.75rem; height: 2.75rem;'}
		>
			<ServerIcon class="size-5" aria-hidden="true" />
		</div>

		<!-- Flex col isi label dan groupname etc -->
		<div bind:clientHeight={contentHeight} class="flex min-w-0 flex-1 flex-col gap-1">
			<div class="flex items-center justify-between gap-2">
				<span class="truncate text-sm font-semibold text-foreground">{host.label}</span>
				{#if connecting}
					<span class="font-mono text-[11px] text-muted-foreground">Connecting…</span>
				{/if}
			</div>

			<div class="flex flex-wrap items-center gap-1.5 text-xs">
				{#if groupName}
					<span
						class="py-0.2 py-0.1text-[10px] inline-flex items-center rounded border border-border/60 bg-muted/60 px-1 text-[9px] text-muted-foreground"
					>
						{groupName}
					</span>
				{/if}
				{#if credentialName}
					<span
						class="py-0.2 inline-flex items-center rounded border border-border/60 bg-muted/60 px-1 text-[9px] text-muted-foreground"
					>
						{credentialName}
					</span>
				{/if}
				{#if hint}
					<span
						class="py-0.2 py-0.1 inline-flex items-center rounded border border-amber-500/30 bg-amber-500/10 px-1 text-[9px] font-medium text-amber-500"
					>
						{hint}
					</span>
				{/if}
			</div>
		</div>
	</div>
</li>
