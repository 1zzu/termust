<script lang="ts">
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
		class="group flex cursor-pointer flex-col gap-2 rounded-lg border p-3 transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring {selected
			? 'border-primary bg-accent/40 ring-1 ring-primary'
			: 'border-border hover:border-muted-foreground/30 hover:bg-muted/30'}"
		onclick={handleClick}
		ondblclick={handleDblClick}
		onkeydown={handleKeyDown}
		aria-pressed={selected}
		aria-label="{host.label}{selected ? ' (selected)' : ''}. Double-click to connect."
		title="Click to view details, double-click to connect"
	>
		<div class="flex items-center justify-between gap-2">
			<span class="truncate text-sm font-semibold text-foreground">{host.label}</span>
			{#if connecting}
				<span class="font-mono text-[11px] text-muted-foreground">Connecting…</span>
			{/if}
		</div>

		<div class="flex flex-wrap items-center gap-1.5 text-xs">
			{#if groupName}
				<span
					class="inline-flex items-center rounded border border-border/60 bg-muted/60 px-1.5 py-0.5 text-[11px] text-muted-foreground"
				>
					{groupName}
				</span>
			{/if}
			{#if credentialName}
				<span
					class="inline-flex items-center rounded border border-border/60 bg-muted/60 px-1.5 py-0.5 text-[11px] text-muted-foreground"
				>
					{credentialName}
				</span>
			{/if}
			{#if hint}
				<span
					class="inline-flex items-center rounded border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-[11px] font-medium text-amber-500"
				>
					{hint}
				</span>
			{/if}
		</div>
	</div>
</li>
