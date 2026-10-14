<script lang="ts">
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
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
	import { errorMessage } from '#lib/errors';
	import {
		RESTORE_PHRASE,
		chooseBackupToOpen,
		chooseBackupToSave,
		createdAtLabel,
		defaultBackupFileName,
		exportEncryptedBackup,
		exportPasswordProblem,
		exportReady,
		exportRequest,
		exportWarningLines,
		fileNameOf,
		formatBytes,
		inspectEncryptedBackup,
		inspectRequest,
		kindCountLines,
		restoreEncryptedBackup,
		restoreProblem,
		restoreReady,
		restoreRequest,
		restoreResultLine,
		restoreWarningLines,
		summaryLine,
		type BackupSummary,
		type ExportSummary,
		type RestoreSummary
	} from '#lib/backup';
	import type { VaultStatus } from '#lib/vault';

	// The dialogs follow the vault and the sessions: they are only reachable while
	// the vault is open, locking it throws every password away, and any live SSH
	// session — not only the one on screen — stops a restore, without ever
	// disconnecting it.
	let {
		vaultStatus,
		hasLiveSessions = false,
		onrestored
	}: {
		vaultStatus: VaultStatus | undefined;
		/** Whether any tab still has a session that is connected or connecting. */
		hasLiveSessions?: boolean;
		/** Called after a successful restore, so every list is re-read. */
		onrestored?: () => void | Promise<void>;
	} = $props();

	// --- export ---------------------------------------------------------

	let exportOpen = $state(false);
	let exportPassword = $state('');
	let exportConfirmation = $state('');
	let exportFailure = $state('');
	let exported = $state<ExportSummary | null>(null);
	let exporting = $state(false);

	const exportProblem = $derived(exportPasswordProblem(exportPassword, exportConfirmation));
	const canExport = $derived(exportReady(exportPassword, exportConfirmation, exporting));

	// --- restore --------------------------------------------------------

	type Step = 'choose' | 'preview' | 'confirm' | 'result';

	let restoreOpen = $state(false);
	let step = $state<Step>('choose');
	/** The file the user chose. Never displayed whole, only its name. */
	let sourcePath = $state('');
	let restorePassword = $state('');
	let preview = $state<BackupSummary | null>(null);
	let typed = $state('');
	let restoreFailure = $state('');
	let restored = $state<RestoreSummary | null>(null);
	let restoring = $state(false);

	const restoreProblemText = $derived(restoreProblem(typed, hasLiveSessions, restoring));
	const canRestore = $derived(restoreReady(typed, hasLiveSessions, restoring));

	$effect(() => {
		// Anything other than an open vault means these dialogs have no business
		// holding a password.
		if (vaultStatus !== 'unlocked' && !closed()) {
			clear();
		}
	});

	/** Whether both dialogs are already closed and empty. */
	function closed(): boolean {
		return (
			!exportOpen &&
			!restoreOpen &&
			exportPassword === '' &&
			exportConfirmation === '' &&
			restorePassword === '' &&
			typed === ''
		);
	}

	/**
	 * Throws away everything these dialogs hold: the passwords, the chosen path,
	 * the preview and the results. Used on close, on cancel and when the vault
	 * locks.
	 */
	function clear() {
		exportOpen = false;
		exportPassword = '';
		exportConfirmation = '';
		exportFailure = '';
		exported = null;
		exporting = false;

		restoreOpen = false;
		step = 'choose';
		sourcePath = '';
		restorePassword = '';
		preview = null;
		typed = '';
		restoreFailure = '';
		restored = null;
		restoring = false;
	}

	function openExport() {
		clear();
		exportOpen = true;
	}

	function openRestore() {
		clear();
		restoreOpen = true;
	}

	/** The chosen file's name, which is all the dialog shows. */
	const sourceName = $derived(sourcePath === '' ? '' : fileNameOf(sourcePath));

	/**
	 * Writes the backup.
	 *
	 * The native save dialog asks before replacing a file, so a cancelled dialog
	 * is simply that: nothing is written and nothing is reported as a failure.
	 */
	async function runExport() {
		if (!canExport) {
			return;
		}

		exporting = true;
		exportFailure = '';
		try {
			const destinationPath = await chooseBackupToSave(defaultBackupFileName());
			if (destinationPath === null) {
				return;
			}
			exported = await exportEncryptedBackup(exportRequest(destinationPath, exportPassword));
			// The password has done its work and does not stay in memory.
			exportPassword = '';
			exportConfirmation = '';
		} catch (error) {
			exportFailure = errorMessage(error);
		} finally {
			exporting = false;
		}
	}

	/** Asks for the file to restore. Only a file, never a folder. */
	async function chooseFile() {
		restoreFailure = '';
		try {
			const chosen = await chooseBackupToOpen();
			if (chosen === null) {
				return;
			}
			sourcePath = chosen;
			preview = null;
			restored = null;
			step = 'choose';
		} catch (error) {
			restoreFailure = errorMessage(error);
		}
	}

	/**
	 * Reads the file and reports what it holds. This writes nothing, in the vault
	 * or on disk; it is what lets the user see the size before deciding.
	 */
	async function inspect() {
		if (sourcePath === '' || restorePassword.length === 0 || restoring) {
			return;
		}

		restoring = true;
		restoreFailure = '';
		try {
			preview = await inspectEncryptedBackup(inspectRequest(sourcePath, restorePassword));
			step = 'preview';
		} catch (error) {
			// The password is wrong or the file is damaged; either way the
			// message is the backend's one generic answer.
			preview = null;
			restoreFailure = errorMessage(error);
		} finally {
			restoring = false;
		}
	}

	function askForConfirmation() {
		if (preview === null) {
			return;
		}
		typed = '';
		step = 'confirm';
	}

	/** Replaces the vault. The mode and the typed phrase travel with it. */
	async function runRestore() {
		if (!canRestore || sourcePath === '' || preview === null) {
			return;
		}

		restoring = true;
		restoreFailure = '';
		try {
			restored = await restoreEncryptedBackup(
				restoreRequest(sourcePath, restorePassword, typed, preview.fileDigest)
			);
			// The file is open no more: the password goes with the work that
			// needed it.
			restorePassword = '';
			typed = '';
			step = 'result';
			// Every list the panel shows is re-read from the vault that just
			// changed.
			await onrestored?.();
		} catch (error) {
			// The current data is still there: a failed restore changes nothing.
			restoreFailure = errorMessage(error);
		} finally {
			restoring = false;
		}
	}
