<script lang="ts">
	import FolderIcon from '@lucide/svelte/icons/folder';
	import ServerIcon from '@lucide/svelte/icons/server';
	import { Card } from '#lib/components/ui/card';
	import type { Group, Host } from '#lib/inventory';

	let {
		group,
		hosts = [],
		selected = false,
		busy = false,
		onselect
	}: {
		group: Group;
		hosts?: Host[];
		selected?: boolean;
		busy?: boolean;
		onselect: (group: Group) => void;
	} = $props();

	const linkedHosts = $derived(hosts.filter((h) => h.groupId === group.id));

	const subtitle = $derived(
		linkedHosts.length > 0
			? `${linkedHosts.length} ${linkedHosts.length === 1 ? 'host' : 'hosts'}`
			: 'Empty group'
	);

	function handleClick() {
		onselect(group);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onselect(group);
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
		aria-label="{group.name}{selected ? ' (selected)' : ''}. Click to view details."
		title="Click to view details"
		class="cursor-pointer flex-row items-center gap-3 p-3 transition-all duration-150 outline-none {selected
			? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
			: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
	>
		<!-- Avatar bulat untuk Group -->
		<div
			class="flex size-10 shrink-0 items-center justify-center rounded-full bg-amber-600 text-white shadow-xs dark:bg-amber-500"
			aria-hidden="true"
		>
			<FolderIcon class="size-5" />
		</div>

		<!-- Info Group: Judul & Subtitle -->
		<div class="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
			<div class="flex items-center justify-between gap-2">
				<span class="truncate text-sm font-semibold text-foreground">{group.name}</span>
			</div>

			<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
				<span class="truncate text-[11px]">{subtitle}</span>
				{#if linkedHosts.length > 0}
					<span
						class="py-0.2 inline-flex items-center gap-1 rounded border border-border/70 bg-muted/50 px-1.5 text-[9px] font-medium text-foreground/80"
					>
						<ServerIcon class="size-2.5" />
						{linkedHosts.length}
					</span>
				{/if}
			</div>
		</div>
	</Card>
</li>
