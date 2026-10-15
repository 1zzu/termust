<script lang="ts">
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import { Card } from '#lib/components/ui/card';
	import { addedAt, type KnownHost } from '#lib/inventory';

	let {
		knownHost,
		selected = false,
		busy = false,
		onselect
	}: {
		knownHost: KnownHost;
		selected?: boolean;
		busy?: boolean;
		onselect: (knownHost: KnownHost) => void;
	} = $props();

	function handleClick() {
		onselect(knownHost);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onselect(knownHost);
		}
	}
</script>

<li class="list-none">
	<Card
		role="button"
		tabindex={0}
		onclick={handleClick}
		onkeydown={handleKeyDown}
		aria-pressed={selected}
		aria-label="{knownHost.hostKeyId}{selected ? ' (selected)' : ''}. Click to view details."
		title="Click to view details"
		class="cursor-pointer flex-row items-center gap-3 p-3 transition-all duration-150 outline-none {selected
			? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
			: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
	>
		<!-- Avatar bulat untuk Known Host -->
		<div
			class="flex size-10 shrink-0 items-center justify-center rounded-full bg-cyan-600 text-white shadow-xs dark:bg-cyan-500"
			aria-hidden="true"
		>
			<ShieldCheckIcon class="size-5" />
		</div>

		<!-- Info Known Host: Judul & Subtitle -->
		<div class="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
			<div class="flex items-center justify-between gap-2">
				<span class="truncate font-mono text-sm font-semibold text-foreground">
					{knownHost.hostKeyId}
				</span>
			</div>

			<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
				<span class="truncate font-mono text-[11px]">{knownHost.keyAlgorithm}</span>
				<span class="text-[11px]">•</span>
				<span class="truncate text-[11px]">{addedAt(knownHost.createdAt)}</span>
			</div>

			<!-- <span class="truncate font-mono text-[10px] text-muted-foreground/80">
				{knownHost.fingerprintSha256}
			</span> -->
		</div>
	</Card>
</li>
