<script lang="ts">
	import type { Host } from '#lib/inventory';
	import { canConnectHost, hostConnectionHint } from '#lib/inventory';
	import { Button } from '#lib/components/ui/button';

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
</script>

<li
	class="group relative flex flex-col justify-between rounded-lg border bg-card p-3.5 transition-colors {selected
		? 'border-primary bg-accent/40 ring-1 ring-primary'
		: 'border-border hover:border-muted-foreground/30 hover:bg-muted/30'}"
>
	<button
		type="button"
		class="flex w-full flex-1 flex-col items-start gap-1.5 rounded-md text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
		onclick={() => onselect(host)}
		aria-pressed={selected}
		aria-label="Select {host.label}"
	>
		<div class="flex w-full items-center justify-between gap-2">
			<span class="truncate text-sm font-semibold text-foreground">{host.label}</span>
			{#if selected}
				<span class="shrink-0 text-[10px] font-medium tracking-wider text-primary uppercase"
					>Selected</span
				>
			{/if}
		</div>

		<span class="truncate font-mono text-xs text-muted-foreground">
			{host.username}@{host.address}:{host.port}
		</span>

		<div class="flex flex-wrap items-center gap-1.5 pt-1 text-xs">
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
	</button>

	<div class="mt-3 flex items-center justify-end border-t border-border/40 pt-2.5">
		<Button
			size="sm"
			class="h-7 text-xs"
			disabled={busy || connecting || !connectable}
			onclick={(event) => {
				event.stopPropagation();
				onconnect(host);
			}}
			aria-label="Connect to {host.label}"
		>
			{connecting ? 'Connecting…' : 'Connect'}
		</Button>
	</div>
</li>
