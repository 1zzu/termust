<script lang="ts">
	import PlayIcon from '@lucide/svelte/icons/play';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';

	import { Alert, AlertDescription } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle
	} from '#lib/components/ui/dialog';
	import { Input } from '#lib/components/ui/input';
	import { errorMessage } from '#lib/errors';
	import { snippetTarget, snippetTargetReady, type SnippetTarget } from '#lib/session-tabs';
	import {
		createSnippet,
		deleteSnippet,
		emptySnippetForm,
		runEnabled,
		runSnippet,
		runSnippetRequest,
		snippetFormFor,
		snippetFormProblem,
		snippetInput,
		updateSnippet,
		type Snippet,
		type SnippetFormValues
	} from '#lib/snippets';
	import type { ConnectionStatus } from '#lib/types';
	import type { VaultStatus } from '#lib/vault';
	import ListDetail from './list-detail.svelte';
	import type { InventoryPageProps } from './page-props';
	import SnippetGrid from './snippet-grid.svelte';
	import SnippetSectionPanel from './snippet-section-panel.svelte';

	let {
		snippets,
		busy = false,
		vaultStatus,
		sessionStatus = 'idle',
		sessionId,
		sessionTarget = '',
		onmutate,
		onerror,
		onrefresh,
		onopenvault
	}: InventoryPageProps & {
		snippets: Snippet[];
		vaultStatus?: VaultStatus;
		sessionStatus?: ConnectionStatus;
		sessionId?: string;
		sessionTarget?: string;
		onrefresh?: () => void | Promise<void>;
		onopenvault?: () => void;
	} = $props();

	let searchQuery = $state('');

	// Snippet inspector panel state
	let formOpen = $state(false);
	let editingSnippetId = $state<string | null>(null);
	let snippetForm = $state<SnippetFormValues>(emptySnippetForm());

	// Run preview dialog state
	let runTarget = $state<Snippet | null>(null);
	let runSession = $state<SnippetTarget | null>(null);
	let running = $state(false);

	const canRun = $derived(runEnabled(sessionStatus, sessionId, running));
	const canRunTarget = $derived(
		snippetTargetReady(runSession, sessionId ?? null, sessionStatus, running)
	);

	$effect(() => {
		// Close inspector if vault locks
		if (vaultStatus !== 'unlocked' && formOpen) {
			formOpen = false;
			editingSnippetId = null;
			snippetForm = emptySnippetForm();
		}
	});

	const filteredSnippets = $derived(
		snippets.filter((snippet) => {
			if (!searchQuery.trim()) return true;
			const q = searchQuery.toLowerCase().trim();
			return snippet.name.toLowerCase().includes(q) || snippet.content.toLowerCase().includes(q);
		})
	);

	// Prune selection if snippet was removed
	$effect(() => {
		if (editingSnippetId !== null && !snippets.some((s) => s.id === editingSnippetId)) {
			closeSnippetForm();
		}
	});

	const selectedSnippet = $derived(
		editingSnippetId ? (snippets.find((s) => s.id === editingSnippetId) ?? null) : null
	);

	function handleSelectSnippet(snippet: Snippet) {
		openEditSnippet(snippet);
	}

	function openCreateSnippet() {
		editingSnippetId = null;
		snippetForm = emptySnippetForm();
		formOpen = true;
	}

	function openEditSnippet(snippet: Snippet) {
		editingSnippetId = snippet.id;
		snippetForm = snippetFormFor(snippet);
		formOpen = true;
	}

	function closeSnippetForm() {
		formOpen = false;
		editingSnippetId = null;
		snippetForm = emptySnippetForm();
	}

	async function submitSnippet(event?: SubmitEvent) {
		event?.preventDefault();
		const problem = snippetFormProblem(snippetForm);
		if (problem) {
			onerror(problem);
			return;
		}

		const input = snippetInput(snippetForm);
		const editing = editingSnippetId;
		await onmutate(async () => {
			if (editing === null) {
				await createSnippet(input);
			} else {
				await updateSnippet(editing, input);
			}
			closeSnippetForm();
		});
	}

	async function handleDeleteSelected() {
		const target = selectedSnippet;
		if (!target) return;
		closeSnippetForm();
		await onmutate(() => deleteSnippet(target.id));
	}

	function openRunPreview(snippet: Snippet) {
		runTarget = snippet;
		runSession = snippetTarget(sessionId, sessionTarget);
	}

	function closeRunPreview() {
		runTarget = null;
		runSession = null;
	}

	async function confirmRunSnippet() {
		const target = runTarget;
		const bound = runSession;
		if (!target || !bound || !canRunTarget) return;

		running = true;
		onerror('');
		try {
			await runSnippet(runSnippetRequest(bound.sessionId, target.id));
			closeRunPreview();
		} catch (error) {
			onerror(errorMessage(error));
		} finally {
			running = false;
		}
	}
