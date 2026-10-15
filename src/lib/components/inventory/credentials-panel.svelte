<script lang="ts">
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
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue
	} from '#lib/components/ui/select';
	import { Textarea } from '#lib/components/ui/textarea';
	import {
		createPasswordCredential,
		createPrivateKeyCredential,
		credentialFormProblem,
		deleteCredential,
		emptyCredentialForm,
		renameCredential,
		replacePassword,
		replacePrivateKey,
		type CredentialFormMode,
		type CredentialFormValues,
		type CredentialSummary
	} from '#lib/inventory';
	import type { InventoryPageProps } from './page-props';
	import PageShell from './page-shell.svelte';

	/**
	 * Credentials page: the saved secrets, and the create, rename, replace and
	 * delete flows behind their dialog.
	 *
	 * A typed secret lives in this component only, and only until the dialog
	 * closes: nothing here can read one back.
	 */
	let {
		credentials,
		busy,
		onmutate,
		onerror
	}: InventoryPageProps & {
		credentials: CredentialSummary[];
	} = $props();

	// Credential dialog: create, rename, or replace the secret.
	let credentialOpen = $state(false);
	let credentialMode = $state<CredentialFormMode>('create');
	let credentialTarget = $state<CredentialSummary | null>(null);
	let credentialForm = $state<CredentialFormValues>(emptyCredentialForm());

	const credentialKinds = [
		{ value: 'password', label: 'Password' },
		{ value: 'private_key', label: 'Private key' }
	];

	function openCreateCredential() {
		credentialMode = 'create';
		credentialTarget = null;
		credentialForm = emptyCredentialForm();
		credentialOpen = true;
	}

	function openRenameCredential(credential: CredentialSummary) {
		credentialMode = 'rename';
		credentialTarget = credential;
		credentialForm = { ...emptyCredentialForm(), name: credential.name, kind: credential.kind };
		credentialOpen = true;
	}

	function openReplaceSecret(credential: CredentialSummary) {
		credentialMode = 'replace';
		credentialTarget = credential;
		// The kind is fixed here: replacing a secret does not turn a password
		// into a key.
		credentialForm = { ...emptyCredentialForm(), kind: credential.kind };
		credentialOpen = true;
	}

	async function submitCredential(event: SubmitEvent) {
		event.preventDefault();
		const problem = credentialFormProblem(credentialMode, credentialForm);
		if (problem) {
			onerror(problem);
			clearSecret();
			return;
		}

		const target = credentialTarget;
		const { name, kind, secret, passphrase } = credentialForm;
		const optionalPassphrase = passphrase === '' ? null : passphrase;

		const stored = await onmutate(async () => {
			if (credentialMode === 'create' && kind === 'password') {
				await createPasswordCredential(name.trim(), secret);
			} else if (credentialMode === 'create') {
				await createPrivateKeyCredential(name.trim(), secret, optionalPassphrase);
			} else if (credentialMode === 'rename' && target) {
				await renameCredential(target.id, name.trim());
			} else if (target && kind === 'password') {
				await replacePassword(target.id, secret);
			} else if (target) {
				await replacePrivateKey(target.id, secret, optionalPassphrase);
			}
			closeCredentialDialog();
		});

		// The secret does not stay in the form after a failed submit either.
		if (!stored) {
			clearSecret();
		}
	}

	function closeCredentialDialog() {
		credentialOpen = false;
		credentialTarget = null;
		credentialForm = emptyCredentialForm();
	}

	function clearSecret() {
		credentialForm.secret = '';
		credentialForm.passphrase = '';
	}

	async function removeCredential(credential: CredentialSummary) {
		await onmutate(() => deleteCredential(credential.id));
	}
</script>

<PageShell title="Credentials">
	{#snippet actions()}
		<Button size="sm" onclick={openCreateCredential} disabled={busy}>New credential</Button>
	{/snippet}
	{#if credentials.length === 0}
		<p class="text-xs text-muted-foreground">No saved credentials yet.</p>
	{:else}
		<ul class="grid gap-2">
			{#each credentials as credential (credential.id)}
				<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
					<div class="min-w-0">
						<p class="truncate text-sm font-medium">{credential.name}</p>
						<p class="truncate text-xs text-muted-foreground">
							{credential.kind === 'password' ? 'Password' : 'Private key'}
						</p>
					</div>
					<div class="flex shrink-0 gap-2">
						<Button
							variant="outline"
							size="sm"
							onclick={() => openRenameCredential(credential)}
							disabled={busy}
						>
							Rename
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => openReplaceSecret(credential)}
							disabled={busy}
						>
							Replace secret
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => removeCredential(credential)}
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

<Dialog bind:open={credentialOpen}>
	<DialogContent class="sm:max-w-md">
		<DialogHeader>
			<DialogTitle>
				{credentialMode === 'create'
					? 'New credential'
					: credentialMode === 'rename'
						? 'Rename credential'
						: 'Replace secret'}
			</DialogTitle>
			<DialogDescription>
				{#if credentialMode === 'rename'}
					The stored secret is left exactly as it is.
				{:else}
					The secret is encrypted into the vault and is never shown again.
				{/if}
			</DialogDescription>
		</DialogHeader>

		<form class="grid gap-3" onsubmit={submitCredential}>
			{#if credentialMode !== 'replace'}
				<div class="grid gap-1.5">
					<Label for="credential-name">Name</Label>
					<Input id="credential-name" bind:value={credentialForm.name} disabled={busy} />
				</div>
			{/if}

			{#if credentialMode === 'create'}
				<div class="grid gap-1.5">
					<Label for="credential-kind">Type</Label>
					<Select
						type="single"
						bind:value={credentialForm.kind}
						items={credentialKinds}
						disabled={busy}
					>
						<SelectTrigger id="credential-kind" class="w-full">
							<SelectValue placeholder="Choose a type" />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="password">Password</SelectItem>
							<SelectItem value="private_key">Private key</SelectItem>
						</SelectContent>
					</Select>
				</div>
			{/if}

			{#if credentialMode !== 'rename'}
				{#if credentialForm.kind === 'password'}
					<div class="grid gap-1.5">
						<Label for="credential-secret">Password</Label>
						<Input
							id="credential-secret"
							type="password"
							autocomplete="new-password"
							bind:value={credentialForm.secret}
							disabled={busy}
						/>
					</div>
				{:else}
					<div class="grid gap-1.5">
						<Label for="credential-secret">Private key (OpenSSH format)</Label>
						<Textarea
							id="credential-secret"
							rows={6}
							class="font-mono text-xs"
							placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
							bind:value={credentialForm.secret}
							disabled={busy}
						/>
					</div>
					<div class="grid gap-1.5">
						<Label for="credential-passphrase">Passphrase (only if the key is encrypted)</Label>
						<Input
							id="credential-passphrase"
							type="password"
							autocomplete="new-password"
							bind:value={credentialForm.passphrase}
							disabled={busy}
						/>
					</div>
				{/if}
			{/if}

			<DialogFooter>
				<Button type="button" variant="outline" onclick={closeCredentialDialog}>Cancel</Button>
				<Button type="submit" disabled={busy}>
					{credentialMode === 'create'
						? 'Create credential'
						: credentialMode === 'rename'
							? 'Save name'
							: 'Replace secret'}
				</Button>
			</DialogFooter>
		</form>
	</DialogContent>
</Dialog>
