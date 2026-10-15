<script lang="ts">
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
	import { Label } from '#lib/components/ui/label';
	import { Textarea } from '#lib/components/ui/textarea';
	import { errorMessage } from '#lib/errors';
	import { addedAt } from '#lib/inventory';
	import {
		SNIPPET_SECRET_NOTE,
		createSnippet,
		deleteSnippet,
		emptySnippetForm,
		runEnabled,
		runSnippet,
		runSnippetRequest,
		snippetFormFor,
		snippetFormProblem,
		snippetInput,
		snippetPreview,
		updateSnippet,
		type Snippet,
		type SnippetFormValues
	} from '#lib/snippets';
	import { snippetTarget, snippetTargetReady, type SnippetTarget } from '#lib/session-tabs';
	import type { ConnectionStatus } from '#lib/types';
	import type { InventoryPageProps } from './page-props';
	import PageShell from './page-shell.svelte';

	/**
	 * Snippets page: the commands kept encrypted in the vault, and the one way
	 * out of it, a run that is read in full before it is confirmed.
	 */
	let {
		snippets,
		busy,
		sessionStatus,
		sessionId,
		sessionTarget,
		onmutate,
		onerror
	}: InventoryPageProps & {
		snippets: Snippet[];
		/** The status of the tab on screen. */
		sessionStatus: ConnectionStatus;
		/** The id of the tab on screen, if there is one. */
		sessionId?: string;
		/** What that tab is called. */
		sessionTarget: string;
	} = $props();

	/** The line under the heading. The shared note is composed in here once. */
	const description = `Commands kept encrypted in this vault. One is only ever sent to the session that is already open, after you have read it in full and confirmed. ${SNIPPET_SECRET_NOTE}`;

	// Snippet dialog. `editingSnippet` is null while creating a new one.
	let snippetOpen = $state(false);
	let editingSnippet = $state<string | null>(null);
	let snippetForm = $state<SnippetFormValues>(emptySnippetForm());
	// The snippet a delete confirmation is about, and the preview a run waits on.
	let snippetDeleteTarget = $state<Snippet | null>(null);
	let runTarget = $state<Snippet | null>(null);
	// The tab the open preview belongs to, recorded when it was opened. A later
	// switch of the active tab can then never change where the command would go.
	let runSession = $state<SnippetTarget | null>(null);
	let running = $state(false);

	// At most one run at a time, and only against a session that is really up.
	// When a session ends while the preview is open, this turns false by itself.
	const canRun = $derived(runEnabled(sessionStatus, sessionId, running));
	// Whether the preview's own session may still take the command: it has to be
	// the tab on screen, and it has to be up.
	const canRunTarget = $derived(
		snippetTargetReady(runSession, sessionId ?? null, sessionStatus, running)
	);

	function openCreateSnippet() {
		editingSnippet = null;
		snippetForm = emptySnippetForm();
		snippetOpen = true;
	}

	function openEditSnippet(snippet: Snippet) {
		editingSnippet = snippet.id;
		snippetForm = snippetFormFor(snippet);
		snippetOpen = true;
	}

	async function submitSnippet(event: SubmitEvent) {
		event.preventDefault();
		const problem = snippetFormProblem(snippetForm);
		if (problem) {
			onerror(problem);
			return;
		}

		const input = snippetInput(snippetForm);
		const editing = editingSnippet;
		await onmutate(async () => {
			if (editing === null) {
				await createSnippet(input);
			} else {
				await updateSnippet(editing, input);
			}
			snippetOpen = false;
			snippetForm = emptySnippetForm();
		});
	}

	async function confirmDeleteSnippet() {
		const target = snippetDeleteTarget;
		if (!target) return;
		await onmutate(async () => {
			await deleteSnippet(target.id);
			snippetDeleteTarget = null;
		});
	}

	/**
	 * Opens the preview. Opening it runs nothing: only the confirm button does.
	 *
	 * The session is recorded here, so a tab switch cannot quietly aim the
	 * confirmation at a different session than the one the dialog names.
	 */
	function openRunPreview(snippet: Snippet) {
		runTarget = snippet;
		runSession = snippetTarget(sessionId, sessionTarget);
	}

	function closeRunPreview() {
		runTarget = null;
		runSession = null;
	}

	/**
	 * Sends the snippet the preview is showing to the session it was opened for.
	 *
	 * A failure keeps the dialog open and is reported: the snippet is never
	 * presented as having run when the backend refused it. Closing the dialog, on
	 * the other hand, is a cancel, so nothing is sent.
	 */
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

