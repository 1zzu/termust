<script lang="ts">
	import ArrowRightToLineIcon from '@lucide/svelte/icons/arrow-right-to-line';
	import CheckCircle2Icon from '@lucide/svelte/icons/check-circle-2';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import DownloadIcon from '@lucide/svelte/icons/download';

	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import { Label } from '#lib/components/ui/label';
	import {
		chooseBackupToSave,
		createdAtLabel,
		defaultBackupFileName,
		exportEncryptedBackup,
		exportPasswordProblem,
		exportReady,
		exportRequest,
		exportWarningLines,
		formatBytes,
		summaryLine,
		type ExportSummary
	} from '#lib/backup';
	import { errorMessage } from '#lib/errors';
	import type { VaultStatus } from '#lib/vault';

	let {
		vaultStatus,
		busy = false,
		onclose
	}: {
		vaultStatus?: VaultStatus;
		busy?: boolean;
		onclose: () => void;
	} = $props();

	let exportPassword = $state('');
	let exportConfirmation = $state('');
	let exportFailure = $state('');
	let exported = $state<ExportSummary | null>(null);
	let exporting = $state(false);

	const exportProblem = $derived(exportPasswordProblem(exportPassword, exportConfirmation));
	const canExport = $derived(exportReady(exportPassword, exportConfirmation, exporting));

	async function runExport() {
		if (!canExport) return;

		exporting = true;
		exportFailure = '';
		try {
			const destinationPath = await chooseBackupToSave(defaultBackupFileName());
			if (destinationPath === null) {
				return;
			}
			exported = await exportEncryptedBackup(exportRequest(destinationPath, exportPassword));
			exportPassword = '';
			exportConfirmation = '';
		} catch (error) {
			exportFailure = errorMessage(error);
		} finally {
			exporting = false;
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
	aria-label="Export Backup"
>
	<!-- Header -->
	<header class="flex items-start justify-between gap-2 pt-0.5">
		<div class="flex flex-col gap-0.5">
			<h3 class="text-sm font-semibold text-foreground">Export Vault Backup</h3>
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
			aria-label="Close export panel"
			title="Close panel"
		>
			<ArrowRightToLineIcon class="size-3.5" />
		</Button>
	</header>

	<!-- Body -->
	<div class="flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5 sm:gap-5">
		{#if exported === null}
			<!-- Warnings & Information -->
			<div class="rounded-md border border-border/60 bg-muted/20 p-3">
				<p class="text-xs font-medium text-foreground">Encrypted with your chosen password</p>
				<ul class="mt-2 flex flex-col gap-1.5 text-[11px] text-muted-foreground">
					{#each exportWarningLines() as line, index (index)}
						<li class="flex items-start gap-1.5">
							<span class="text-muted-foreground">•</span>
							<span>{line}</span>
						</li>
					{/each}
				</ul>
			</div>

			<!-- Password inputs -->
			<div class="flex flex-col gap-3">
				<div class="flex flex-col gap-1.5">
					<Label for="panel-backup-password" class="text-xs font-medium text-muted-foreground">
						Backup password
					</Label>
					<Input
						id="panel-backup-password"
						type="password"
						autocomplete="new-password"
						spellcheck={false}
						bind:value={exportPassword}
						placeholder="Minimum 12 characters"
						disabled={exporting}
						class="h-9 border-border/60 bg-muted/30 font-mono text-xs"
					/>
				</div>

				<div class="flex flex-col gap-1.5">
					<Label
						for="panel-backup-password-confirmation"
						class="text-xs font-medium text-muted-foreground"
					>
						Confirm backup password
					</Label>
					<Input
						id="panel-backup-password-confirmation"
						type="password"
						autocomplete="new-password"
						spellcheck={false}
						bind:value={exportConfirmation}
						placeholder="Repeat your password"
						disabled={exporting}
						class="h-9 border-border/60 bg-muted/30 font-mono text-xs"
					/>
				</div>

				{#if exportPassword.length > 0 && exportConfirmation.length > 0 && exportProblem}
					<Alert variant="destructive" class="py-2 text-xs">
						<AlertDescription>{exportProblem}</AlertDescription>
					</Alert>
				{/if}

				{#if exportFailure !== ''}
					<Alert variant="destructive" class="py-2 text-xs">
						<AlertTitle>Export failed</AlertTitle>
						<AlertDescription>{exportFailure}</AlertDescription>
					</Alert>
				{/if}
			</div>
		{:else}
			<!-- Success state -->
			<div
				class="flex flex-col gap-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 p-3"
			>
				<div class="flex items-center gap-2 text-emerald-600 dark:text-emerald-400">
					<CheckCircle2Icon class="size-4" />
					<span class="text-xs font-semibold">Backup created successfully</span>
				</div>
				<div class="text-[11px] text-muted-foreground">
					<p class="font-mono text-foreground">{exported.fileName}</p>
					<p class="mt-1">{formatBytes(exported.bytes)} • {summaryLine(exported)}</p>
					<p class="mt-0.5">Created {createdAtLabel(exported.createdAt)}</p>
				</div>
			</div>
		{/if}
	</div>

	<!-- Footer -->
	<footer>
		{#if exported === null}
			<Button
				class="h-10 w-full bg-blue-600 text-xs font-semibold text-white shadow-sm hover:bg-blue-500"
				onclick={runExport}
				disabled={!canExport || exporting || busy}
			>
				<DownloadIcon class="mr-1.5 size-3.5" />
				<span>{exporting ? 'Encrypting & saving…' : 'Choose location and export'}</span>
			</Button>
		{:else}
			<Button
				variant="outline"
				class="h-10 w-full text-xs font-medium"
				onclick={() => {
					exported = null;
					onclose();
				}}
			>
				Done
			</Button>
		{/if}
	</footer>
</aside>
