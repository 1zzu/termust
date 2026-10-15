<script lang="ts">
	import ArrowRightToLineIcon from '@lucide/svelte/icons/arrow-right-to-line';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';
	import PlayIcon from '@lucide/svelte/icons/play';
	import TerminalIcon from '@lucide/svelte/icons/terminal';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';

	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import { Label } from '#lib/components/ui/label';
	import { Textarea } from '#lib/components/ui/textarea';
	import { runEnabled, type Snippet, type SnippetFormValues } from '#lib/snippets';
	import type { ConnectionStatus } from '#lib/types';
	import type { VaultStatus } from '#lib/vault';

	let {
		mode = 'edit',
		snippet = null,
		form = $bindable(),
		sessionStatus = 'idle',
		sessionId,
		sessionTarget = '',
		busy = false,
		running = false,
		vaultStatus,
		onsubmit,
		onrun,
		ondelete,
		onclose
	}: {
		mode: 'create' | 'edit';
		snippet?: Snippet | null;
		form: SnippetFormValues;
		sessionStatus?: ConnectionStatus;
		sessionId?: string;
		sessionTarget?: string;
		busy?: boolean;
		running?: boolean;
		vaultStatus?: VaultStatus;
		onsubmit: (event?: SubmitEvent) => void;
		onrun?: (snippet: Snippet) => void;
		ondelete?: () => void;
		onclose: () => void;
	} = $props();

	let showMoreMenu = $state(false);

	const canRun = $derived(runEnabled(sessionStatus, sessionId, running));

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			if (document.querySelector('[data-slot="dialog-content"]')) return;
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<aside
	class="flex w-full shrink-0 flex-col gap-4 border-l border-border bg-sidebar p-4 sm:p-5 lg:min-h-0 lg:w-96 lg:self-stretch xl:w-104"
	aria-label={mode === 'create' ? 'New Snippet' : 'Snippet Details'}
>
	<!-- Header: Title, Vault badge, Options and Close button -->
	<header class="flex items-start justify-between gap-2 pt-0.5">
		<div class="flex flex-col gap-0.5">
			<h3 class="text-sm font-semibold text-foreground">
				{mode === 'create' ? 'New Snippet' : 'Snippet Details'}
			</h3>
			<div class="flex items-center gap-1 text-[11px] text-muted-foreground">
				<span>{vaultStatus === 'unlocked' ? 'Personal vault' : 'Local vault'}</span>
				<ChevronDownIcon class="size-3 opacity-60" />
			</div>
		</div>

		<div class="flex items-center gap-0.5">
			{#if mode === 'edit' && ondelete}
				<div class="relative">
					<Button
						variant="ghost"
						size="icon"
						class="size-7 text-muted-foreground hover:text-foreground"
						onclick={() => (showMoreMenu = !showMoreMenu)}
						title="More options"
						aria-label="More options"
					>
						<MoreHorizontalIcon class="size-4" />
					</Button>
					{#if showMoreMenu}
						<div
							class="fixed inset-0 z-40"
							onclick={() => (showMoreMenu = false)}
							aria-hidden="true"
						></div>
						<div
							class="absolute top-full right-0 z-50 mt-1 min-w-36 animate-in rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md fade-in-0 zoom-in-95"
						>
							<button
								type="button"
								class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-destructive hover:bg-destructive/10"
								onclick={() => {
									showMoreMenu = false;
									ondelete();
								}}
							>
								<Trash2Icon class="size-3.5" />
								<span>Delete snippet</span>
							</button>
						</div>
					{/if}
				</div>
			{/if}

			<Button
				variant="ghost"
				size="icon"
				class="size-7 text-muted-foreground hover:text-foreground"
				onclick={onclose}
				aria-label="Close snippet details"
				title="Close panel"
			>
				<ArrowRightToLineIcon class="size-3.5" />
			</Button>
		</div>
	</header>

	<!-- Scrollable Form Body -->
	<form
		id="snippet-inspector-form"
		class="flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5 sm:gap-5"
		onsubmit={(e) => {
			e.preventDefault();
			onsubmit(e);
		}}
	>
		<!-- 1. General Section (Name) -->
		<div class="flex flex-col gap-2">
			<Label for="inspector-snippet-name" class="text-xs font-medium text-muted-foreground">
				General
			</Label>

			<!-- Snippet Name Input -->
			<Input
				id="inspector-snippet-name"
				bind:value={form.name}
				disabled={busy}
				placeholder="Snippet name (e.g. Restart Docker, Tail Nginx logs)"
				class="h-9 border-border/60 bg-muted/30 text-xs"
				required
			/>
		</div>

		<!-- 2. Command Section -->
		<div class="flex flex-col gap-1.5">
			<Label for="inspector-snippet-content" class="text-xs font-medium text-muted-foreground">
				Command
			</Label>
			<Textarea
				id="inspector-snippet-content"
				rows={8}
				bind:value={form.content}
				disabled={busy}
				placeholder="docker compose down && docker compose up -d"
				class="border-border/60 bg-muted/30 font-mono text-xs"
				required
			/>
			<p class="text-[11px] text-muted-foreground">
				Multi-line commands are sent as typed input to the active session.
			</p>
		</div>

		<!-- 3. Target Session Section (in edit mode) -->
		{#if mode === 'edit'}
			<div class="flex flex-col gap-2 border-t border-border/60 pt-3">
				<span class="text-xs font-medium text-muted-foreground">Target Session</span>

				{#if canRun && snippet && onrun}
					<div
						class="flex items-center justify-between rounded-md border border-border/60 bg-muted/20 p-2.5"
					>
						<div class="flex items-center gap-2 truncate text-xs">
							<span class="size-2 rounded-full bg-emerald-500"></span>
							<span class="truncate font-medium text-foreground">{sessionTarget}</span>
						</div>
						<Button
							type="button"
							size="sm"
							variant="secondary"
							class="h-7 shrink-0 gap-1.5 text-xs"
							onclick={() => onrun(snippet)}
							disabled={busy || running}
						>
							<PlayIcon class="size-3 fill-current" />
							<span>{running ? 'Running…' : 'Run now'}</span>
						</Button>
					</div>
				{:else}
					<div
						class="flex items-center gap-2 rounded-md border border-border/50 bg-muted/10 p-2.5 text-xs text-muted-foreground"
					>
						<TerminalIcon class="size-4 shrink-0 opacity-60" />
						<span>No active connected terminal. Connect to a host to run this snippet.</span>
					</div>
				{/if}
			</div>
		{/if}
	</form>

	<!-- Footer: Submit button -->
	<footer>
		<Button
			type="submit"
			form="snippet-inspector-form"
			class="h-10 w-full bg-blue-600 text-xs font-semibold text-white shadow-sm hover:bg-blue-500"
			disabled={busy || running}
		>
			{mode === 'create' ? 'Create snippet' : 'Save changes'}
		</Button>
	</footer>
</aside>
