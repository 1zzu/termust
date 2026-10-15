<script lang="ts">
	import LockIcon from '@lucide/svelte/icons/lock';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import TerminalIcon from '@lucide/svelte/icons/terminal';

	import { Button } from '#lib/components/ui/button';
	import type { Snippet } from '#lib/snippets';
	import type { VaultStatus } from '#lib/vault';
	import SnippetCard from './snippet-card.svelte';

	let {
		snippets,
		totalCount,
		selectedSnippetId = null,
		busy = false,
		canRun = false,
		vaultStatus,
		onselect,
		onrun,
		onopenvault
	}: {
		snippets: Snippet[];
		totalCount: number;
		selectedSnippetId?: string | null;
		busy?: boolean;
		canRun?: boolean;
		vaultStatus?: VaultStatus;
		onselect: (snippet: Snippet) => void;
		onrun?: (snippet: Snippet) => void;
		onopenvault?: () => void;
	} = $props();
</script>

{#if vaultStatus === 'locked'}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<LockIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">Saved inventory is locked</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Unlock the local vault to access saved snippets and commands.
		</p>
		{#if onopenvault}
			<Button variant="outline" size="sm" class="mt-4 text-xs" onclick={onopenvault}>
				Unlock Vault
			</Button>
		{/if}
	</div>
{:else if vaultStatus === 'uninitialized'}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<ShieldAlertIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No local vault yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Create a vault to save encrypted commands and snippets.
		</p>
		{#if onopenvault}
			<Button variant="outline" size="sm" class="mt-4 text-xs" onclick={onopenvault}>
				Create Vault
			</Button>
		{/if}
	</div>
{:else if totalCount === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<TerminalIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No snippets yet</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			Snippets keep commonly used terminal commands and scripts encrypted in your vault for fast
			execution.
		</p>
	</div>
{:else if snippets.length === 0}
	<div
		class="flex flex-col items-center justify-center rounded-lg border border-dashed border-border/80 p-8 text-center"
		role="status"
	>
		<SearchIcon class="size-10 text-muted-foreground/50" />
		<h4 class="mt-3 text-sm font-medium text-foreground">No matching snippets</h4>
		<p class="mt-1 max-w-sm text-xs text-muted-foreground">
			No snippets match your search query. Try adjusting your search query.
		</p>
	</div>
{:else}
	<div class="@container">
		<ul
			class="grid grid-cols-1 gap-3 @lg:grid-cols-2 @3xl:grid-cols-3 @5xl:grid-cols-4"
			role="list"
		>
			{#each snippets as snippet (snippet.id)}
				<SnippetCard
					{snippet}
					selected={snippet.id === selectedSnippetId}
					{busy}
					{canRun}
					{onselect}
					{onrun}
				/>
			{/each}
		</ul>
	</div>
{/if}