</script>

{#if vaultStatus === 'unlocked'}
	<div class="flex flex-wrap gap-2">
		<Button size="sm" onclick={openExport} disabled={exporting}>Export backup</Button>
		<Button
			size="sm"
			variant="outline"
			onclick={openRestore}
			disabled={restoring || hasLiveSessions}
			title={hasLiveSessions ? 'Disconnect every SSH session first' : undefined}
		>
			Restore from backup
		</Button>
	</div>

	{#if hasLiveSessions}
		<p class="text-xs text-amber-500">
			An SSH session is still open. A restore is refused while one is running, so disconnect it
			first. Termust never disconnects it for you.
		</p>
	{/if}
{/if}

<!-- Export -->
<Dialog bind:open={exportOpen} onOpenChange={(next) => !next && !exporting && clear()}>
	<DialogContent class="sm:max-w-lg">
		<DialogHeader>
			<DialogTitle>Export an encrypted backup</DialogTitle>
			<DialogDescription>
				Every group, host, credential, known host and snippet in this vault is written to one
				encrypted file. The file opens with the password you choose here, and with nothing else.
			</DialogDescription>
		</DialogHeader>

		<ul class="grid gap-2 text-xs text-muted-foreground">
			{#each exportWarningLines() as line, index (index)}
				<li>{line}</li>
			{/each}
		</ul>

		{#if exported === null}
			<div class="grid gap-3">
				<div class="grid gap-1.5">
					<Label for="backup-password">Backup password</Label>
					<Input
						id="backup-password"
						type="password"
						autocomplete="new-password"
						spellcheck={false}
						bind:value={exportPassword}
						disabled={exporting}
					/>
				</div>
				<div class="grid gap-1.5">
					<Label for="backup-password-confirmation">Confirm backup password</Label>
					<Input
						id="backup-password-confirmation"
						type="password"
						autocomplete="new-password"
						spellcheck={false}
						bind:value={exportConfirmation}
						disabled={exporting}
					/>
				</div>
				{#if exportPassword.length > 0 && exportConfirmation.length > 0 && exportProblem}
					<Alert variant="destructive">
						<AlertTitle>The form is not ready</AlertTitle>
						<AlertDescription>{exportProblem}</AlertDescription>
					</Alert>
				{/if}
				{#if exportFailure !== ''}
					<Alert variant="destructive">
						<AlertTitle>The backup was not written</AlertTitle>
						<AlertDescription>{exportFailure}</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else}
			<Alert>
				<AlertTitle>Backup written</AlertTitle>
				<AlertDescription>
					<p>
						{exported.fileName} — {formatBytes(exported.bytes)}, {summaryLine(exported)}
					</p>
					<p class="text-xs">Made {createdAtLabel(exported.createdAt)}.</p>
				</AlertDescription>
			</Alert>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={clear} disabled={exporting}>Close</Button>
			{#if exported === null}
				<Button onclick={runExport} disabled={!canExport}>
					{exporting ? 'Encrypting…' : 'Choose location and export'}
				</Button>
			{/if}
		</DialogFooter>
	</DialogContent>
</Dialog>

<!-- Restore -->
<Dialog bind:open={restoreOpen} onOpenChange={(next) => !next && !restoring && clear()}>
	<DialogContent class="sm:max-w-lg">
		<DialogHeader>
			<DialogTitle>Restore from a backup</DialogTitle>
			<DialogDescription>
				The backup's records replace everything this vault holds. The vault itself stays: its master
				password and its key are untouched, and every restored record is encrypted again with them.
			</DialogDescription>
		</DialogHeader>

		{#if step === 'choose' || step === 'preview'}
			<div class="grid gap-3">
				<div class="grid gap-1.5">
					<Label for="backup-file">Backup file</Label>
					<div class="flex items-center gap-2">
						<Button variant="outline" size="sm" onclick={chooseFile} disabled={restoring}>
							Choose file
						</Button>
						<span class="truncate font-mono text-xs text-muted-foreground">
							{sourceName === '' ? 'No file chosen' : sourceName}
						</span>
					</div>
				</div>

				<div class="grid gap-1.5">
					<Label for="restore-password">Backup password</Label>
					<Input
						id="restore-password"
						type="password"
						autocomplete="off"
						spellcheck={false}
						bind:value={restorePassword}
						disabled={restoring}
					/>
				</div>

				{#if restoreFailure !== ''}
					<Alert variant="destructive">
						<AlertTitle>The backup was not read</AlertTitle>
						<AlertDescription>{restoreFailure}</AlertDescription>
					</Alert>
				{/if}

				{#if step === 'preview' && preview !== null}
					<div class="grid gap-2 rounded-lg border p-3">
						<p class="text-sm font-medium">{summaryLine(preview)}</p>
						<ul class="grid gap-1 text-xs text-muted-foreground">
							{#each kindCountLines(preview.countByKind) as line, index (index)}
								<li>{line}</li>
							{/each}
						</ul>
						<p class="text-xs text-muted-foreground">
							Made {createdAtLabel(preview.createdAt)} by Termust {preview.sourceAppVersion}. No
							secret is shown here: only how much there is.
						</p>
					</div>
				{/if}
			</div>
		{:else if step === 'confirm'}
			<div class="grid gap-3">
				<Alert variant="destructive">
					<AlertTitle>This replaces everything in this vault</AlertTitle>
					<AlertDescription>
						<ul class="grid gap-1">
							{#each restoreWarningLines() as line, index (index)}
								<li>{line}</li>
							{/each}
						</ul>
					</AlertDescription>
				</Alert>

				<div class="grid gap-1.5">
					<Label for="restore-confirmation">Type {RESTORE_PHRASE} to confirm</Label>
					<Input
						id="restore-confirmation"
						autocomplete="off"
						spellcheck={false}
						placeholder={RESTORE_PHRASE}
						bind:value={typed}
						disabled={restoring}
					/>
					{#if restoreProblemText && typed.length > 0}
						<p class="text-xs text-muted-foreground">{restoreProblemText}</p>
					{/if}
				</div>

				{#if restoreFailure !== ''}
					<Alert variant="destructive">
						<AlertTitle>The vault was not replaced</AlertTitle>
						<AlertDescription>
							<p>{restoreFailure}</p>
							<p class="text-xs">
								Nothing in this vault changed: the records in it are the ones you had before.
							</p>
						</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else}
			<Alert>
				<AlertTitle>Restore finished</AlertTitle>
				<AlertDescription>
					{#if restored !== null}
						<p>{restoreResultLine(restored)}</p>
						<ul class="grid gap-1 text-xs">
							{#each kindCountLines(restored.countByKind) as line, index (index)}
								<li>{line}</li>
							{/each}
						</ul>
					{/if}
				</AlertDescription>
			</Alert>
		{/if}

		<DialogFooter>
			{#if step === 'choose'}
				<Button variant="outline" onclick={clear} disabled={restoring}>Cancel</Button>
				<Button
					onclick={inspect}
					disabled={sourcePath === '' || restorePassword.length === 0 || restoring}
				>
					{restoring ? 'Reading…' : 'Read backup'}
				</Button>
			{:else if step === 'preview'}
				<Button variant="outline" onclick={clear} disabled={restoring}>Cancel</Button>
				<Button onclick={askForConfirmation} disabled={restoring}>Continue</Button>
			{:else if step === 'confirm'}
				<Button variant="outline" onclick={() => (step = 'preview')} disabled={restoring}
					>Back</Button
				>
				<Button variant="destructive" onclick={runRestore} disabled={!canRestore}>
					{restoring ? 'Restoring…' : 'Replace this vault'}
				</Button>
			{:else}
				<Button onclick={clear} disabled={restoring}>Close</Button>
			{/if}
		</DialogFooter>
	</DialogContent>
</Dialog>
