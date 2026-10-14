<script lang="ts">
	import { onMount } from 'svelte';

	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import {
		Card,
		CardContent,
		CardDescription,
		CardHeader,
		CardTitle
	} from '#lib/components/ui/card';
	import { Input } from '#lib/components/ui/input';
	import { Label } from '#lib/components/ui/label';
	import { errorMessage } from '#lib/errors';
	import {
		createPasswordProblem,
		createVault,
		lockVault,
		unlockVault,
		vaultStatus,
		vaultStatusLabel,
		type VaultStatus
	} from '#lib/vault';

	// The page owns the status so it can decide whether the saved inventory is
	// on screen; locking here takes that inventory away there.
	let {
		status = $bindable<VaultStatus | undefined>(undefined)
	}: {
		status?: VaultStatus | undefined;
	} = $props();

	// The password lives here and nowhere else: not in a store, not in local
	// storage, and it is cleared as soon as a call is done with it.
	let password = $state('');
	let confirmation = $state('');
	let busy = $state(false);
	let errorText = $state('');

	onMount(() => {
		void refresh();
	});

	async function refresh() {
		try {
			status = await vaultStatus();
		} catch (error) {
			errorText = errorMessage(error);
		}
	}

	function clearForm() {
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
			status = await createVault(password);
		} catch (error) {
			errorText = errorMessage(error);
		} finally {
			clearForm();
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
			status = await unlockVault(password);
		} catch (error) {
			errorText = errorMessage(error);
		} finally {
			clearForm();
			busy = false;
		}
	}

	async function lock() {
		errorText = '';
		busy = true;
		try {
			status = await lockVault();
		} catch (error) {
			errorText = errorMessage(error);
		} finally {
			clearForm();
			busy = false;
		}
	}
</script>

<Card>
	<CardHeader>
		<CardTitle>
			Local vault
			<span class="ml-2 text-xs font-normal text-muted-foreground">
				{status === undefined ? 'Checking…' : vaultStatusLabel(status)}
			</span>
		</CardTitle>
		<CardDescription>
			Secrets are encrypted on this device with your master password. The password cannot be
			recovered.
		</CardDescription>
	</CardHeader>
	<CardContent class="grid gap-3">
		{#if status === undefined}
			<p class="text-sm text-muted-foreground">Checking the vault…</p>
		{:else if status === 'uninitialized'}
			<form class="grid gap-3 sm:grid-cols-2" onsubmit={submitCreate}>
				<div class="grid gap-1.5 sm:col-span-2">
					<Label for="vault-password">Master password</Label>
					<Input
						id="vault-password"
						type="password"
						autocomplete="new-password"
						bind:value={password}
						disabled={busy}
					/>
				</div>
				<div class="grid gap-1.5 sm:col-span-2">
					<Label for="vault-confirmation">Confirm master password</Label>
					<Input
						id="vault-confirmation"
						type="password"
						autocomplete="new-password"
						bind:value={confirmation}
						disabled={busy}
					/>
				</div>
				<div class="sm:col-span-2">
					<Button type="submit" disabled={busy}>Create Vault</Button>
				</div>
			</form>
		{:else if status === 'locked'}
			<form class="grid gap-3 sm:grid-cols-2" onsubmit={submitUnlock}>
				<p class="text-sm text-muted-foreground sm:col-span-2">
					This vault is stored locally on this device.
				</p>
				<div class="grid gap-1.5 sm:col-span-2">
					<Label for="vault-unlock-password">Master password</Label>
					<Input
						id="vault-unlock-password"
						type="password"
						autocomplete="current-password"
						bind:value={password}
						disabled={busy}
					/>
				</div>
				<div class="sm:col-span-2">
					<Button type="submit" disabled={busy}>Unlock</Button>
				</div>
			</form>
		{:else}
			<div class="grid gap-2">
				<p class="text-sm font-medium">Vault Unlocked</p>
				<p class="text-xs text-muted-foreground">
					Saved hosts and credentials live in this vault; manage them below.
				</p>
				<div>
					<Button variant="outline" onclick={lock} disabled={busy}>Lock</Button>
				</div>
			</div>
		{/if}

		{#if errorText}
			<Alert variant="destructive">
				<AlertTitle>Vault error</AlertTitle>
				<AlertDescription>{errorText}</AlertDescription>
			</Alert>
		{/if}
	</CardContent>
</Card>
