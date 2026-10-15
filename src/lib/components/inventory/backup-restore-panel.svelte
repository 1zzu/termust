<script lang="ts">
	import AlertTriangleIcon from '@lucide/svelte/icons/alert-triangle';
	import ArrowRightToLineIcon from '@lucide/svelte/icons/arrow-right-to-line';
	import CheckCircle2Icon from '@lucide/svelte/icons/check-circle-2';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import UploadIcon from '@lucide/svelte/icons/upload';

	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import { Label } from '#lib/components/ui/label';
	import {
		chooseBackupToOpen,
		createdAtLabel,
		fileNameOf,
		inspectEncryptedBackup,
		inspectRequest,
		kindCountLines,
		RESTORE_PHRASE,
		restoreEncryptedBackup,
		restoreProblem,
		restoreReady,
		restoreRequest,
		restoreResultLine,
		restoreWarningLines,
		summaryLine,
		type BackupSummary,
		type RestoreSummary
	} from '#lib/backup';
	import { errorMessage } from '#lib/errors';
	import type { VaultStatus } from '#lib/vault';

	let {
		vaultStatus,
		hasLiveSessions = false,
		busy = false,
		onrestored,
		onclose
	}: {
		vaultStatus?: VaultStatus;
		hasLiveSessions?: boolean;
		busy?: boolean;
		onrestored?: () => void | Promise<void>;
		onclose: () => void;
	} = $props();

	type Step = 'choose' | 'preview' | 'confirm' | 'result';

	let step = $state<Step>('choose');
	let sourcePath = $state('');
	let restorePassword = $state('');
	let preview = $state<BackupSummary | null>(null);
	let typed = $state('');
	let restoreFailure = $state('');
	let restored = $state<RestoreSummary | null>(null);
	let restoring = $state(false);

	const sourceName = $derived(sourcePath === '' ? '' : fileNameOf(sourcePath));
	const restoreProblemText = $derived(restoreProblem(typed, hasLiveSessions, restoring));
	const canRestore = $derived(restoreReady(typed, hasLiveSessions, restoring));

	async function chooseFile() {
		restoreFailure = '';
		try {
			const chosen = await chooseBackupToOpen();
			if (chosen === null) return;
			sourcePath = chosen;
			preview = null;
			restored = null;
			step = 'choose';
		} catch (error) {
			restoreFailure = errorMessage(error);
		}
	}

	async function inspect() {
		if (sourcePath === '' || restorePassword.length === 0 || restoring) return;

		restoring = true;
		restoreFailure = '';
		try {
			preview = await inspectEncryptedBackup(inspectRequest(sourcePath, restorePassword));
			step = 'preview';
		} catch (error) {
			preview = null;
			restoreFailure = errorMessage(error);
		} finally {
			restoring = false;
		}
	}

	function askForConfirmation() {
		if (preview === null) return;
		typed = '';
		step = 'confirm';
	}

	async function runRestore() {
		if (!canRestore || sourcePath === '' || preview === null) return;

		restoring = true;
		restoreFailure = '';
		try {
			restored = await restoreEncryptedBackup(
				restoreRequest(sourcePath, restorePassword, typed, preview.fileDigest)
			);
			restorePassword = '';
			typed = '';
			step = 'result';
			await onrestored?.();
		} catch (error) {
			restoreFailure = errorMessage(error);
		} finally {
			restoring = false;
		}
	}

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
	aria-label="Restore Backup"
