<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * Desktop application shell root.
	 *
	 * Provides the full-window frame with no outer scroll:
	 * native title bar at the top, collapsible sidebar on the left,
	 * and the active workspace filling the remaining viewport.
	 */
	let {
		titlebar,
		sidebar,
		children
	}: {
		titlebar?: Snippet;
		sidebar?: Snippet;
		children?: Snippet;
	} = $props();
</script>

<div
	class="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground select-none"
>
	{#if titlebar}
		{@render titlebar()}
	{/if}
	<div class="flex min-h-0 flex-1 overflow-hidden">
		{#if sidebar}
			{@render sidebar()}
		{/if}
		<main class="relative min-w-0 flex-1 overflow-hidden select-text">
			{#if children}
				{@render children()}
			{/if}
		</main>
	</div>
</div>
