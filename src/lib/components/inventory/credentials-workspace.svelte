<script lang="ts">
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';

	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue
	} from '#lib/components/ui/select';
	import {
		createPasswordCredential,
		createPrivateKeyCredential,
		credentialFormProblem,
		deleteCredential,
		emptyCredentialForm,
		renameCredential,
		replacePassword,
		replacePrivateKey,
		type CredentialFormValues,
		type CredentialSummary,
		type Host
	} from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import CredentialGrid from './credential-grid.svelte';
	import CredentialSectionPanel from './credential-section-panel.svelte';
	import ListDetail from './list-detail.svelte';
	import type { InventoryPageProps } from './page-props';

	let {
		credentials,
		hosts = [],
		busy = false,
		vaultStatus,
		onmutate,
		onerror,
		onrefresh,
		onopenvault
	}: InventoryPageProps & {
		credentials: CredentialSummary[];
		hosts?: Host[];
		vaultStatus?: VaultStatus;
		onrefresh?: () => void | Promise<void>;
		onopenvault?: () => void;
	} = $props();

	let searchQuery = $state('');
	let selectedType = $state<'all' | 'password' | 'private_key'>('all');

	// Credential inspector panel state
	let formOpen = $state(false);
	let editingCredentialId = $state<string | null>(null);
	let credentialForm = $state<CredentialFormValues>(emptyCredentialForm());

	$effect(() => {
		// Close inspector if vault locks
		if (vaultStatus !== 'unlocked' && formOpen) {
			formOpen = false;
			editingCredentialId = null;
			credentialForm = emptyCredentialForm();
		}
	});

	const typeFilterOptions = [
		{ value: 'all', label: 'All types' },
		{ value: 'password', label: 'Password' },
		{ value: 'private_key', label: 'Private key' }
	];

	const filteredCredentials = $derived(
		credentials.filter((cred) => {
			if (selectedType !== 'all' && cred.kind !== selectedType) {
				return false;
			}
			if (!searchQuery.trim()) return true;
			const q = searchQuery.toLowerCase().trim();
			return (
				cred.name.toLowerCase().includes(q) ||
				(cred.kind === 'password' ? 'password' : 'private key key ssh').includes(q)
			);
		})
	);

	// Prune selection if credential was removed
	$effect(() => {
		if (editingCredentialId !== null && !credentials.some((c) => c.id === editingCredentialId)) {
			closeCredentialForm();
		}
	});

	const selectedCredential = $derived(
		editingCredentialId ? (credentials.find((c) => c.id === editingCredentialId) ?? null) : null
	);

	function handleSelectCredential(cred: CredentialSummary) {
		openEditCredential(cred);
	}

	function openCreateCredential() {
		editingCredentialId = null;
		credentialForm = emptyCredentialForm();
		formOpen = true;
	}

	function openEditCredential(cred: CredentialSummary) {
		editingCredentialId = cred.id;
		credentialForm = {
			name: cred.name,
			kind: cred.kind,
			secret: '',
			passphrase: ''
		};
		formOpen = true;
	}

	function closeCredentialForm() {
		formOpen = false;
		editingCredentialId = null;
		credentialForm = emptyCredentialForm();
	}

	async function submitCredential(event?: SubmitEvent) {
		event?.preventDefault();

		if (editingCredentialId === null) {
			// Creating new credential
			const problem = credentialFormProblem('create', credentialForm);
			if (problem) {
				onerror(problem);
				clearSecret();
				return;
			}

			const { name, kind, secret, passphrase } = credentialForm;
			const optionalPassphrase = passphrase.trim() === '' ? null : passphrase.trim();

			await onmutate(async () => {
				if (kind === 'password') {
					await createPasswordCredential(name.trim(), secret);
				} else {
					await createPrivateKeyCredential(name.trim(), secret, optionalPassphrase);
				}
				closeCredentialForm();
			});
		} else {
			// Editing existing credential
			const target = selectedCredential;
			if (!target) return;

			if (credentialForm.name.trim() === '') {
				onerror('Enter a name for this credential.');
				return;
			}

			const nameChanged = credentialForm.name.trim() !== target.name;
			const secretProvided = credentialForm.secret.trim() !== '';

			await onmutate(async () => {
				if (nameChanged) {
					await renameCredential(target.id, credentialForm.name.trim());
				}

				if (secretProvided) {
					if (target.kind === 'password') {
						await replacePassword(target.id, credentialForm.secret);
					} else {
						const optionalPassphrase =
							credentialForm.passphrase.trim() === '' ? null : credentialForm.passphrase.trim();
						await replacePrivateKey(target.id, credentialForm.secret, optionalPassphrase);
					}
				}

				closeCredentialForm();
			});
		}
	}

	function clearSecret() {
		credentialForm.secret = '';
		credentialForm.passphrase = '';
	}

	async function handleDeleteSelected() {
		const target = selectedCredential;
		if (!target) return;
		closeCredentialForm();
		await onmutate(() => deleteCredential(target.id));
	}
