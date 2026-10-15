<script lang="ts">
	import ArrowRightToLineIcon from '@lucide/svelte/icons/arrow-right-to-line';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import KeyIcon from '@lucide/svelte/icons/key';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LockIcon from '@lucide/svelte/icons/lock';
	import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';
	import ServerIcon from '@lucide/svelte/icons/server';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';

	import { Button } from '#lib/components/ui/button';
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
	import type { CredentialFormValues, CredentialSummary, Host } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';

	let {
		mode = 'edit',
		credential = null,
		form = $bindable(),
		hosts = [],
		busy = false,
		vaultStatus,
		onsubmit,
		ondelete,
		onclose
	}: {
		mode: 'create' | 'edit';
		credential?: CredentialSummary | null;
		form: CredentialFormValues;
		hosts?: Host[];
		busy?: boolean;
		vaultStatus?: VaultStatus;
		onsubmit: (event?: SubmitEvent) => void;
		ondelete?: () => void;
		onclose: () => void;
	} = $props();

	let showMoreMenu = $state(false);

	const credentialKinds = [
		{ value: 'password', label: 'Password' },
		{ value: 'private_key', label: 'Private key' }
	];

	const linkedHosts = $derived(
		credential ? hosts.filter((h) => h.credentialId === credential.id) : []
	);

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
	aria-label={mode === 'create' ? 'New Credential' : 'Credential Details'}
