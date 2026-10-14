<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';

	import AppShell from '#lib/components/app-shell/app-shell.svelte';
	import AppTitlebar from '#lib/components/app-shell/app-titlebar.svelte';
	import AppSidebar from '#lib/components/app-shell/app-sidebar.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import InventoryPanel from '#lib/components/inventory-panel.svelte';
	// The component is aliased: `TerminalSession` is the session descriptor type.
	import TerminalSessionPane from '#lib/components/terminal-session.svelte';
	import VaultPanel from '#lib/components/vault-panel.svelte';
	import { Button } from '#lib/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle
	} from '#lib/components/ui/dialog';
	import {
		connectSsh,
		disconnectSsh,
		errorMessage,
		hostKeyPromise,
		respondHostKey,
		startSavedSsh
	} from '#lib/ssh';
	import { createSessionLifecycle } from '#lib/session-lifecycle';
	import {
		clearHostKeyPrompt,
		createSession,
		findSession,
		firstPromptSessionId,
		hasLiveSessions,
		quickConnectTitle,
		savedHostConnecting,
		sessionCanDisconnect,
		setSessionError,
		type TerminalRuntime,
		type TerminalSession
	} from '#lib/session-tabs';
	import type { Host } from '#lib/inventory';
	import type { AuthMethod } from '#lib/types';
	import type { VaultStatus } from '#lib/vault';

	type Workspace = 'inventory' | 'terminal';

	/** Active workspace: inventory (hosts, credentials, vault) or terminal (SSH sessions). */
	let activeWorkspace = $state<Workspace>('inventory');
	/** Whether the left sidebar navigation rail is collapsed. */
	let sidebarCollapsed = $state(false);

	// The vault state lives here so the saved inventory appears and disappears
	// with it; the vault panel owns the password fields themselves.
	let vaultStatus = $state<VaultStatus | undefined>(undefined);

	// The Quick Connect form. Its credentials stay in these fields and are never
	// copied into a session.
	let host = $state('localhost');
	let port = $state(22);
	let username = $state('');
	let auth = $state<AuthMethod>('password');
	let password = $state('');
	let privateKey = $state('');
	let passphrase = $state('');

	/**
	 * Every open tab, in the order it was opened. This list is the whole of the
	 * window's session state: one descriptor per session, and no descriptor ever
	 * holds a credential.
	 */
	let sessions = $state<TerminalSession[]>([]);
	/**
	 * The tab the single-session parts of the UI point at: the Disconnect
	 * button, the status line, the host-key prompt and the snippet target. It is
	 * null only while there is no tab at all.
	 */
	let activeSessionId = $state<string | null>(null);
	/** True while a Quick Connect submit is on its way to the backend. */
	let quickSubmitting = $state(false);
	/** Set when the session events could not be subscribed to at all. */
	let eventsError = $state('');

	const activeSession = $derived(findSession(sessions, activeSessionId));
	/** Whether any tab is talking to a host, or on its way out after a close. */
	const anyLiveSession = $derived(hasLiveSessions(sessions));
	/** Whether the Disconnect button can do anything for the tab on screen. */
	const canDisconnect = $derived(
		activeSession !== undefined && sessionCanDisconnect(activeSession)
	);
	const activePrompt = $derived(activeSession?.hostKeyPrompt ?? null);

	// Terminals belong to the tab components, one each. This is only how the page
	// reaches the terminal behind a session id, so it is not reactive: nothing on
	// screen is rendered from it.
	const runtimes: Record<string, TerminalRuntime> = {};

	/**
	 * The window's session lifecycle: when the backend may be touched, and in
	 * what order. The tabs stay this component's Svelte state: the lifecycle
	 * reads and writes that same list, so there is still one source of truth.
	 */
	const lifecycle = createSessionLifecycle({
		read: () => sessions,
		write: (next) => {
			sessions = next;
		},
		readActive: () => activeSessionId,
		writeActive: (sessionId) => {
			activeSessionId = sessionId;
		},
		// The lifecycle deals in payloads, so the event wrapper is unwrapped here
		// and nowhere else.
		listen: <P,>(event: string, handler: (payload: P) => void) =>
			listen<P>(event, ({ payload }) => handler(payload)),
		connect: (request) => connectSsh(request),
		startSaved: (request) => startSavedSsh(request),
		disconnect: (sessionId) => disconnectSsh(sessionId),
		size: (sessionId) => runtimes[sessionId]?.size() ?? { cols: 80, rows: 24 },
		writeTerminal: (sessionId, bytes) => {
			runtimes[sessionId]?.write(bytes);
		},
		onEventsUnavailable: (error) => {
			// Without all four subscriptions a session would run with no visible
			// output, so the failure has to reach the user.
			eventsError = errorMessage(error);
		},
		onQuickStartFinished: () => {
			// The credentials have done their work, whether the backend took the
			// request, refused it, or its tab was closed before it got there.
			clearCredentials();
			quickSubmitting = false;
		}
	});

	onMount(() => {
		lifecycle.attach();

		return () => {
			// The window is going away. Pending starts are cancelled and every
			// session that is still running is stopped, not only the one on screen;
			// the terminals belong to the tabs, which dispose of their own.
			void lifecycle.teardown();
		};
	});

	// --- the Quick Connect form ------------------------------------------

	function clearCredentials() {
		password = '';
		privateKey = '';
		passphrase = '';
	}

	function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		// One submit opens one tab: a second click while the first request is
		// still being handed to the backend is ignored. As soon as the backend
		// has it, the form is free again for another connection.
		if (quickSubmitting) return;
		quickSubmitting = true;

		const trimmedUser = username.trim();
		const trimmedHost = host.trim();
		const session = createSession({
			title: quickConnectTitle(trimmedUser, trimmedHost),
			source: 'quick'
		});

		openTab(session);
		activeWorkspace = 'terminal';
		lifecycle.begin(session.id, {
			kind: 'quick',
			request: {
				sessionId: session.id,
				host: trimmedHost,
				port: Number(port),
				username: trimmedUser,
				auth,
				// Taken now, so later edits to the form cannot change what is sent.
				password: auth === 'password' ? password : undefined,
				privateKey: auth === 'privateKey' ? privateKey : undefined,
				passphrase: auth === 'privateKey' && passphrase.length > 0 ? passphrase : undefined
			}
		});
	}

	// --- saved hosts ------------------------------------------------------

	/**
	 * Opens the session a saved host describes, as a tab of its own.
	 *
	 * Only the host id and the terminal size leave the frontend: the address,
	 * the username and the stored secret are resolved by the backend, so no
	 * credential is ever copied into this component's state or into the Quick
	 * Connect form. A duplicate connection to the same host is allowed; a second
	 * click while this host's tab is still connecting is not.
	 */
	function connectSavedHost(savedHost: Host) {
		if (savedHostConnecting(sessions, savedHost.id)) return;

		const session = createSession({
			title: savedHost.label,
			source: 'saved',
			savedHostId: savedHost.id
		});
		openTab(session);
		activeWorkspace = 'terminal';
		lifecycle.begin(session.id, { kind: 'saved', hostId: savedHost.id });
	}

	// --- tabs -------------------------------------------------------------

	/** Adds a tab and brings it forward. */
	function openTab(session: TerminalSession) {
		sessions = [...sessions, session];
		activeSessionId = session.id;
	}

	function selectTab(sessionId: string) {
		// Switching tabs only changes what is on screen: no session is touched,
		// no error is cleared and no scrollback is thrown away.
		activeSessionId = sessionId;
		activeWorkspace = 'terminal';
	}

	/** Called by a tab once its terminal is usable. */
	function handleReady(sessionId: string, runtime: TerminalRuntime) {
		// A tab that was closed while it was mounting does not get a runtime.
		if (!findSession(sessions, sessionId)) return;
		runtimes[sessionId] = runtime;
		lifecycle.terminalReady(sessionId);
	}

	// --- disconnect and close ---------------------------------------------

	/**
	 * Closes one tab; the lifecycle stops that session and removes only that tab,
	 * and leaves it in place if the backend could not be stopped.
	 */
	async function closeTab(sessionId: string) {
		await lifecycle.close(sessionId);
		// The runtime belongs to the tab, so it goes only once the tab is gone.
		if (!findSession(sessions, sessionId)) delete runtimes[sessionId];
	}

	// --- host-key prompt --------------------------------------------------

	/**
	 * Answers the prompt of the tab it belongs to. The tab on screen can change
	 * between the prompt arriving and this being clicked, so the answer carries
	 * the session id of the prompt itself, never the active one.
	 */
	async function answerHostKey(accepted: boolean) {
		const prompt = activePrompt;
		if (!prompt) return;

		sessions = clearHostKeyPrompt(sessions, prompt.sessionId);
		try {
			await respondHostKey(prompt.sessionId, accepted);
		} catch (error) {
			sessions = setSessionError(sessions, prompt.sessionId, errorMessage(error));
		}

		// Another tab may be waiting for an answer of its own.
		const waiting = firstPromptSessionId(sessions);
		if (waiting !== null) activeSessionId = waiting;
	}
