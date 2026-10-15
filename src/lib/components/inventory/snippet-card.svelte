<script lang="ts">
	import PlayIcon from '@lucide/svelte/icons/play';
	import TerminalIcon from '@lucide/svelte/icons/terminal';
	import { Button } from '#lib/components/ui/button';
	import { Card } from '#lib/components/ui/card';
	import { snippetPreview, type Snippet } from '#lib/snippets';

	let {
		snippet,
		selected = false,
		busy = false,
		canRun = false,
		onselect,
		onrun
	}: {
		snippet: Snippet;
		selected?: boolean;
		busy?: boolean;
		canRun?: boolean;
		onselect: (snippet: Snippet) => void;
		onrun?: (snippet: Snippet) => void;
	} = $props();

	function handleClick() {
		onselect(snippet);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onselect(snippet);
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
		aria-label="{snippet.name}{selected ? ' (selected)' : ''}. Click to view details."
		title="Click to view details"
		class="cursor-pointer flex-row items-center gap-3 p-3 transition-all duration-150 outline-none {selected
			? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
			: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
	>
		<!-- Avatar bulat untuk Snippet -->
		<div
			class="flex size-10 shrink-0 items-center justify-center rounded-full bg-violet-600 text-white shadow-xs dark:bg-violet-500"
			aria-hidden="true"
		>
			<TerminalIcon class="size-5" />
		</div>

		<!-- Info Snippet: Judul & Subtitle -->
		<div class="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
			<div class="flex items-center justify-between gap-2">
				<span class="truncate text-sm font-semibold text-foreground">{snippet.name}</span>
			</div>

			<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
				<span class="truncate font-mono text-[11px]">{snippetPreview(snippet.content)}</span>
			</div>
		</div>

		<!-- Quick Run button jika ada session aktif -->
		{#if onrun && canRun}
			<Button
				variant="ghost"
				size="icon"
				class="size-8 shrink-0 text-muted-foreground hover:bg-primary/10 hover:text-primary"
				onclick={(e) => {
					e.stopPropagation();
					onrun(snippet);
				}}
				title="Run in active terminal"
				aria-label="Run snippet in active terminal"
				disabled={busy}
			>
				<PlayIcon class="size-3.5 fill-current" />
			</Button>
		{/if}
	</Card>
</li>