</script>

<ListDetail open={formOpen}>
	{#snippet header()}
		<!-- Snippets Toolbar: search on top, actions row below -->
		<div class="flex flex-col gap-3">
			<!-- Top Search Bar -->
			<div class="relative w-full">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					placeholder="Find a snippet by name or command..."
					bind:value={searchQuery}
					class="h-9 w-full rounded-lg border-border/70 bg-card/60 pr-4 pl-9 text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-primary/40"
					aria-label="Find a snippet by name or command"
					disabled={busy}
				/>
			</div>

			<!-- Secondary Action Bar (Single line) -->
			<div class="flex items-center justify-between gap-2">
				<div class="flex items-center gap-2">
					<Button
						size="sm"
						class="h-8 shrink-0 gap-1.5 text-xs font-medium"
						onclick={openCreateSnippet}
						disabled={busy || vaultStatus !== 'unlocked'}
					>
						<PlusIcon class="size-3.5" />
						<span>New snippet</span>
					</Button>
				</div>
			</div>
		</div>
	{/snippet}

	<div class="flex flex-col gap-2">
		<div class="flex items-center justify-between px-0.5">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Snippets</h3>
			{#if filteredSnippets.length > 0}
				<span class="text-[11px] text-muted-foreground">
					{filteredSnippets.length}
					{filteredSnippets.length === 1 ? 'snippet' : 'snippets'}
				</span>
			{/if}
		</div>

		<SnippetGrid
			snippets={filteredSnippets}
			totalCount={snippets.length}
			selectedSnippetId={editingSnippetId}
			{busy}
			{canRun}
			{vaultStatus}
			onselect={handleSelectSnippet}
			onrun={openRunPreview}
			{onopenvault}
		/>
	</div>

	{#snippet panel()}
		{#if formOpen}
			<SnippetSectionPanel
				mode={editingSnippetId === null ? 'create' : 'edit'}
				snippet={selectedSnippet}
				bind:form={snippetForm}
				{sessionStatus}
				{sessionId}
				{sessionTarget}
				{busy}
				{running}
				{vaultStatus}
				onsubmit={submitSnippet}
				onrun={openRunPreview}
				ondelete={handleDeleteSelected}
				onclose={closeSnippetForm}
			/>
		{/if}
	{/snippet}
</ListDetail>

<!-- Run snippet confirmation preview dialog -->
<Dialog open={runTarget !== null} onOpenChange={(open) => !open && !running && closeRunPreview()}>
	<DialogContent class="sm:max-w-xl">
		<DialogHeader>
			<DialogTitle>Run this snippet?</DialogTitle>
			<DialogDescription>
				This sends the command to the shell of {runSession?.title ?? 'the tab it was opened for'}.
				Check it here before it is sent.
			</DialogDescription>
		</DialogHeader>

		{#if runTarget}
			<div class="grid gap-2">
				<p class="truncate text-sm font-medium">{runTarget.name}</p>
				<pre
					class="max-h-64 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs wrap-break-word whitespace-pre-wrap">{runTarget.content}</pre>
				<p class="text-xs text-amber-500">
					It is sent as typed input to the shell that is already open.
				</p>
			</div>
		{/if}

		{#if runSession && !canRunTarget}
			<Alert>
				<AlertDescription>
					This preview belongs to {runSession.title}. It is only ever sent to the session it was
					confirmed for, so switch back to that tab to run it.
				</AlertDescription>
			</Alert>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={closeRunPreview} disabled={running}>Cancel</Button>
			<Button onclick={confirmRunSnippet} disabled={!canRunTarget || running} class="gap-1.5">
				<PlayIcon class="size-3.5 fill-current" />
				<span>{running ? 'Running…' : 'Run snippet'}</span>
			</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>
