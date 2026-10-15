<script lang="ts">
	import { Card, CardContent } from '#lib/components/ui/card';
	import type { Snippet } from 'svelte';

	/**
	 * The frame the inventory pages share: one card, one heading, an optional
	 * line under it, and an optional action on the right of the heading.
	 *
	 * A page brings only its own list, its dialogs and its actions, so a change
	 * to the shared chrome lands on every page at once. Hosts is the exception
	 * by design: it keeps its own toolbar and layout.
	 */
	let {
		title,
		description,
		actions,
		children
	}: {
		title: string;
		/** A line under the heading that says what the page holds. */
		description?: string;
		/** The page's main action, shown on the right of the heading. */
		actions?: Snippet;
		children?: Snippet;
	} = $props();
</script>

<Card>
	<CardContent class="grid gap-4">
		<div class="grid gap-3">
			<div class="flex items-center justify-between">
				<h3 class="text-sm font-medium">{title}</h3>
				{#if actions}
					{@render actions()}
				{/if}
			</div>
			{#if description}
				<p class="text-xs text-muted-foreground">{description}</p>
			{/if}
			{@render children?.()}
		</div>
	</CardContent>
</Card>
