<script lang="ts">
	import { Button } from '#lib/components/ui/button';
	import { Card } from '#lib/components/ui/card';
	import type { Host } from '#lib/inventory';
	import { canConnectHost, hostConnectionHint } from '#lib/inventory';
	import { Cpu } from '@lucide/svelte';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import { canTriggerDoubleClickConnect } from './context-state';

	let {
		host,
		groupName = '',
		credentialName = '',
		selected = false,
		busy = false,
		connecting = false,
		onselect,
		onedit,
		onconnect
	}: {
		host: Host;
		groupName?: string;
		credentialName?: string;
		selected?: boolean;
		busy?: boolean;
		connecting?: boolean;
		onselect: (host: Host) => void;
		onedit?: (host: Host) => void;
		onconnect: (host: Host) => void;
	} = $props();

	const connectable = $derived(canConnectHost(host));
	const hint = $derived(hostConnectionHint(host));

	const subtitle = $derived.by(() => {
		const parts: string[] = ['ssh'];
		if (host.username) {
			parts.push(host.username);
		} else if (groupName) {
			parts.push(groupName);
		}
		return parts.join(', ');
	});

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
	<Card
		role="button"
		tabindex={0}
		onclick={handleClick}
		ondblclick={handleDblClick}
		onkeydown={handleKeyDown}
		aria-pressed={selected}
		aria-label="{host.label}{selected ? ' (selected)' : ''}. Double-click to connect."
		title="Click to select, double-click to connect"
		class="group cursor-pointer flex-row items-center gap-3 p-3 transition-all duration-150 outline-none {selected
			? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
			: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
	>
		<!-- Avatar OS bulat oranye di kiri -->
		<div
			class="flex size-10 shrink-0 items-center justify-center rounded-full bg-[#E95420] text-white shadow-xs"
			aria-hidden="true"
		>
			<Cpu class="size-5" />
			<!-- <svg viewBox="0 0 24 24" class="size-5 fill-current">
				<circle cx="12" cy="12" r="10" fill="#E95420" />
				<path
					fill="#ffffff"
					d="M12 4.5a7.5 7.5 0 0 0-7.3 5.8 2 2 0 0 1 .8.4 2 2 0 0 1 .7 1.3 7.5 7.5 0 0 0 0 0 2 2 0 0 1-.7 1.3 2 2 0 0 1-.8.4A7.5 7.5 0 1 0 12 4.5zm0 1.8a5.7 5.7 0 0 1 4.5 2.2 2 2 0 0 1-.9.7 2 2 0 0 1-.3 1.5 5.7 5.7 0 0 1 0 2.6 2 2 0 0 1 .3 1.5 2 2 0 0 1 .9.7 5.7 5.7 0 0 1-9 0 2 2 0 0 1 .9-.7 2 2 0 0 1 .3-1.5 5.7 5.7 0 0 1 0-2.6 2 2 0 0 1-.3-1.5 2 2 0 0 1-.9-.7A5.7 5.7 0 0 1 12 6.3z"
				/>
			</svg> -->
		</div>

		<!-- Info Host: Judul & Subtitle -->
		<div class="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
			<div class="flex items-center justify-between gap-2">
				<span class="truncate text-sm font-semibold text-foreground">{host.label}</span>
				{#if connecting}
					<span class="font-mono text-[11px] text-muted-foreground">Connecting…</span>
				{/if}
			</div>

			<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
				<span class="truncate text-[11px]">{subtitle}</span>
				{#if hint}
					<span
						class="py-0.2 inline-flex items-center rounded border border-amber-500/30 bg-amber-500/10 px-1 text-[9px] font-medium text-amber-500"
					>
						{hint}
					</span>
				{/if}
			</div>
		</div>

		<!-- Tombol Edit di sisi kanan card (tampil saat hover) -->
		<div class="flex shrink-0 items-center">
			<Button
				variant="ghost"
				size="icon-xs"
				class="size-7 rounded-md text-muted-foreground opacity-0 transition-opacity duration-150 hover:bg-muted/80 hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
				title="Edit host"
				aria-label={`Edit ${host.label}`}
				onclick={(e) => {
					e.stopPropagation();
					onedit?.(host);
				}}
			>
				<PencilIcon class="size-3.5" aria-hidden="true" />
			</Button>
		</div>
	</Card>
</li>