</script>

<AppShell>
	{#snippet titlebar()}
		<AppTitlebar
			{activeWorkspace}
			{sessions}
			{activeSessionId}
			{canDisconnect}
			onnavigateinventory={() => (activeWorkspace = 'inventory')}
			onselectsession={selectTab}
			onclosesession={closeTab}
			ondisconnect={() => void lifecycle.disconnect()}
		/>
	{/snippet}

	{#snippet sidebar()}
		<AppSidebar
			{activeWorkspace}
			collapsed={sidebarCollapsed}
			sessionCount={sessions.length}
			{vaultStatus}
			ontogglecollapse={() => (sidebarCollapsed = !sidebarCollapsed)}
			onnavigate={(workspace) => (activeWorkspace = workspace)}
		/>
	{/snippet}

	<div class="flex h-full min-h-0 flex-col">
		<!-- Global error notification if backend event subscriptions fail -->
		{#if eventsError}
			<div class="shrink-0 p-4 pb-0">
				<Alert variant="destructive">
					<AlertTitle>Session events unavailable</AlertTitle>
					<AlertDescription>{eventsError}</AlertDescription>
				</Alert>
			</div>
		{/if}

		<div class="relative min-h-0 flex-1 overflow-hidden">
			<!-- Inventory Workspace (kept mounted in DOM) -->
			<div class="h-full w-full overflow-y-auto p-4" class:hidden={activeWorkspace !== 'inventory'}>
				<div class="mx-auto flex max-w-5xl flex-col gap-4">
					<VaultPanel bind:status={vaultStatus} />

					<InventoryPanel
						{vaultStatus}
						sessionStatus={activeSession?.status ?? 'idle'}
						sessionId={activeSession?.id}
						sessionTarget={activeSession?.title ?? ''}
						hasLiveSessions={anyLiveSession}
						hostConnecting={(hostId) => savedHostConnecting(sessions, hostId)}
						onconnect={connectSavedHost}
						bind:quickHost={host}
						bind:quickPort={port}
						bind:quickUsername={username}
						bind:quickAuth={auth}
						bind:quickPassword={password}
						bind:quickPrivateKey={privateKey}
						bind:quickPassphrase={passphrase}
						{quickSubmitting}
						onquicksubmit={handleSubmit}
						onquickclear={clearCredentials}
					/>

					{#if activeSession?.errorMessage}
						<Alert variant="destructive">
							<AlertTitle>Connection failed</AlertTitle>
							<AlertDescription>{activeSession.errorMessage}</AlertDescription>
						</Alert>
					{:else if activeSession?.statusMessage}
						<Alert>
							<AlertDescription>{activeSession.statusMessage}</AlertDescription>
						</Alert>
					{/if}
				</div>
			</div>

			<!-- Terminal Workspace (kept mounted in DOM) -->
			<div
				class="relative flex h-full w-full flex-col overflow-hidden p-2"
				class:hidden={activeWorkspace !== 'terminal'}
			>
				{#if activeSession?.errorMessage}
					<div class="mb-2 shrink-0">
						<Alert variant="destructive">
							<AlertTitle>Connection failed ({activeSession.title})</AlertTitle>
							<AlertDescription>{activeSession.errorMessage}</AlertDescription>
						</Alert>
					</div>
				{/if}

				<div class="relative min-h-0 flex-1">
					{#if sessions.length === 0}
						<div class="flex h-full items-center justify-center rounded-lg border border-dashed">
							<div class="flex flex-col items-center gap-2 p-4 text-center">
								<p class="text-xs text-muted-foreground">No active terminal sessions.</p>
								<Button
									variant="outline"
									size="sm"
									class="h-7 text-xs"
									onclick={() => (activeWorkspace = 'inventory')}
								>
									Go to Hosts
								</Button>
							</div>
						</div>
					{/if}
					{#each sessions as session (session.id)}
						<TerminalSessionPane
							{session}
							active={activeWorkspace === 'terminal' && session.id === activeSessionId}
							onready={handleReady}
						/>
					{/each}
				</div>
			</div>
		</div>
	</div>
</AppShell>

<Dialog open={activePrompt !== null}>
	<DialogContent
		showCloseButton={false}
		interactOutsideBehavior="ignore"
		escapeKeydownBehavior="ignore"
		class="sm:max-w-md"
	>
		<DialogHeader>
			<DialogTitle>Verify the host key</DialogTitle>
			<DialogDescription>
				This is the first connection to {activeSession?.title ?? 'this host'}. Compare the
				fingerprint with the server operator before continuing, or cancel to stop the connection.
				{#if activePrompt}
					{hostKeyPromise(activePrompt.persistent)}
				{/if}
			</DialogDescription>
		</DialogHeader>

		{#if activePrompt}
			<dl class="grid gap-2 text-sm">
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">Host</dt>
					<dd class="font-mono">
						{activePrompt.host}:{activePrompt.port}
					</dd>
				</div>
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">Algorithm</dt>
					<dd class="font-mono">{activePrompt.algorithm}</dd>
				</div>
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">SHA-256 fingerprint</dt>
					<dd class="font-mono break-all">{activePrompt.fingerprint}</dd>
				</div>
			</dl>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={() => answerHostKey(false)}>Cancel</Button>
			<Button onclick={() => answerHostKey(true)}>Accept and connect</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>