>
	<!-- Header -->
	<header class="flex items-start justify-between gap-2 pt-0.5">
		<div class="flex flex-col gap-0.5">
			<h3 class="text-sm font-semibold text-foreground">Restore from Backup</h3>
			<div class="flex items-center gap-1 text-[11px] text-muted-foreground">
				<span>{vaultStatus === 'unlocked' ? 'Personal vault' : 'Local vault'}</span>
				<ChevronDownIcon class="size-3 opacity-60" />
			</div>
		</div>

		<Button
			variant="ghost"
			size="icon"
			class="size-7 text-muted-foreground hover:text-foreground"
			onclick={onclose}
			aria-label="Close restore panel"
			title="Close panel"
		>
			<ArrowRightToLineIcon class="size-3.5" />
		</Button>
	</header>

	<!-- Body -->
	<div class="flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5 sm:gap-5">
		{#if step === 'choose' || step === 'preview'}
			<!-- Choose File and Enter Password -->
			<div class="flex flex-col gap-3">
				<div class="flex flex-col gap-1.5">
					<Label for="panel-backup-file" class="text-xs font-medium text-muted-foreground">
						Backup file
					</Label>
					<div class="flex items-center gap-2">
						<Button
							variant="outline"
							size="sm"
							class="h-8 shrink-0 text-xs"
							onclick={chooseFile}
							disabled={restoring}
						>
							Choose file
						</Button>
						<span class="truncate font-mono text-xs text-muted-foreground">
							{sourceName === '' ? 'No file chosen' : sourceName}
						</span>
					</div>
				</div>

				<div class="flex flex-col gap-1.5">
					<Label for="panel-restore-password" class="text-xs font-medium text-muted-foreground">
						Backup password
					</Label>
					<Input
						id="panel-restore-password"
						type="password"
						autocomplete="off"
						spellcheck={false}
						bind:value={restorePassword}
						placeholder="Enter password set during export"
						disabled={restoring}
						class="h-9 border-border/60 bg-muted/30 font-mono text-xs"
					/>
				</div>

				{#if restoreFailure !== ''}
					<Alert variant="destructive" class="py-2 text-xs">
						<AlertTitle>Cannot read backup</AlertTitle>
						<AlertDescription>{restoreFailure}</AlertDescription>
					</Alert>
				{/if}

				{#if step === 'preview' && preview !== null}
					<!-- Decrypted Preview Info -->
					<div class="flex flex-col gap-2 rounded-md border border-border/60 bg-muted/30 p-3">
						<p class="text-xs font-semibold text-foreground">{summaryLine(preview)}</p>
						<ul class="flex flex-col gap-1 text-[11px] text-muted-foreground">
							{#each kindCountLines(preview.countByKind) as line, index (index)}
								<li class="flex items-center gap-1.5">
									<span class="size-1 rounded-full bg-primary/60"></span>
									<span>{line}</span>
								</li>
							{/each}
						</ul>
						<p class="mt-1 text-[10px] text-muted-foreground/80">
							Created {createdAtLabel(preview.createdAt)} (Termust {preview.sourceAppVersion}).
						</p>
					</div>
				{/if}
			</div>
		{:else if step === 'confirm'}
			<!-- Destructive Confirmation Step -->
			<div class="flex flex-col gap-3">
				<Alert variant="destructive" class="py-2.5">
					<AlertTitle class="flex items-center gap-1.5 text-xs font-semibold">
						<AlertTriangleIcon class="size-3.5" />
						<span>Warning: Replaces Entire Vault</span>
					</AlertTitle>
					<AlertDescription class="mt-1 text-[11px]">
						<ul class="flex flex-col gap-1">
							{#each restoreWarningLines() as line, index (index)}
								<li>• {line}</li>
							{/each}
						</ul>
					</AlertDescription>
				</Alert>

				<div class="flex flex-col gap-1.5">
					<Label for="panel-restore-confirm" class="text-xs font-medium text-muted-foreground">
						Type <strong class="font-mono text-foreground">{RESTORE_PHRASE}</strong> to confirm
					</Label>
					<Input
						id="panel-restore-confirm"
						autocomplete="off"
						spellcheck={false}
						placeholder={RESTORE_PHRASE}
						bind:value={typed}
						disabled={restoring}
						class="h-9 border-border/60 bg-muted/30 font-mono text-xs uppercase"
					/>
					{#if restoreProblemText && typed.length > 0}
						<p class="text-[11px] text-muted-foreground">{restoreProblemText}</p>
					{/if}
				</div>

				{#if restoreFailure !== ''}
					<Alert variant="destructive" class="py-2 text-xs">
						<AlertTitle>Restore failed</AlertTitle>
						<AlertDescription>{restoreFailure}</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else if step === 'result'}
			<!-- Success state -->
			<div
				class="flex flex-col gap-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 p-3"
			>
				<div class="flex items-center gap-2 text-emerald-600 dark:text-emerald-400">
					<CheckCircle2Icon class="size-4" />
					<span class="text-xs font-semibold">Vault restored successfully</span>
				</div>
				{#if restored !== null}
					<div class="text-[11px] text-muted-foreground">
						<p class="font-medium text-foreground">{restoreResultLine(restored)}</p>
						<ul class="mt-1.5 flex flex-col gap-1">
							{#each kindCountLines(restored.countByKind) as line, index (index)}
								<li>• {line}</li>
							{/each}
						</ul>
					</div>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Footer -->
	<footer>
		{#if step === 'choose'}
			<Button
				class="h-10 w-full bg-blue-600 text-xs font-semibold text-white shadow-sm hover:bg-blue-500"
				onclick={inspect}
				disabled={sourcePath === '' || restorePassword.length === 0 || restoring || busy}
			>
				<span>{restoring ? 'Reading backup…' : 'Read & inspect backup'}</span>
			</Button>
		{:else if step === 'preview'}
			<div class="flex gap-2">
				<Button
					variant="outline"
					class="h-10 flex-1 text-xs"
					onclick={() => (step = 'choose')}
					disabled={restoring}
				>
					Back
				</Button>
				<Button
					class="h-10 flex-1 bg-blue-600 text-xs font-semibold text-white shadow-sm hover:bg-blue-500"
					onclick={askForConfirmation}
					disabled={restoring}
				>
					Continue
				</Button>
			</div>
		{:else if step === 'confirm'}
			<div class="flex gap-2">
				<Button
					variant="outline"
					class="h-10 flex-1 text-xs"
					onclick={() => (step = 'preview')}
					disabled={restoring}
				>
					Back
				</Button>
				<Button
					variant="destructive"
					class="h-10 flex-1 text-xs font-semibold"
					onclick={runRestore}
					disabled={!canRestore || restoring}
				>
					{restoring ? 'Restoring…' : 'Replace vault'}
				</Button>
			</div>
		{:else}
			<Button
				variant="outline"
				class="h-10 w-full text-xs font-medium"
				onclick={() => {
					step = 'choose';
					onclose();
				}}
			>
				Done
			</Button>
		{/if}
	</footer>
</aside>