<PageShell title="Snippets" {description}>
	{#snippet actions()}
		<Button size="sm" onclick={openCreateSnippet} disabled={busy}>New snippet</Button>
	{/snippet}
	{#if snippets.length === 0}
		<p class="text-xs text-muted-foreground">No snippets yet.</p>
	{:else}
		<ul class="grid gap-2">
			{#each snippets as snippet (snippet.id)}
				<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
					<div class="min-w-0">
						<p class="truncate text-sm font-medium">{snippet.name}</p>
						<p class="truncate font-mono text-xs text-muted-foreground">
							{snippetPreview(snippet.content)}
						</p>
						<p class="truncate text-xs text-muted-foreground">
							updated {addedAt(snippet.updatedAt)}
						</p>
					</div>
					<div class="flex shrink-0 gap-2">
						<Button
							size="sm"
							onclick={() => openRunPreview(snippet)}
							disabled={busy || !canRun}
							title={canRun ? 'Read this command before it is sent' : 'Connect a session first'}
						>
							Run
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => openEditSnippet(snippet)}
							disabled={busy}
						>
							Edit
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => (snippetDeleteTarget = snippet)}
							disabled={busy}
						>
							Delete
						</Button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</PageShell>

<Dialog bind:open={snippetOpen}>
	<DialogContent class="sm:max-w-xl">
		<DialogHeader>
			<DialogTitle>{editingSnippet === null ? 'New snippet' : 'Edit snippet'}</DialogTitle>
			<DialogDescription>
				The command is stored encrypted in this vault and is only sent to a session you run it
				against, after you confirm the preview. Nothing here runs on its own.
				{SNIPPET_SECRET_NOTE}
			</DialogDescription>
		</DialogHeader>

		<form class="grid gap-3" onsubmit={submitSnippet}>
			<div class="grid gap-1.5">
				<Label for="snippet-name">Name</Label>
				<Input id="snippet-name" bind:value={snippetForm.name} disabled={busy} />
			</div>
			<div class="grid gap-1.5">
				<Label for="snippet-content">Command</Label>
				<Textarea
					id="snippet-content"
					rows={8}
					class="font-mono text-xs"
					placeholder="systemctl restart app"
					bind:value={snippetForm.content}
					disabled={busy}
				/>
				<p class="text-xs text-muted-foreground">
					More than one line is fine. Termust does not check the syntax: the remote shell decides
					what the command means.
				</p>
			</div>

			<DialogFooter>
				<Button type="button" variant="outline" onclick={() => (snippetOpen = false)}>
					Cancel
				</Button>
				<Button type="submit" disabled={busy}>
					{editingSnippet === null ? 'Create snippet' : 'Save snippet'}
				</Button>
			</DialogFooter>
		</form>
	</DialogContent>
</Dialog>

<Dialog
	open={snippetDeleteTarget !== null}
	onOpenChange={(open) => !open && (snippetDeleteTarget = null)}
>
	<DialogContent class="sm:max-w-md">
		<DialogHeader>
			<DialogTitle>Delete this snippet?</DialogTitle>
			<DialogDescription>
				It disappears from the list and can no longer be read or run. Nothing on the server is
				touched: a snippet only ever did something when you ran it.
			</DialogDescription>
		</DialogHeader>

		{#if snippetDeleteTarget}
			<p class="truncate text-sm font-medium">{snippetDeleteTarget.name}</p>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={() => (snippetDeleteTarget = null)}>Cancel</Button>
			<Button variant="destructive" onclick={confirmDeleteSnippet} disabled={busy}>Delete</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>

<Dialog open={runTarget !== null} onOpenChange={(open) => !open && !running && closeRunPreview()}>
	<DialogContent class="sm:max-w-2xl">
		<DialogHeader>
			<DialogTitle>Run this snippet?</DialogTitle>
			<DialogDescription>
				This sends the command to the shell of {runSession?.title ?? 'the tab it was opened for'}.
				Termust does not read the command itself, so check it here before it goes.
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
			<Button onclick={confirmRunSnippet} disabled={!canRunTarget}>
				{running ? 'Running…' : 'Run snippet'}
			</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>
