<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';
	import type { Snippet } from 'svelte';
	import { Button } from '#lib/components/ui/button';

	/**
	 * The right column itself: header, own scrolling body, footer close to the
	 * bottom. Every form and detail view in the inventory uses it so the width,
	 * the Escape key and the landmark are not repeated per page.
	 *
	 * The panel owns no state. Its content is unmounted when the caller stops
	 * rendering it — that is what clears half-typed secrets when a form closes.
	 */
	let {
		label,
		kicker,
		title,
		size = 'default',
		onclose,
		children,
		footer
	}: {
		/** Landmark name for assistive tech, e.g. "Host details". */
		label: string;
		/** Small line above the title, e.g. "Host Details". */
		kicker: string;
		title: string;
		/**
		 * Column width. `wide` is for forms whose fields or select options need
		 * the room; it still collapses to just `top`, so the column keeps its
		 * width while a form is open instead of resizing with the content.
		 */
		size?: 'default' | 'wide';
		onclose: () => void;
		children?: Snippet;
		/** Submit and cancel controls; those that scroll with the form. */
		footer?: Snippet;
	} = $props();

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			// Let an open dialog keep Escape: it closes itself, and closing the
			// panel behind it at the same time would discard the form silently.
			if (document.querySelector('[data-slot="dialog-content"]')) return;
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<aside
	class="{size === 'wide'
		? 'lg:w-88'
		: 'lg:w-context-panel'} flex min-w-0 flex-col gap-3 rounded-lg border border-border/70 bg-card p-4 lg:min-h-0 lg:self-stretch"
	aria-label={label}
>
	<header class="flex items-start justify-between gap-2">
		<div class="min-w-0">
			<p class="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
				{kicker}
			</p>
			<h4 class="truncate text-sm font-semibold text-foreground" {title}>{title}</h4>
		</div>
		<Button
			variant="ghost"
			size="icon"
			class="size-7 shrink-0 text-muted-foreground"
			onclick={onclose}
			aria-label="Close {label}"
		>
			<XIcon class="size-3.5" />
		</Button>
	</header>

	<div class="flex flex-col gap-3 lg:min-h-0 lg:flex-1 lg:overflow-y-auto">
		{@render children?.()}

		{#if footer}
			<div class="flex flex-col gap-2 border-t border-border/50 pt-3 lg:mt-auto">
				{@render footer()}
			</div>
		{/if}
	</div>
</aside>
