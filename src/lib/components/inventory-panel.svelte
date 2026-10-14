<script lang="ts">
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import { Card, CardContent } from '#lib/components/ui/card';
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
	import { errorMessage } from '#lib/errors';
	import BackupRestore from '#lib/components/backup-restore.svelte';
	import HostWorkspace from '#lib/components/inventory/host-workspace.svelte';
	import {
		NO_RELATION,
		addedAt,
		createGroup,
		createHost,
		createPasswordCredential,
		createPrivateKeyCredential,
		credentialFormProblem,
		deleteCredential,
		deleteGroup,
		deleteHost,
		emptyCredentialForm,
		emptyHostForm,
		emptyInventory,
		forgetKnownHost,
		hostFormProblem,
		hostInput,
		inventoryVisible,
		listCredentials,
		listGroups,
		listHosts,
		listKnownHosts,
		renameCredential,
		renameGroup,
		replacePassword,
		replacePrivateKey,
		updateHost,
		type CredentialFormMode,
		type CredentialFormValues,
		type CredentialSummary,
		type Group,
		type Host,
		type HostFormValues,
		type KnownHost
	} from '#lib/inventory';
	import {
		SNIPPET_SECRET_NOTE,
		createSnippet,
		deleteSnippet,
		emptySnippetForm,
		listSnippets,
		runEnabled,
		runSnippet,
		runSnippetRequest,
		snippetFormFor,
		snippetFormProblem,
		snippetInput,
		snippetPreview,
		updateSnippet,
		type Snippet,
		type SnippetFormValues
	} from '#lib/snippets';
	import { snippetTarget, snippetTargetReady, type SnippetTarget } from '#lib/session-tabs';
	import type { AuthMethod, ConnectionStatus } from '#lib/types';
	import type { VaultStatus } from '#lib/vault';

	// The page owns the vault status and every session, so this panel is told when
	// the vault opens or closes, which tab is on screen (the single-session parts
	// of this panel follow that one), whether any session is still alive at all,
	// and which saved hosts are already being connected.
	let {
		vaultStatus,
		sessionStatus = 'idle',
		sessionId,
		sessionTarget = '',
		hasLiveSessions = false,
		hostConnecting = () => false,
		onconnect,
		quickHost = $bindable('localhost'),
		quickPort = $bindable(22),
		quickUsername = $bindable(''),
		quickAuth = $bindable<AuthMethod>('password'),
		quickPassword = $bindable(''),
		quickPrivateKey = $bindable(''),
		quickPassphrase = $bindable(''),
		quickSubmitting = false,
		onquicksubmit,
		onquickclear
	}: {
		vaultStatus: VaultStatus | undefined;
		/** The status of the tab on screen. */
		sessionStatus?: ConnectionStatus;
		/** The id of the tab on screen, if there is one. */
		sessionId?: string;
		/** What that tab is called. */
		sessionTarget?: string;
		/** Whether any tab is still talking to a host, closing included. */
		hasLiveSessions?: boolean;
		/** Whether a tab for this saved host is already being opened. */
		hostConnecting?: (hostId: string) => boolean;
		onconnect?: (host: Host) => void;
		quickHost?: string;
		quickPort?: number;
		quickUsername?: string;
		quickAuth?: AuthMethod;
		quickPassword?: string;
		quickPrivateKey?: string;
		quickPassphrase?: string;
		quickSubmitting?: boolean;
		onquicksubmit?: (event: SubmitEvent) => void;
		onquickclear?: () => void;
	} = $props();

	let section = $state<'hosts' | 'credentials' | 'groups' | 'known_hosts' | 'snippets' | 'backup'>(
		'hosts'
	);
	let hosts = $state<Host[]>([]);
	let groups = $state<Group[]>([]);
	let credentials = $state<CredentialSummary[]>([]);
	let knownHosts = $state<KnownHost[]>([]);
	let snippets = $state<Snippet[]>([]);
	let errorText = $state('');
	let busy = $state(false);

	// Host dialog. `editingHost` is null while creating a new one.
	let hostOpen = $state(false);
	let editingHost = $state<string | null>(null);
	let hostForm = $state<HostFormValues>(emptyHostForm());

	// Group dialog. `renamingGroup` is null while creating a new one.
	let groupOpen = $state(false);
	let renamingGroup = $state<string | null>(null);
	let groupNameInput = $state('');

	// Known-host dialog: forgetting is the only thing it can do.
	let forgetTarget = $state<KnownHost | null>(null);

	// Credential dialog: create, rename, or replace the secret.
	let credentialOpen = $state(false);
	let credentialMode = $state<CredentialFormMode>('create');
	let credentialTarget = $state<CredentialSummary | null>(null);
	let credentialForm = $state<CredentialFormValues>(emptyCredentialForm());

	// Snippet dialog. `editingSnippet` is null while creating a new one.
	let snippetOpen = $state(false);
	let editingSnippet = $state<string | null>(null);
	let snippetForm = $state<SnippetFormValues>(emptySnippetForm());
	// The snippet a delete confirmation is about, and the preview a run waits on.
	let snippetDeleteTarget = $state<Snippet | null>(null);
	let runTarget = $state<Snippet | null>(null);
	// The tab the open preview belongs to, recorded when it was opened. A later
	// switch of the active tab can then never change where the command would go.
	let runSession = $state<SnippetTarget | null>(null);
	let running = $state(false);

	const visible = $derived(inventoryVisible(vaultStatus));
	// At most one run at a time, and only against a session that is really up.
	// When a session ends while the preview is open, this turns false by itself.
	const canRun = $derived(runEnabled(sessionStatus, sessionId, running));
	// Whether the preview's own session may still take the command: it has to be
	// the tab on screen, and it has to be up.
	const canRunTarget = $derived(
		snippetTargetReady(runSession, sessionId ?? null, sessionStatus, running)
	);
	// bits-ui reads a closed trigger's text from these, so a select shows a name
	// instead of the raw id it actually stores.
	const groupOptions = $derived([
		{ value: NO_RELATION, label: 'None' },
		...groups.map((group) => ({ value: group.id, label: group.name }))
	]);
	const credentialOptions = $derived([
		{ value: NO_RELATION, label: 'None' },
		...credentials.map((credential) => ({ value: credential.id, label: credential.name }))
	]);
	const credentialKinds = [
		{ value: 'password', label: 'Password' },
		{ value: 'private_key', label: 'Private key' }
	];
	// Not reactive on purpose: it only remembers whether the lists are loaded.
	let holding = false;

	$effect(() => {
		if (visible && !holding) {
			holding = true;
			void refresh();
		} else if (!visible && holding) {
			holding = false;
			clear();
		}
	});

	async function refresh() {
		try {
			groups = await listGroups();
			credentials = await listCredentials();
			hosts = await listHosts();
			knownHosts = await listKnownHosts();
			snippets = await listSnippets();
		} catch (error) {
			errorText = errorMessage(error);
		}
	}

	/** Switching sections re-reads, so a key approved since the vault opened shows up. */
	function show(next: typeof section) {
		if (section === 'hosts' && next !== 'hosts') {
			onquickclear?.();
		}
		section = next;
		void refresh();
	}

	/** Runs one change, then reports its failure in one place. */
	async function run(work: () => Promise<void>) {
		busy = true;
		errorText = '';
		try {
			await work();
		} catch (error) {
			errorText = errorMessage(error);
		} finally {
			busy = false;
		}
	}

	/**
	 * Throws away everything this panel holds: used when the vault locks. The
	 * snippets go with it, and every snippet dialog closes, so nothing decrypted
	 * is left on screen.
	 */
	function clear() {
		const empty = emptyInventory();
		hosts = empty.hosts;
		groups = empty.groups;
		credentials = empty.credentials;
		knownHosts = empty.knownHosts;
		snippets = empty.snippets;
		forgetTarget = null;
		hostOpen = false;
		editingHost = null;
		hostForm = emptyHostForm();
		groupOpen = false;
		renamingGroup = null;
		groupNameInput = '';
		credentialOpen = false;
		credentialTarget = null;
		credentialForm = emptyCredentialForm();
		snippetOpen = false;
		editingSnippet = null;
		snippetForm = emptySnippetForm();
		snippetDeleteTarget = null;
		runTarget = null;
		runSession = null;
		running = false;
		errorText = '';
		onquickclear?.();
	}

	// --- hosts ---

	function openCreateHost() {
		editingHost = null;
		hostForm = emptyHostForm();
		hostOpen = true;
	}

	function openEditHost(host: Host) {
		editingHost = host.id;
		hostForm = {
			label: host.label,
			address: host.address,
			port: String(host.port),
			username: host.username,
			groupId: host.groupId ?? NO_RELATION,
			credentialId: host.credentialId ?? NO_RELATION
		};
		hostOpen = true;
	}

	async function submitHost(event: SubmitEvent) {
		event.preventDefault();
		const problem = hostFormProblem(hostForm);
		if (problem) {
			errorText = problem;
			return;
		}

		const input = hostInput(hostForm);
		const editing = editingHost;
		await run(async () => {
			if (editing === null) {
				await createHost(input);
			} else {
				await updateHost(editing, input);
			}
			hosts = await listHosts();
			hostOpen = false;
			hostForm = emptyHostForm();
		});
	}

	async function confirmForget() {
		const target = forgetTarget;
		if (!target) return;
		await run(async () => {
			await forgetKnownHost(target.id);
			knownHosts = await listKnownHosts();
			forgetTarget = null;
		});
	}

	async function removeHost(host: Host) {
		await run(async () => {
			await deleteHost(host.id);
			hosts = await listHosts();
		});
	}

	// --- groups ---

	function openCreateGroup() {
		renamingGroup = null;
		groupNameInput = '';
		groupOpen = true;
	}

	function openRenameGroup(group: Group) {
		renamingGroup = group.id;
		groupNameInput = group.name;
		groupOpen = true;
	}

	async function submitGroup(event: SubmitEvent) {
		event.preventDefault();
		if (groupNameInput.trim() === '') {
			errorText = 'Enter a group name.';
			return;
		}

		const renaming = renamingGroup;
		await run(async () => {
			if (renaming === null) {
				await createGroup(groupNameInput.trim());
			} else {
				await renameGroup(renaming, groupNameInput.trim());
			}
			groups = await listGroups();
			groupOpen = false;
			groupNameInput = '';
		});
	}

	async function removeGroup(group: Group) {
		await run(async () => {
			await deleteGroup(group.id);
			groups = await listGroups();
		});
	}

	// --- credentials ---

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
			errorText = problem;
			clearSecret();
			return;
		}

		const target = credentialTarget;
		const { name, kind, secret, passphrase } = credentialForm;
		const optionalPassphrase = passphrase === '' ? null : passphrase;

		busy = true;
		errorText = '';
		try {
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
			credentials = await listCredentials();
			closeCredentialDialog();
		} catch (error) {
			errorText = errorMessage(error);
			// The secret does not stay in the form after a failed submit either.
			clearSecret();
		} finally {
			busy = false;
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
		await run(async () => {
			await deleteCredential(credential.id);
			credentials = await listCredentials();
		});
	}

	// --- snippets ---

	function openCreateSnippet() {
		editingSnippet = null;
		snippetForm = emptySnippetForm();
		snippetOpen = true;
	}

	function openEditSnippet(snippet: Snippet) {
		editingSnippet = snippet.id;
		snippetForm = snippetFormFor(snippet);
		snippetOpen = true;
	}

	async function submitSnippet(event: SubmitEvent) {
		event.preventDefault();
		const problem = snippetFormProblem(snippetForm);
		if (problem) {
			errorText = problem;
			return;
		}

		const input = snippetInput(snippetForm);
		const editing = editingSnippet;
		await run(async () => {
			if (editing === null) {
				await createSnippet(input);
			} else {
				await updateSnippet(editing, input);
			}
			snippets = await listSnippets();
			snippetOpen = false;
			snippetForm = emptySnippetForm();
		});
	}

	async function confirmDeleteSnippet() {
		const target = snippetDeleteTarget;
		if (!target) return;
		await run(async () => {
			await deleteSnippet(target.id);
			snippets = await listSnippets();
			snippetDeleteTarget = null;
		});
	}

	/**
	 * Opens the preview. Opening it runs nothing: only the confirm button does.
	 *
	 * The session is recorded here, so a tab switch cannot quietly aim the
	 * confirmation at a different session than the one the dialog names.
	 */
	function openRunPreview(snippet: Snippet) {
		runTarget = snippet;
		runSession = snippetTarget(sessionId, sessionTarget);
	}

	function closeRunPreview() {
		runTarget = null;
		runSession = null;
	}

	/**
	 * Sends the snippet the preview is showing to the session it was opened for.
	 *
	 * A failure keeps the dialog open and is reported: the snippet is never
	 * presented as having run when the backend refused it. Closing the dialog, on
	 * the other hand, is a cancel, so nothing is sent.
	 */
	async function confirmRunSnippet() {
		const target = runTarget;
		const bound = runSession;
		if (!target || !bound || !canRunTarget) return;

		running = true;
		errorText = '';
		try {
			await runSnippet(runSnippetRequest(bound.sessionId, target.id));
			closeRunPreview();
		} catch (error) {
			errorText = errorMessage(error);
		} finally {
			running = false;
		}
	}