</script>

<ListDetail open={formOpen}>
	{#snippet header()}
		<!-- Credentials Toolbar: search on top, actions row below -->
		<div class="flex flex-col gap-3">
			<!-- Top Search Bar -->
			<div class="relative w-full">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					placeholder="Find a credential by name or type..."
					bind:value={searchQuery}
					class="h-9 w-full rounded-lg border-border/70 bg-card/60 pr-4 pl-9 text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-primary/40"
					aria-label="Find a credential by name or type"
					disabled={busy}
				/>
			</div>

			<!-- Secondary Action Bar (Single line) -->
			<div class="flex items-center justify-between gap-2">
				<div class="flex items-center gap-2">
					<Button
						size="sm"
						class="h-8 shrink-0 gap-1.5 text-xs font-medium"
						onclick={openCreateCredential}
						disabled={busy || vaultStatus !== 'unlocked'}
					>
						<PlusIcon class="size-3.5" />
						<span>New credential</span>
					</Button>
				</div>

				<div class="flex shrink-0 items-center gap-1.5 sm:gap-2">
					<Select
						type="single"
						bind:value={selectedType}
						items={typeFilterOptions}
						disabled={busy || vaultStatus !== 'unlocked'}
					>
						<SelectTrigger
							id="credential-type-filter"
							class="h-8 w-36 border-border/70 bg-card/60 text-xs"
							aria-label="Filter by credential type"
						>
							<div class="flex items-center gap-1.5 truncate">
								<KeyRoundIcon class="size-3.5 shrink-0 text-muted-foreground" />
								<SelectValue placeholder="All types" />
							</div>
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="all">All types</SelectItem>
							<SelectItem value="password">Password</SelectItem>
							<SelectItem value="private_key">Private key</SelectItem>
						</SelectContent>
					</Select>
				</div>
			</div>
		</div>
	{/snippet}

	<div class="flex flex-col gap-2">
		<div class="flex items-center justify-between px-0.5">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Credentials</h3>
			{#if filteredCredentials.length > 0}
				<span class="text-[11px] text-muted-foreground">
					{filteredCredentials.length}
					{filteredCredentials.length === 1 ? 'credential' : 'credentials'}
				</span>
			{/if}
		</div>

		<CredentialGrid
			credentials={filteredCredentials}
			totalCount={credentials.length}
			{hosts}
			selectedCredentialId={editingCredentialId}
			{busy}
			{vaultStatus}
			onselect={handleSelectCredential}
			{onopenvault}
		/>
	</div>

	{#snippet panel()}
		{#if formOpen}
			<CredentialSectionPanel
				mode={editingCredentialId === null ? 'create' : 'edit'}
				credential={selectedCredential}
				bind:form={credentialForm}
				{hosts}
				{busy}
				{vaultStatus}
				onsubmit={submitCredential}
				ondelete={handleDeleteSelected}
				onclose={closeCredentialForm}
			/>
		{/if}
	{/snippet}
</ListDetail>
