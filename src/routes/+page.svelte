<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';

	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import InventoryPanel from '#lib/components/inventory-panel.svelte';
	import SessionTabStrip from '#lib/components/session-tab-strip.svelte';
	// The component is aliased: `TerminalSession` is the session descriptor type.
	import TerminalSessionPane from '#lib/components/terminal-session.svelte';
	import VaultPanel from '#lib/components/vault-panel.svelte';
	import { Button } from '#lib/components/ui/button';
	import {
		Card,
		CardContent,
		CardDescription,
		CardHeader,
		CardTitle
	} from '#lib/components/ui/card';
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
	import { Separator } from '#lib/components/ui/separator';
	import { Textarea } from '#lib/components/ui/textarea';
	import {
		connectSsh,
		disconnectSsh,
		errorMessage,
		hostKeyPromise,
		respondHostKey,
		startSavedSsh,
		statusLabel
	} from '#lib/ssh';
	import { createSessionLifecycle } from '#lib/session-lifecycle';
	import {
		clearHostKeyPrompt,
		createSession,
		findSession,
		firstPromptSessionId,
		hasLiveSessions,
		isLiveStatus,
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
	 * what order. The tabs stay this component's Svelte state — the lifecycle
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

	// bits-ui reads a closed trigger's text from these, so the select shows a
	// label instead of the raw value it stores.
	const authMethods = [
		{ value: 'password', label: 'Password' },
		{ value: 'privateKey', label: 'Private key' }
	];

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

<div class="flex h-screen flex-col gap-4 p-4">
	<header class="flex items-center justify-between">
		<div>
			<h1 class="text-lg font-semibold">Termust</h1>
			<p class="text-xs text-muted-foreground">
				Direct SSH from this device. Saved hosts and credentials are stored in the local vault.
			</p>
		</div>
		<div class="flex items-center gap-2 text-sm">
			{#if activeSession}
				<span
					class="size-2 rounded-full"
					class:bg-emerald-500={activeSession.status === 'connected'}
					class:bg-amber-500={isLiveStatus(activeSession.status) &&
						activeSession.status !== 'connected'}
					class:bg-rose-500={activeSession.status === 'error'}
					class:bg-muted-foreground={!isLiveStatus(activeSession.status) &&
						activeSession.status !== 'error'}
					aria-hidden="true"
				></span>
				<span aria-live="polite">{statusLabel(activeSession.status)}</span>
			{:else}
				<span class="size-2 rounded-full bg-muted-foreground" aria-hidden="true"></span>
				<span aria-live="polite">{statusLabel('idle')}</span>
			{/if}
		</div>
	</header>

	<!-- The panels scroll next to the terminal instead of squeezing it: the
	     window keeps its height on every screen size. -->
	<div class="grid min-h-0 flex-1 auto-rows-max content-start gap-4 overflow-y-auto">
		<VaultPanel bind:status={vaultStatus} />

		<InventoryPanel
			{vaultStatus}
			sessionStatus={activeSession?.status ?? 'idle'}
			sessionId={activeSession?.id}
			sessionTarget={activeSession?.title ?? ''}
			hasLiveSessions={anyLiveSession}
			hostConnecting={(hostId) => savedHostConnecting(sessions, hostId)}
			onconnect={connectSavedHost}
		/>

		<Card>
			<CardHeader>
				<CardTitle>Quick Connect</CardTitle>
				<CardDescription>
					Connects straight from this device with credentials you type here. Saved hosts have their
					own Connect button and are not used here. Every Connect opens a tab of its own, so several
					sessions can run at once.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<form class="grid gap-3 sm:grid-cols-2" onsubmit={handleSubmit}>
					<div class="grid gap-1.5">
						<Label for="host">Hostname or IP</Label>
						<Input
							id="host"
							bind:value={host}
							placeholder="server.example.com"
							disabled={quickSubmitting}
						/>
					</div>
					<div class="grid gap-1.5">
						<Label for="port">Port</Label>
						<Input
							id="port"
							type="number"
							min="1"
							max="65535"
							bind:value={port}
							disabled={quickSubmitting}
						/>
					</div>
					<div class="grid gap-1.5">
						<Label for="username">Username</Label>
						<Input
							id="username"
							bind:value={username}
							placeholder="root"
							disabled={quickSubmitting}
						/>
					</div>
					<div class="grid gap-1.5">
						<Label for="auth">Authentication</Label>
						<Select type="single" bind:value={auth} items={authMethods} disabled={quickSubmitting}>
							<SelectTrigger id="auth" class="w-full">
								<SelectValue placeholder="Choose a method" />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="password">Password</SelectItem>
								<SelectItem value="privateKey">Private key</SelectItem>
							</SelectContent>
						</Select>
					</div>

					{#if auth === 'password'}
						<div class="grid gap-1.5 sm:col-span-2">
							<Label for="password">Password</Label>
							<Input
								id="password"
								type="password"
								bind:value={password}
								disabled={quickSubmitting}
							/>
						</div>
					{:else}
						<div class="grid gap-1.5 sm:col-span-2">
							<Label for="privateKey">Private key (OpenSSH format)</Label>
							<Textarea
								id="privateKey"
								rows={6}
								class="font-mono text-xs"
								placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
								bind:value={privateKey}
								disabled={quickSubmitting}
							/>
						</div>
						<div class="grid gap-1.5 sm:col-span-2">
							<Label for="passphrase">Passphrase (only if the key is encrypted)</Label>
							<Input
								id="passphrase"
								type="password"
								bind:value={passphrase}
								disabled={quickSubmitting}
							/>
						</div>
					{/if}

					<div class="flex items-center gap-2 sm:col-span-2">
						<Button type="submit" disabled={quickSubmitting}>
							{quickSubmitting ? 'Connecting…' : 'Connect'}
						</Button>
						<Button
							type="button"
							variant="outline"
							onclick={() => void lifecycle.disconnect()}
							disabled={!canDisconnect}
							title="Closes the connection but keeps the tab"
						>
							Disconnect
						</Button>
					</div>
				</form>
			</CardContent>
		</Card>

		{#if eventsError}
			<Alert variant="destructive">
				<AlertTitle>Session events unavailable</AlertTitle>
				<AlertDescription>{eventsError}</AlertDescription>
			</Alert>
		{:else if activeSession?.errorMessage}
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

	<Separator />

	<SessionTabStrip {sessions} {activeSessionId} onselect={selectTab} onclose={closeTab} />

	<!-- Every tab stays mounted and only the active one is shown: that is what
	     keeps a session's scrollback while the user is looking at another. -->
	<div class="relative min-h-0 flex-1">
		{#if sessions.length === 0}
			<div class="flex h-full items-center justify-center rounded-lg border border-dashed">
				<p class="text-xs text-muted-foreground">Connect to a host to open a terminal tab.</p>
			</div>
		{/if}
		{#each sessions as session (session.id)}
			<TerminalSessionPane
				{session}
				active={session.id === activeSessionId}
				onready={handleReady}
			/>
		{/each}
	</div>

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
</div>