</script>

{#if visible}
	<div class="flex flex-col gap-4">
		<!-- Compact section navigation bar -->
		<div class="flex items-center gap-1.5 overflow-x-auto border-b border-border/60 pb-2">
			<Button
				size="sm"
				variant={section === 'hosts' ? 'default' : 'ghost'}
				class="h-7 text-xs"
				onclick={() => show('hosts')}
			>
				Hosts ({hosts.length})
			</Button>
			<Button
				size="sm"
				variant={section === 'credentials' ? 'default' : 'ghost'}
				class="h-7 text-xs"
				onclick={() => show('credentials')}
			>
				Credentials ({credentials.length})
			</Button>
			<Button
				size="sm"
				variant={section === 'groups' ? 'default' : 'ghost'}
				class="h-7 text-xs"
				onclick={() => show('groups')}
			>
				Groups ({groups.length})
			</Button>
			<Button
				size="sm"
				variant={section === 'known_hosts' ? 'default' : 'ghost'}
				class="h-7 text-xs"
				onclick={() => show('known_hosts')}
			>
				Known hosts ({knownHosts.length})
			</Button>
			<Button
				size="sm"
				variant={section === 'snippets' ? 'default' : 'ghost'}
				class="h-7 text-xs"
				onclick={() => show('snippets')}
			>
				Snippets ({snippets.length})
			</Button>
			<Button
				size="sm"
				variant={section === 'backup' ? 'default' : 'ghost'}
				class="h-7 text-xs"
				onclick={() => show('backup')}
			>
				Backup &amp; Restore
			</Button>
		</div>

		{#if errorText}
			<Alert variant="destructive">
				<AlertTitle>Inventory error</AlertTitle>
				<AlertDescription>{errorText}</AlertDescription>
			</Alert>
		{/if}

		{#if section === 'hosts'}
			<HostWorkspace
				{hosts}
				{groups}
				{credentials}
				{busy}
				{vaultStatus}
				{hostConnecting}
				onconnect={(host) => onconnect?.(host)}
				onedit={openEditHost}
				ondelete={removeHost}
				oncreate={openCreateHost}
				onimported={refresh}
				bind:quickHost
				bind:quickPort
				bind:quickUsername
				bind:quickAuth
				bind:quickPassword
				bind:quickPrivateKey
				bind:quickPassphrase
				{quickSubmitting}
				{onquicksubmit}
				{onquickclear}
			/>
		{:else}
			<Card>
				<CardContent class="grid gap-4 pt-6">
					{#if section === 'credentials'}
						<div class="grid gap-3">
							<div class="flex items-center justify-between">
								<h3 class="text-sm font-medium">Credentials</h3>
								<Button size="sm" onclick={openCreateCredential} disabled={busy}
									>New credential</Button
								>
							</div>
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
						</div>
					{:else if section === 'snippets'}
						<div class="grid gap-3">
							<div class="flex items-center justify-between">
								<h3 class="text-sm font-medium">Snippets</h3>
								<Button size="sm" onclick={openCreateSnippet} disabled={busy}>New snippet</Button>
							</div>
							<p class="text-xs text-muted-foreground">
								Commands kept encrypted in this vault. One is only ever sent to the session that is
								already open, after you have read it in full and confirmed. {SNIPPET_SECRET_NOTE}
							</p>
							{#if snippets.length === 0}
								<p class="text-xs text-muted-foreground">No snippets yet.</p>
							{:else}
								<ul class="grid gap-2">
									{#each snippets as snippet (snippet.id)}
										<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
											<div class="min-w-0">
												<p class="truncate text-sm font-medium">{snippet.name}</p>
												<p class="truncate font-mono text-xs text-muted-foreground">
													{snippetPreview(snippet.content)}
												</p>
												<p class="truncate text-xs text-muted-foreground">
													updated {addedAt(snippet.updatedAt)}
												</p>
											</div>
											<div class="flex shrink-0 gap-2">
												<Button
													size="sm"
													onclick={() => openRunPreview(snippet)}
													disabled={busy || !canRun}
													title={canRun
														? 'Read this command before it is sent'
														: 'Connect a session first'}
												>
													Run
												</Button>
												<Button
													variant="outline"
													size="sm"
													onclick={() => openEditSnippet(snippet)}
													disabled={busy}
												>
													Edit
												</Button>
												<Button
													variant="outline"
													size="sm"
													onclick={() => (snippetDeleteTarget = snippet)}
													disabled={busy}
												>
													Delete
												</Button>
											</div>
										</li>
									{/each}
								</ul>
							{/if}
						</div>
					{:else if section === 'groups'}
						<div class="grid gap-3">
							<div class="flex items-center justify-between">
								<h3 class="text-sm font-medium">Groups</h3>
								<Button size="sm" onclick={openCreateGroup} disabled={busy}>New group</Button>
							</div>
							{#if groups.length === 0}
								<p class="text-xs text-muted-foreground">No groups yet.</p>
							{:else}
								<ul class="grid gap-2">
									{#each groups as group (group.id)}
										<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
											<p class="truncate text-sm font-medium">{group.name}</p>
											<div class="flex shrink-0 gap-2">
												<Button
													variant="outline"
													size="sm"
													onclick={() => openRenameGroup(group)}
													disabled={busy}
												>
													Rename
												</Button>
												<Button
													variant="outline"
													size="sm"
													onclick={() => removeGroup(group)}
													disabled={busy}
												>
													Delete
												</Button>
											</div>
										</li>
									{/each}
								</ul>
							{/if}
						</div>
					{:else if section === 'backup'}
						<div class="grid gap-3">
							<h3 class="text-sm font-medium">Backup &amp; Restore</h3>
							<p class="text-xs text-muted-foreground">
								A backup is one encrypted file, locked with a password you choose for it. It holds
								every group, host, credential, known host and snippet in this vault, and it can be
								restored into this vault or into another one with a different master password. It is
								not a sync: restoring replaces every record in this vault with the ones in the file.
							</p>
							<BackupRestore {vaultStatus} {hasLiveSessions} onrestored={refresh} />
						</div>
					{:else}
						<div class="grid gap-3">
							<div class="flex items-center justify-between">
								<h3 class="text-sm font-medium">Known hosts</h3>
							</div>
							{#if knownHosts.length === 0}
								<p class="text-xs text-muted-foreground">
									No host keys are trusted yet. Approving one while connecting adds it here.
								</p>
							{:else}
								<ul class="grid gap-2">
									{#each knownHosts as known (known.id)}
										<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
											<div class="min-w-0">
												<p class="truncate font-mono text-sm font-medium">{known.hostKeyId}</p>
												<p class="truncate text-xs text-muted-foreground">
													{known.keyAlgorithm} · added {addedAt(known.createdAt)}
												</p>
												<p class="truncate font-mono text-xs text-muted-foreground">
													{known.fingerprintSha256}
												</p>
											</div>
											<div class="shrink-0">
												<Button
													variant="outline"
													size="sm"
													onclick={() => (forgetTarget = known)}
													disabled={busy}
												>
													Forget
												</Button>
											</div>
										</li>
									{/each}
								</ul>
							{/if}
						</div>
					{/if}
				</CardContent>
			</Card>
		{/if}
	</div>

	<Dialog open={forgetTarget !== null} onOpenChange={(open) => !open && (forgetTarget = null)}>
		<DialogContent class="sm:max-w-md">
			<DialogHeader>
				<DialogTitle>Forget this host key?</DialogTitle>
				<DialogDescription>
					The next connection to this host will ask you to check its fingerprint again. Nothing else
					is removed.
				</DialogDescription>
			</DialogHeader>

			{#if forgetTarget}
				<dl class="grid gap-2 text-sm">
					<div class="grid gap-0.5">
						<dt class="text-xs text-muted-foreground">Host</dt>
						<dd class="font-mono">{forgetTarget.hostKeyId}</dd>
					</div>
					<div class="grid gap-0.5">
						<dt class="text-xs text-muted-foreground">SHA-256 fingerprint</dt>
						<dd class="font-mono break-all">{forgetTarget.fingerprintSha256}</dd>
					</div>
				</dl>
			{/if}

			<DialogFooter>
				<Button variant="outline" onclick={() => (forgetTarget = null)}>Cancel</Button>
				<Button variant="destructive" onclick={confirmForget} disabled={busy}>Forget</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>

	<Dialog bind:open={hostOpen}>
		<DialogContent class="sm:max-w-md">
			<DialogHeader>
				<DialogTitle>{editingHost === null ? 'New host' : 'Edit host'}</DialogTitle>
				<DialogDescription>
					The Credential chosen here is the one its Connect button uses. Secrets stay encrypted and
					are never shown again.
				</DialogDescription>
			</DialogHeader>

			<form class="grid gap-3" onsubmit={submitHost}>
				<div class="grid gap-1.5">
					<Label for="host-label">Label</Label>
					<Input id="host-label" bind:value={hostForm.label} disabled={busy} />
				</div>
				<div class="grid gap-3 sm:grid-cols-2">
					<div class="grid gap-1.5">
						<Label for="host-address">Address</Label>
						<Input id="host-address" bind:value={hostForm.address} disabled={busy} />
					</div>
					<div class="grid gap-1.5">
						<Label for="host-port">Port</Label>
						<Input id="host-port" type="number" bind:value={hostForm.port} disabled={busy} />
					</div>
				</div>
				<div class="grid gap-1.5">
					<Label for="host-username">Username</Label>
					<Input id="host-username" bind:value={hostForm.username} disabled={busy} />
				</div>
				<div class="grid gap-1.5">
					<Label for="host-group">Group</Label>
					<Select type="single" bind:value={hostForm.groupId} items={groupOptions} disabled={busy}>
						<SelectTrigger id="host-group" class="w-full">
							<SelectValue placeholder="None" />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value={NO_RELATION}>None</SelectItem>
							{#each groups as group (group.id)}
								<SelectItem value={group.id}>{group.name}</SelectItem>
							{/each}
						</SelectContent>
					</Select>
				</div>
				<div class="grid gap-1.5">
					<Label for="host-credential">Credential</Label>
					<Select
						type="single"
						bind:value={hostForm.credentialId}
						items={credentialOptions}
						disabled={busy}
					>
						<SelectTrigger id="host-credential" class="w-full">
							<SelectValue placeholder="None" />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value={NO_RELATION}>None</SelectItem>
							{#each credentials as credential (credential.id)}
								<SelectItem value={credential.id}>{credential.name}</SelectItem>
							{/each}
						</SelectContent>
					</Select>
				</div>

				<DialogFooter>
					<Button type="button" variant="outline" onclick={() => (hostOpen = false)}>Cancel</Button>
					<Button type="submit" disabled={busy}>
						{editingHost === null ? 'Create host' : 'Save host'}
					</Button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>

	<Dialog bind:open={groupOpen}>
		<DialogContent class="sm:max-w-md">
			<DialogHeader>
				<DialogTitle>{renamingGroup === null ? 'New group' : 'Rename group'}</DialogTitle>
				<DialogDescription>Groups keep related hosts together.</DialogDescription>
			</DialogHeader>

			<form class="grid gap-3" onsubmit={submitGroup}>
				<div class="grid gap-1.5">
					<Label for="group-name">Name</Label>
					<Input id="group-name" bind:value={groupNameInput} disabled={busy} />
				</div>
				<DialogFooter>
					<Button type="button" variant="outline" onclick={() => (groupOpen = false)}>
						Cancel
					</Button>
					<Button type="submit" disabled={busy}>
						{renamingGroup === null ? 'Create group' : 'Save name'}
					</Button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>

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

	<Dialog bind:open={snippetOpen}>
		<DialogContent class="sm:max-w-xl">
			<DialogHeader>
				<DialogTitle>{editingSnippet === null ? 'New snippet' : 'Edit snippet'}</DialogTitle>
				<DialogDescription>
					The command is stored encrypted in this vault and is only sent to a session you run it
					against, after you confirm the preview. Nothing here runs on its own.
					{SNIPPET_SECRET_NOTE}
				</DialogDescription>
			</DialogHeader>

			<form class="grid gap-3" onsubmit={submitSnippet}>
				<div class="grid gap-1.5">
					<Label for="snippet-name">Name</Label>
					<Input id="snippet-name" bind:value={snippetForm.name} disabled={busy} />
				</div>
				<div class="grid gap-1.5">
					<Label for="snippet-content">Command</Label>
					<Textarea
						id="snippet-content"
						rows={8}
						class="font-mono text-xs"
						placeholder="systemctl restart app"
						bind:value={snippetForm.content}
						disabled={busy}
					/>
					<p class="text-xs text-muted-foreground">
						More than one line is fine. Termust does not check the syntax: the remote shell decides
						what the command means.
					</p>
				</div>

				<DialogFooter>
					<Button type="button" variant="outline" onclick={() => (snippetOpen = false)}>
						Cancel
					</Button>
					<Button type="submit" disabled={busy}>
						{editingSnippet === null ? 'Create snippet' : 'Save snippet'}
					</Button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>

	<Dialog
		open={snippetDeleteTarget !== null}
		onOpenChange={(open) => !open && (snippetDeleteTarget = null)}
	>
		<DialogContent class="sm:max-w-md">
			<DialogHeader>
				<DialogTitle>Delete this snippet?</DialogTitle>
				<DialogDescription>
					It disappears from the list and can no longer be read or run. Nothing on the server is
					touched: a snippet only ever did something when you ran it.
				</DialogDescription>
			</DialogHeader>

			{#if snippetDeleteTarget}
				<p class="truncate text-sm font-medium">{snippetDeleteTarget.name}</p>
			{/if}

			<DialogFooter>
				<Button variant="outline" onclick={() => (snippetDeleteTarget = null)}>Cancel</Button>
				<Button variant="destructive" onclick={confirmDeleteSnippet} disabled={busy}>Delete</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>

	<Dialog open={runTarget !== null} onOpenChange={(open) => !open && !running && closeRunPreview()}>
		<DialogContent class="sm:max-w-2xl">
			<DialogHeader>
				<DialogTitle>Run this snippet?</DialogTitle>
				<DialogDescription>
					This sends the command to the shell of {runSession?.title ?? 'the tab it was opened for'}.
					Termust does not read the command itself, so check it here before it goes.
				</DialogDescription>
			</DialogHeader>

			{#if runTarget}
				<div class="grid gap-2">
					<p class="truncate text-sm font-medium">{runTarget.name}</p>
					<pre
						class="max-h-64 overflow-auto rounded-md border bg-muted p-3 font-mono text-xs break-words whitespace-pre-wrap">{runTarget.content}</pre>
					<p class="text-xs text-amber-500">
						It is sent as typed input to the shell that is already open.
					</p>
				</div>
			{/if}

			{#if runSession && !canRunTarget}
				<Alert>
					<AlertDescription>
						This preview belongs to {runSession.title}. It is only ever sent to the session it was
						confirmed for, so switch back to that tab to run it.
					</AlertDescription>
				</Alert>
			{/if}

			<DialogFooter>
				<Button variant="outline" onclick={closeRunPreview} disabled={running}>Cancel</Button>
				<Button onclick={confirmRunSnippet} disabled={!canRunTarget}>
					{running ? 'Running…' : 'Run snippet'}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
{/if}