>
	<!-- Header: Title, Vault badge, Options and Close button -->
	<header class="flex items-start justify-between gap-2 pt-0.5">
		<div class="flex flex-col gap-0.5">
			<h3 class="text-sm font-semibold text-foreground">
				{mode === 'create' ? 'New Credential' : 'Credential Details'}
			</h3>
			<div class="flex items-center gap-1 text-[11px] text-muted-foreground">
				<span>{vaultStatus === 'unlocked' ? 'Personal vault' : 'Local vault'}</span>
				<ChevronDownIcon class="size-3 opacity-60" />
			</div>
		</div>

		<div class="flex items-center gap-0.5">
			{#if mode === 'edit' && ondelete}
				<div class="relative">
					<Button
						variant="ghost"
						size="icon"
						class="size-7 text-muted-foreground hover:text-foreground"
						onclick={() => (showMoreMenu = !showMoreMenu)}
						title="More options"
						aria-label="More options"
					>
						<MoreHorizontalIcon class="size-4" />
					</Button>
					{#if showMoreMenu}
						<div
							class="fixed inset-0 z-40"
							onclick={() => (showMoreMenu = false)}
							aria-hidden="true"
						></div>
						<div
							class="absolute top-full right-0 z-50 mt-1 min-w-36 animate-in rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md fade-in-0 zoom-in-95"
						>
							<button
								type="button"
								class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-destructive hover:bg-destructive/10"
								onclick={() => {
									showMoreMenu = false;
									ondelete();
								}}
							>
								<Trash2Icon class="size-3.5" />
								<span>Delete credential</span>
							</button>
						</div>
					{/if}
				</div>
			{/if}

			<Button
				variant="ghost"
				size="icon"
				class="size-7 text-muted-foreground hover:text-foreground"
				onclick={onclose}
				aria-label="Close credential details"
				title="Close panel"
			>
				<ArrowRightToLineIcon class="size-3.5" />
			</Button>
		</div>
	</header>

	<!-- Scrollable Form Body -->
	<form
		id="credential-inspector-form"
		class="flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5 sm:gap-5"
		onsubmit={(e) => {
			e.preventDefault();
			onsubmit(e);
		}}
	>
		<!-- 1. General Section (Name, Type) -->
		<div class="flex flex-col gap-2">
			<Label for="inspector-credential-name" class="text-xs font-medium text-muted-foreground">
				General
			</Label>

			<!-- Credential Name -->
			<Input
				id="inspector-credential-name"
				bind:value={form.name}
				disabled={busy}
				placeholder="Credential name (e.g. Production root, AWS key)"
				class="h-9 border-border/60 bg-muted/30 text-xs"
				required
			/>

			<!-- Credential Type -->
			{#if mode === 'create'}
				<Select type="single" bind:value={form.kind} items={credentialKinds} disabled={busy}>
					<SelectTrigger
						id="inspector-credential-kind"
						class="h-9! w-full border-border/60 bg-muted/30 text-xs"
					>
						<div class="flex items-center gap-2 truncate">
							{#if form.kind === 'password'}
								<KeyIcon class="size-3.5 shrink-0 text-muted-foreground" />
							{:else}
								<KeyRoundIcon class="size-3.5 shrink-0 text-muted-foreground" />
							{/if}
							<SelectValue placeholder="Choose a type" />
						</div>
					</SelectTrigger>
					<SelectContent>
						<SelectItem value="password">Password</SelectItem>
						<SelectItem value="private_key">Private key</SelectItem>
					</SelectContent>
				</Select>
			{:else}
				<div
					class="flex items-center justify-between rounded-md border border-border/60 bg-muted/30 px-3 py-2 text-xs"
				>
					<span class="text-muted-foreground">Type</span>
					<div class="flex items-center gap-1.5 font-medium text-foreground">
						{#if credential?.kind === 'password'}
							<KeyIcon class="size-3.5 text-emerald-500" />
							<span>Password</span>
						{:else}
							<KeyRoundIcon class="size-3.5 text-indigo-500" />
							<span>Private key</span>
						{/if}
					</div>
				</div>
			{/if}
		</div>

		<!-- 2. Secret Section -->
		<div class="flex flex-col gap-2">
			<div class="flex items-center justify-between">
				<span class="text-xs font-medium text-muted-foreground">
					{mode === 'create' ? 'Secret' : 'Secret Management'}
				</span>
			</div>

			{#if mode === 'edit'}
				<div
					class="rounded-md border border-border/60 bg-muted/20 p-2.5 text-[11px] text-muted-foreground"
				>
					The stored secret is encrypted and never displayed. Enter a new value below to replace it,
					or leave it blank to keep the current secret.
				</div>
			{/if}

			{#if (mode === 'create' && form.kind === 'password') || (mode === 'edit' && credential?.kind === 'password')}
				<!-- Password Input -->
				<div class="flex flex-col gap-1.5">
					<Label for="inspector-password" class="text-xs font-medium text-muted-foreground">
						{mode === 'create' ? 'Password' : 'New Password (optional)'}
					</Label>
					<Input
						id="inspector-password"
						type="password"
						autocomplete="new-password"
						bind:value={form.secret}
						disabled={busy}
						placeholder={mode === 'create' ? 'Enter password' : 'Leave empty to keep existing'}
						class="h-9 border-border/60 bg-muted/30 font-mono text-xs"
						required={mode === 'create'}
					/>
				</div>
			{:else}
				<!-- Private Key Textarea -->
				<div class="flex flex-col gap-1.5">
					<Label for="inspector-private-key" class="text-xs font-medium text-muted-foreground">
						{mode === 'create' ? 'Private key (OpenSSH format)' : 'New Private Key (optional)'}
					</Label>
					<Textarea
						id="inspector-private-key"
						rows={5}
						bind:value={form.secret}
						disabled={busy}
						placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
						class="border-border/60 bg-muted/30 font-mono text-xs"
						required={mode === 'create'}
					/>
				</div>

				<!-- Passphrase Input -->
				<div class="flex flex-col gap-1.5">
					<Label for="inspector-passphrase" class="text-xs font-medium text-muted-foreground">
						Passphrase (optional)
					</Label>
					<Input
						id="inspector-passphrase"
						type="password"
						autocomplete="new-password"
						bind:value={form.passphrase}
						disabled={busy}
						placeholder="Only if the key is encrypted"
						class="h-9 border-border/60 bg-muted/30 font-mono text-xs"
					/>
				</div>
			{/if}
		</div>

		<!-- 3. Attached Hosts Section (in edit mode) -->
		{#if mode === 'edit'}
			<div class="flex flex-col gap-2">
				<div class="flex items-center justify-between">
					<span class="text-xs font-medium text-muted-foreground">Attached Hosts</span>
					<span class="text-[11px] text-muted-foreground">{linkedHosts.length}</span>
				</div>

				{#if linkedHosts.length === 0}
					<p class="text-xs text-muted-foreground/80">No hosts are using this credential yet.</p>
				{:else}
					<ul class="flex flex-col gap-1.5">
						{#each linkedHosts as host (host.id)}
							<li
								class="flex items-center justify-between rounded-md border border-border/50 bg-muted/20 px-2.5 py-1.5 text-xs"
							>
								<div class="flex items-center gap-2 truncate">
									<ServerIcon class="size-3.5 shrink-0 text-muted-foreground" />
									<span class="font-medium text-foreground">{host.label}</span>
								</div>
								<span class="font-mono text-[11px] text-muted-foreground">
									{host.username}@{host.address}
								</span>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{/if}
	</form>

	<!-- Footer: Submit button -->
	<footer>
		<Button
			type="submit"
			form="credential-inspector-form"
			class="h-10 w-full bg-blue-600 text-xs font-semibold text-white shadow-sm hover:bg-blue-500"
			disabled={busy}
		>
			{mode === 'create' ? 'Create credential' : 'Save changes'}
		</Button>
	</footer>
</aside>
