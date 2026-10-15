<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * The inventory split: a middle column with a toolbar over its list, and a
	 * right column that is only there while something is open.
	 *
	 * The toolbar belongs to the middle column, so the right column starts at
	 * the same line as the toolbar and reaches the bottom of the workspace: it
	 * is a column beside the list, not a drawer over it, and it scrolls on its
	 * own. Below `lg` the two columns stack and the page scrolls as one, which
	 * is what a narrow window can afford.
	 */
	let {
		open = false,
		header,
		children,
		panel
	}: {
		/** Whether the right column takes space at all. */
		open?: boolean;
		/** The middle column's toolbar, above the list; it stays put. */
		header?: Snippet;
		/** The middle column's list, which scrolls under the toolbar at `lg`. */
		children?: Snippet;
		/** The right column: a side panel. Nothing is rendered without `open`. */
		panel?: Snippet;
	} = $props();
</script>

<div class="flex flex-col gap-4 lg:min-h-0 lg:flex-1 lg:flex-row lg:items-stretch">
	<div class="flex min-w-0 flex-col gap-4 lg:min-h-0 lg:flex-1">
		{#if header}
			<div class="shrink-0">{@render header()}</div>
		{/if}
		<div class="lg:min-h-0 lg:flex-1 lg:overflow-y-auto">
			{@render children?.()}
		</div>
	</div>

	{#if open && panel}
		{@render panel()}
	{/if}
</div>
