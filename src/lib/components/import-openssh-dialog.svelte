<script lang="ts">
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import { Checkbox } from '#lib/components/ui/checkbox';
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
	import {
		MAX_CONFIG_BYTES,
		configProblem,
		confirmationLines,
		defaultSelection,
		entryStatusLabel,
		importOpenSshHosts,
		outcomeLabel,
		previewOpenSshConfig,
		problemResults,
		resultLine,
		selectable,
		toggleSelection,
		warningLabel,
		type ImportPreview,
		type ImportSummary
	} from '#lib/openssh';
	import type { VaultStatus } from '#lib/vault';

	// The dialog follows the vault: it is only reachable while the vault is open,
	// and locking it throws the config away along with the preview.
	let {
		vaultStatus,
		onimported
	}: {
		vaultStatus: VaultStatus | undefined;
		/** Called after an import, so the Hosts list is re-read. */
		onimported?: () => void | Promise<void>;
	} = $props();

	type Step = 'input' | 'preview' | 'confirm' | 'result';

	let open = $state(false);
	let step = $state<Step>('input');
	let fileName = $state('');
	// The config text: held here, never in local storage, and dropped as soon as
	// the dialog closes, the import finishes, or the vault locks.
	let content = $state('');
	let problem = $state('');
	let preview = $state<ImportPreview | null>(null);
	let selection = $state<number[]>([]);
	let summary = $state<ImportSummary | null>(null);
	let failure = $state('');
	let busy = $state(false);

	const canPreview = $derived(content.trim() !== '' && !busy);

	/** The shape this build reads, as the editor hint. */
	const EXAMPLE_CONFIG = 'Host production\n  HostName 192.0.2.10\n  User deploy\n  Port 22';

	$effect(() => {
		// Anything other than an open vault means this dialog has no business
		// holding a config.
		if (vaultStatus !== 'unlocked' && (open || content !== '' || preview !== null)) {
			open = false;
			clear();
		}
	});

	/** Throws away everything this dialog holds. */
	function clear() {
		step = 'input';
		fileName = '';
		content = '';
		problem = '';
		preview = null;
		selection = [];
		summary = null;
		failure = '';
		busy = false;
	}

	function close() {
		open = false;
		clear();
	}

	function clearInput() {
		fileName = '';
		content = '';
		problem = '';
	}

	/** Reads the chosen file here, in the webview. No path is ever sent. */
	async function chooseFile(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) {
			return;
		}

		problem = '';
		try {
			const text = await file.text();
			problem = configProblem(text, file.size) ?? '';
			if (problem !== '') {
				fileName = '';
				content = '';
			} else {
				fileName = file.name;
				content = text;
			}
		} catch {
			problem = 'That file could not be read.';
		} finally {
			// The picker keeps no handle on the file once it has been read.
			input.value = '';
		}
	}

	async function askForPreview() {
		const found = configProblem(content);
		if (found !== null) {
			problem = found;
			return;
		}

		busy = true;
		problem = '';
		failure = '';
		try {
			const answer = await previewOpenSshConfig(content);
			preview = answer;
			// Only the entries that can be imported start ticked.
			selection = defaultSelection(answer);
			step = 'preview';
		} catch (error) {
			problem = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	function toggle(index: number) {
		if (preview === null) {
			return;
		}
		selection = toggleSelection(selection, index, preview);
	}

	/** The confirmation step, where nothing has been written yet. */
	function askForConfirmation() {
		if (selection.length === 0) {
			return;
		}
		step = 'confirm';
	}

	async function runImport() {
		if (preview === null || selection.length === 0) {
			return;
		}

		busy = true;
		failure = '';
		try {
			summary = await importOpenSshHosts(content, selection);
			// The config has done its work: it does not stay in memory for a
			// second run, and the result step is the only thing left to read.
			content = '';
			fileName = '';
			preview = null;
			selection = [];
			step = 'result';
			await onimported?.();
		} catch (error) {
			failure = errorMessage(error);
		} finally {
			busy = false;
		}
	}
</script>

{#if vaultStatus === 'unlocked'}
	<Button variant="outline" size="sm" onclick={() => (open = true)} disabled={busy}>
		Import OpenSSH
	</Button>
{/if}

<Dialog bind:open onOpenChange={(next) => !next && !busy && clear()}>
	<DialogContent class="sm:max-w-2xl">
		<DialogHeader>
			<DialogTitle>Import OpenSSH config</DialogTitle>
			<DialogDescription>
				A small subset of an OpenSSH client config: <code>Host</code>, <code>HostName</code>,
				<code>User</code> and <code>Port</code>. Everything else is reported and left alone. Nothing
				is read from your disk but the file you choose, and no key or included file is ever
				followed.
			</DialogDescription>
		</DialogHeader>

		{#if step === 'input'}
			<div class="grid gap-3">
				<div class="grid gap-1.5">
					<Label for="openssh-file">Choose a config file</Label>
					<Input
						id="openssh-file"
						type="file"
						accept=".conf,.config,text/plain"
						onchange={chooseFile}
						disabled={busy}
					/>
					{#if fileName !== ''}
						<p class="text-xs text-muted-foreground">Chosen file: {fileName}</p>
					{/if}
				</div>

				<div class="grid gap-1.5">
					<Label for="openssh-content">Or paste the config</Label>
					<Textarea
						id="openssh-content"
						rows={8}
						placeholder={EXAMPLE_CONFIG}
						bind:value={content}
						disabled={busy}
					/>
					<p class="text-xs text-muted-foreground">
						Up to {MAX_CONFIG_BYTES} bytes. The text stays in this window: it is not saved anywhere, and
						it is dropped when you close this dialog or lock the vault.
					</p>
				</div>

				{#if problem !== ''}
					<Alert variant="destructive">
						<AlertTitle>Nothing was sent</AlertTitle>
						<AlertDescription>{problem}</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else if step === 'preview' && preview !== null}
			<div class="grid gap-3">
				{#if preview.warnings.length > 0}
					<Alert>
						<AlertTitle>Skipped before any Host block</AlertTitle>
						<AlertDescription>
							<ul class="grid gap-1">
								{#each preview.warnings as warning, index (index)}
									<li>{warningLabel(warning)}</li>
								{/each}
							</ul>
						</AlertDescription>
					</Alert>
				{/if}

				{#if preview.entries.length === 0}
					<p class="text-sm text-muted-foreground">
						This config has no Host block that Termust can read.
					</p>
				{:else}
					<ul class="grid max-h-80 gap-2 overflow-y-auto">
						{#each preview.entries as entry (entry.index)}
							{@const canImport = selectable(entry)}
							<li class="flex items-start gap-3 rounded-lg border p-2">
								<Checkbox
									class="mt-0.5"
									checked={selection.includes(entry.index)}
									disabled={!canImport}
									aria-label={`Import ${entry.label}`}
									onCheckedChange={() => toggle(entry.index)}
								/>
								<div class="min-w-0 grow">
									<div class="flex items-baseline justify-between gap-2">
										<p class="truncate text-sm font-medium">{entry.label}</p>
										<p class="shrink-0 text-xs text-muted-foreground">
											{canImport ? '' : entryStatusLabel(entry.status)}
										</p>
									</div>
									<p class="truncate text-xs text-muted-foreground">
										{entry.username}@{entry.address}:{entry.port ?? '—'}
									</p>
									{#if entry.warnings.length > 0}
										<ul class="mt-1 grid gap-0.5">
											{#each entry.warnings as warning, index (index)}
												<li class="text-xs text-amber-500">{warningLabel(warning)}</li>
											{/each}
										</ul>
									{/if}
								</div>
							</li>
						{/each}
					</ul>

					<p class="text-xs text-muted-foreground">
						{selection.length} of {preview.entries.length} selected. Only a ready host can be imported;
						the others are shown so you can see what was skipped.
					</p>
				{/if}

				{#if problem !== ''}
					<Alert variant="destructive">
						<AlertTitle>Import error</AlertTitle>
						<AlertDescription>{problem}</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else if step === 'confirm'}
			<div class="grid gap-3">
				<ul class="grid gap-1 text-sm">
					{#each confirmationLines(selection.length) as line, index (index)}
						<li>{line}</li>
					{/each}
				</ul>
				<p class="text-xs text-muted-foreground">Nothing is written until you press Import.</p>
				{#if failure !== ''}
					<Alert variant="destructive">
						<AlertTitle>Import error</AlertTitle>
						<AlertDescription>{failure}</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else if step === 'result' && summary !== null}
			<div class="grid gap-3">
				<p class="text-sm font-medium">{resultLine(summary)}</p>
				{#if problemResults(summary).length > 0}
					<Alert variant={summary.failed > 0 ? 'destructive' : 'default'}>
						<AlertTitle>
							{summary.failed > 0 ? 'Some hosts were not added' : 'Some entries were left out'}
						</AlertTitle>
						<AlertDescription>
							<ul class="grid gap-1">
								{#each problemResults(summary) as result (result.index)}
									<li>
										{result.label} — {outcomeLabel(result.outcome)}{#if result.reason}: {result.reason}{/if}
									</li>
								{/each}
							</ul>
						</AlertDescription>
					</Alert>
				{/if}
				<p class="text-xs text-muted-foreground">
					Imported hosts are ordinary saved hosts: edit one to give it a Credential, then its
					Connect button works.
				</p>
			</div>
		{/if}

		{#if failure !== '' && step !== 'confirm'}
			<Alert variant="destructive">
				<AlertTitle>Import error</AlertTitle>
				<AlertDescription>{failure}</AlertDescription>
			</Alert>
		{/if}

		<DialogFooter>
			{#if step === 'input'}
				<Button variant="outline" onclick={close} disabled={busy}>Cancel</Button>
				<Button variant="outline" onclick={clearInput} disabled={busy || content === ''}
					>Clear</Button
				>
				<Button onclick={askForPreview} disabled={!canPreview}>Preview</Button>
			{:else if step === 'preview'}
				<Button variant="outline" onclick={close} disabled={busy}>Cancel</Button>
				<Button onclick={askForConfirmation} disabled={busy || selection.length === 0}>
					Continue
				</Button>
			{:else if step === 'confirm'}
				<Button variant="outline" onclick={() => (step = 'preview')} disabled={busy}>Back</Button>
				<Button onclick={runImport} disabled={busy}>
					{busy ? 'Importing…' : `Import ${selection.length}`}
				</Button>
			{:else}
				<Button onclick={close} disabled={busy}>Close</Button>
			{/if}
		</DialogFooter>
	</DialogContent>
</Dialog>
