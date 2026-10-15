<script lang="ts">
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import Loader2Icon from '@lucide/svelte/icons/loader-2';
	import LockIcon from '@lucide/svelte/icons/lock';
	import ShieldAlertIcon from '@lucide/svelte/icons/shield-alert';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import ZapIcon from '@lucide/svelte/icons/zap';

	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
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
		class="border-border/70 p-4 shadow-xl sm:max-w-md"
		interactOutsideBehavior="ignore"
		escapeKeydownBehavior="ignore"
	>
		{#if status === undefined}
			<!-- Checking status or check failure -->
			<DialogHeader>
				<div
					class="mb-3 flex size-11 items-center justify-center rounded-xl border shadow-xs {checkError
						? 'border-destructive/20 bg-destructive/10 text-destructive'
						: 'border-border/60 bg-muted/40 text-muted-foreground'}"
				>
					{#if checkError}
						<ShieldAlertIcon class="size-5" />
					{:else}
						<Loader2Icon class="size-5 animate-spin" />
					{/if}
				</div>
				<p class="text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
					Vault Status
				</p>
				<DialogTitle class="text-base font-semibold tracking-tight text-foreground">
					{checkError ? 'Vault check failed' : 'Checking vault status…'}
				</DialogTitle>
				<DialogDescription class="mt-1 text-xs leading-relaxed text-muted-foreground">
					{checkError
						? 'Termust could not check the local vault status. Check your connection or retry.'
						: 'Please wait while Termust inspects the local vault.'}
				</DialogDescription>
			</DialogHeader>

			{#if checkError}
				<Alert variant="destructive" class="border-destructive/20 bg-destructive/10">
					<AlertTitle>Error</AlertTitle>
					<AlertDescription>{checkError}</AlertDescription>
				</Alert>

				<div class="pt-2">
					{#if onretry}
						<Button
							onclick={onretry}
							class="h-9 w-full bg-blue-600 px-4 text-xs font-semibold text-white hover:bg-blue-500"
						>
							Retry
						</Button>
					{/if}
				</div>
			{/if}
		{:else if status === 'uninitialized'}
			<!-- Create Vault Flow -->
			<DialogHeader>
				<div
					class="mb-3 flex size-11 items-center justify-center rounded-xl border border-blue-500/20 bg-blue-500/10 text-blue-500 shadow-xs"
				>
					<ShieldCheckIcon class="size-5" />
				</div>
				<p class="text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
					Vault Setup
				</p>
				<DialogTitle class="text-base font-semibold tracking-tight text-foreground">
					Create your local vault
				</DialogTitle>
				<DialogDescription class="mt-1 text-xs leading-relaxed text-muted-foreground">
					Saved hosts and credentials require an encrypted local vault. Choose a strong master
					password. It cannot be recovered if forgotten.
				</DialogDescription>
			</DialogHeader>

			<form class="grid gap-3.5" onsubmit={submitCreate}>
				<div class="grid gap-1.5">
					<Label for="create-vault-password" class="text-xs font-medium text-muted-foreground">
						Master password
					</Label>
					<div class="relative">
						<KeyRoundIcon
							class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
						/>
						<Input
							id="create-vault-password"
							type="password"
							autocomplete="new-password"
							bind:value={password}
							disabled={busy}
							placeholder="Choose a master password"
							class="h-9 border-border/60 bg-muted/30 pl-8 font-mono text-xs focus-visible:ring-1"
							required
						/>
					</div>
				</div>
				<div class="grid gap-1.5">
					<Label for="create-vault-confirmation" class="text-xs font-medium text-muted-foreground">
						Confirm master password
					</Label>
					<div class="relative">
						<LockIcon
							class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
						/>
						<Input
							id="create-vault-confirmation"
							type="password"
							autocomplete="new-password"
							bind:value={confirmation}
							disabled={busy}
							placeholder="Re-enter master password"
							class="h-9 border-border/60 bg-muted/30 pl-8 font-mono text-xs focus-visible:ring-1"
							required
						/>
					</div>
				</div>

				{#if errorText}
					<Alert variant="destructive" class="border-destructive/20 bg-destructive/10 text-xs">
						<AlertTitle>Create failed</AlertTitle>
						<AlertDescription>{errorText}</AlertDescription>
					</Alert>
				{/if}

				<div class="flex flex-col gap-2 pt-1.5">
					<Button
						type="submit"
						class="h-9 w-full bg-blue-600 px-4 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-blue-500"
						disabled={busy}
					>
						{#if busy}
							<Loader2Icon class="mr-1.5 size-3.5 animate-spin" />
						{/if}
						Create Vault
					</Button>
					<Button
						type="button"
						variant="outline"
						class="h-9 w-full border-border/70 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
						onclick={handleContinue}
						disabled={busy}
					>
						<ZapIcon data-icon="inline-start" class="size-3.5" />
						Continue with Quick Connect
					</Button>
				</div>
			</form>
		{:else if status === 'locked'}
			<!-- Unlock Vault Flow -->
			<DialogHeader>
				<DialogTitle class="text-base font-semibold tracking-tight text-foreground">
					Unlock local vault
				</DialogTitle>
				<DialogDescription class="text-xs leading-relaxed text-muted-foreground">
					Enter your master password to access encrypted hosts and credentials.
				</DialogDescription>
			</DialogHeader>

			<form class="grid gap-3.5" onsubmit={submitUnlock}>
				<div class="grid gap-1.5">
					<Label for="unlock-vault-password" class="text-xs font-medium text-muted-foreground">
						Master password
					</Label>
					<div class="relative">
						<KeyRoundIcon
							class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
						/>
						<Input
							id="unlock-vault-password"
							type="password"
							autocomplete="current-password"
							bind:value={password}
							disabled={busy}
							placeholder="Enter master password"
							class="h-9 border-border/60 bg-muted/30 pl-8 font-mono text-xs focus-visible:ring-1"
							required
						/>
					</div>
				</div>

				{#if errorText}
					<Alert variant="destructive" class="border-destructive/20 bg-destructive/10 text-xs">
						<AlertTitle>Unlock failed</AlertTitle>
						<AlertDescription>{errorText}</AlertDescription>
					</Alert>
				{/if}

				<div class="flex flex-col gap-2 pt-1.5">
					<Button
						type="submit"
						class="h-9 w-full bg-blue-600 px-4 text-xs font-semibold text-white shadow-xs transition-colors hover:bg-blue-500"
						disabled={busy}
					>
						{#if busy}
							<Loader2Icon class="mr-1.5 size-3.5 animate-spin" />
						{/if}
						Unlock Vault
					</Button>
					<Button
						type="button"
						variant="outline"
						class="h-9 w-full border-border/70 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
						onclick={handleContinue}
						disabled={busy}
					>
						<ZapIcon data-icon="inline-start" class="size-3.5" />
						Continue with Quick Connect
					</Button>
				</div>
			</form>
		{/if}
	</DialogContent>
</Dialog>
