<script lang="ts">
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import ZapIcon from '@lucide/svelte/icons/zap';

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
	import { createPasswordProblem, createVault, unlockVault, type VaultStatus } from '#lib/vault';

	let {
		status = undefined,
		open = false,
		checkError = '',
		onunlocked,
		oncontinue,
		onretry
	}: {
		status?: VaultStatus | undefined;
		open: boolean;
		checkError?: string;
		onunlocked: (status: VaultStatus) => void;
		oncontinue: () => void;
		onretry?: () => void;
	} = $props();

	// Passwords live only here in component state and are cleared in finally blocks.
	let password = $state('');
	let confirmation = $state('');
	let busy = $state(false);
	let errorText = $state('');

	function clearPasswords() {
		password = '';
		confirmation = '';
	}

	async function submitCreate(event: SubmitEvent) {
		event.preventDefault();
		errorText = '';

		const problem = createPasswordProblem(password, confirmation);
		if (problem) {
			errorText = problem;
			return;
		}

		busy = true;
		try {
			const nextStatus = await createVault(password);
			clearPasswords();
			onunlocked(nextStatus);
		} catch (error) {
			clearPasswords();
			errorText = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	async function submitUnlock(event: SubmitEvent) {
		event.preventDefault();
		errorText = '';

		if (password.length === 0) {
			errorText = 'Enter your master password.';
			return;
		}

		busy = true;
		try {
			const nextStatus = await unlockVault(password);
			clearPasswords();
			onunlocked(nextStatus);
		} catch (error) {
			clearPasswords();
			errorText = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	function handleContinue() {
		clearPasswords();
		errorText = '';
		oncontinue();
	}
</script>

<Dialog {open}>
	<DialogContent
		showCloseButton={false}
		overlayClass="bg-black/60 backdrop-blur-sm"
		class="sm:max-w-md"
		interactOutsideBehavior="ignore"
		escapeKeydownBehavior="ignore"
	>
		{#if status === undefined}
			<!-- Checking status or check failure -->
			<DialogHeader>
				<div class="mb-2 flex size-10 items-center justify-center rounded-full bg-muted">
					{#if checkError}
						<ShieldAlertIcon class="size-5 text-destructive" />
					{:else}
						<Loader2Icon class="size-5 animate-spin text-muted-foreground" />
					{/if}
				</div>
				<DialogTitle>
					{checkError ? 'Vault check failed' : 'Checking vault status…'}
				</DialogTitle>
				<DialogDescription>
					{checkError
						? 'Termust could not check the local vault status. Check your connection or retry.'
						: 'Please wait while Termust inspects the local vault.'}
				</DialogDescription>
			</DialogHeader>

			{#if checkError}
				<Alert variant="destructive">
					<AlertTitle>Error</AlertTitle>
					<AlertDescription>{checkError}</AlertDescription>
				</Alert>

				<DialogFooter class="flex flex-row justify-end gap-2">
					{#if onretry}
						<Button onclick={onretry} class="text-xs">Retry</Button>
					{/if}
				</DialogFooter>
			{/if}
		{:else if status === 'uninitialized'}
			<!-- Create Vault Flow -->
			<DialogHeader>
				<div class="mb-2 flex size-10 items-center justify-center rounded-full bg-primary/10">
					<ShieldCheckIcon class="size-5 text-primary" />
				</div>
				<DialogTitle>Create your local vault</DialogTitle>
				<DialogDescription>
					Saved hosts and credentials require an encrypted local vault. Choose a strong master
					password. It cannot be recovered if forgotten.
				</DialogDescription>
			</DialogHeader>

			<form class="grid gap-4" onsubmit={submitCreate}>
				<div class="grid gap-1.5">
					<Label for="create-vault-password">Master password</Label>
					<Input
						id="create-vault-password"
						type="password"
						autocomplete="new-password"
						bind:value={password}
						disabled={busy}
						required
					/>
				</div>
				<div class="grid gap-1.5">
					<Label for="create-vault-confirmation">Confirm master password</Label>
					<Input
						id="create-vault-confirmation"
						type="password"
						autocomplete="new-password"
						bind:value={confirmation}
						disabled={busy}
						required
					/>
				</div>

				{#if errorText}
					<Alert variant="destructive">
						<AlertTitle>Create failed</AlertTitle>
						<AlertDescription>{errorText}</AlertDescription>
					</Alert>
				{/if}

				<DialogFooter class="flex flex-col-reverse gap-2 sm:flex-row sm:justify-between sm:gap-2">
					<Button
						type="button"
						variant="outline"
						class="text-xs"
						onclick={handleContinue}
						disabled={busy}
					>
						<ZapIcon data-icon="inline-start" />
						Continue with Quick Connect
					</Button>
					<Button type="submit" class="text-xs" disabled={busy}>
						{#if busy}
							<Loader2Icon class="mr-1.5 size-3.5 animate-spin" />
						{/if}
						Create Vault
					</Button>
				</DialogFooter>
			</form>
		{:else if status === 'locked'}
			<!-- Unlock Vault Flow -->
			<DialogHeader>
				<div class="mb-2 flex size-10 items-center justify-center rounded-full bg-primary/10">
					<ShieldCheckIcon class="size-5 text-primary" />
				</div>
				<DialogTitle>Unlock local vault</DialogTitle>
				<DialogDescription>
					Enter your master password to access encrypted saved hosts and credentials.
				</DialogDescription>
			</DialogHeader>

			<form class="grid gap-4" onsubmit={submitUnlock}>
				<div class="grid gap-1.5">
					<Label for="unlock-vault-password">Master password</Label>
					<Input
						id="unlock-vault-password"
						type="password"
						autocomplete="current-password"
						bind:value={password}
						disabled={busy}
						required
					/>
				</div>

				{#if errorText}
					<Alert variant="destructive">
						<AlertTitle>Unlock failed</AlertTitle>
						<AlertDescription>{errorText}</AlertDescription>
					</Alert>
				{/if}

				<DialogFooter class="flex flex-col-reverse gap-2 sm:flex-row sm:justify-between sm:gap-2">
					<Button
						type="button"
						variant="outline"
						class="text-xs"
						onclick={handleContinue}
						disabled={busy}
					>
						<ZapIcon data-icon="inline-start" />
						Continue with Quick Connect
					</Button>
					<Button type="submit" class="text-xs" disabled={busy}>
						{#if busy}
							<Loader2Icon class="mr-1.5 size-3.5 animate-spin" />
						{/if}
						Unlock
					</Button>
				</DialogFooter>
			</form>
		{/if}
	</DialogContent>
</Dialog>
