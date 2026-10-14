<script lang="ts">
	import { onMount } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';

	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import InventoryPanel from '#lib/components/inventory-panel.svelte';
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
		SSH_EVENT,
		belongsToSession,
		connectSsh,
		decodeOutput,
		disconnectSsh,
		errorMessage,
		isConnected,
		resizeSsh,
		respondHostKey,
		statusLabel,
		writeSsh
	} from '#lib/ssh';
	import type {
		AuthMethod,
		ConnectionStatus,
		DataEvent,
		ErrorEvent,
		HostKeyEvent,
		StatusEvent
	} from '#lib/types';
	import type { VaultStatus } from '#lib/vault';

	// The vault state lives here so the saved inventory appears and disappears
	// with it; the vault panel owns the password fields themselves.
	let vaultStatus = $state<VaultStatus | undefined>(undefined);

	let host = $state('localhost');
	let port = $state(22);
	let username = $state('');
	let auth = $state<AuthMethod>('password');
	let password = $state('');
	let privateKey = $state('');
	let passphrase = $state('');

	let status = $state<ConnectionStatus>('idle');
	let statusMessage = $state('');
	let errorTitle = $state('Connection failed');
	let errorText = $state('');
	let hostKeyPrompt = $state<HostKeyEvent | null>(null);

	let terminalContainer = $state<HTMLDivElement | null>(null);
	let sessionId = $state<string | undefined>(undefined);
	let terminal: Terminal | undefined;
	let fitAddon: FitAddon | undefined;
	let resizeObserver: ResizeObserver | undefined;

	// bits-ui reads a closed trigger's text from these, so the select shows a
	// label instead of the raw value it stores.
	const authMethods = [
		{ value: 'password', label: 'Password' },
		{ value: 'privateKey', label: 'Private key' }
	];

	const busy = $derived(
		status === 'connecting' || status === 'verifying_host_key' || status === 'authenticating'
	);
	const connected = $derived(isConnected(status));

	onMount(() => {
		terminal = new Terminal({
			cursorBlink: true,
			fontSize: 13,
			scrollback: 5000,
			theme: { background: '#0a0a0a', foreground: '#fafafa', cursor: '#fafafa' }
		});
		fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);
		if (terminalContainer) {
			terminal.open(terminalContainer);
			fitAddon.fit();
			resizeObserver = new ResizeObserver(() => fitTerminal());
			resizeObserver.observe(terminalContainer);
		}

		terminal.onData((data) => {
			if (sessionId && isConnected(status)) {
				void writeSsh(sessionId, data).catch(handleFailure);
			}
		});

		let disposed = false;
		const unlisteners: UnlistenFn[] = [];
		void registerListeners()
			.then((registered) => {
				if (disposed) {
					registered.forEach((unlisten) => unlisten());
					return;
				}
				unlisteners.push(...registered);
			})
			.catch((error) => {
				// Without these subscriptions a session would run with no visible
				// output, so the failure has to reach the user.
				showError('Session events unavailable', error);
			});

		return () => {
			disposed = true;
			unlisteners.forEach((unlisten) => unlisten());
			resizeObserver?.disconnect();
			terminal?.dispose();
			if (sessionId) {
				void disconnectSsh(sessionId).catch(() => {
					// The window is closing; the backend cleans up on its own.
				});
			}
		};
	});

	async function registerListeners(): Promise<UnlistenFn[]> {
		return Promise.all([
			listen<DataEvent>(SSH_EVENT.data, ({ payload }) => {
				if (!belongsToSession(payload, sessionId ?? '')) return;
				terminal?.write(decodeOutput(payload.data));
			}),
			listen<StatusEvent>(SSH_EVENT.status, ({ payload }) => {
				if (!belongsToSession(payload, sessionId ?? '')) return;
				applyStatus(payload.status, payload.message ?? '');
			}),
			listen<ErrorEvent>(SSH_EVENT.error, ({ payload }) => {
				if (!belongsToSession(payload, sessionId ?? '')) return;
				errorTitle = 'Connection failed';
				errorText = payload.message;
			}),
			listen<HostKeyEvent>(SSH_EVENT.hostKey, ({ payload }) => {
				if (!belongsToSession(payload, sessionId ?? '')) return;
				hostKeyPrompt = payload;
			})
		]);
	}

	/** Keeps the credentials in this component and nowhere else, then drops them
	 * as soon as the session is over. */
	function applyStatus(next: ConnectionStatus, message: string) {
		status = next;
		statusMessage = message;
		if (next === 'disconnected' || next === 'error') {
			clearCredentials();
		}
	}

	function clearCredentials() {
		password = '';
		privateKey = '';
		passphrase = '';
	}

	function handleFailure(error: unknown) {
		showError('Session error', error);
	}

	function showError(title: string, error: unknown) {
		errorTitle = title;
		errorText = errorMessage(error);
	}

	async function connect() {
		if (!terminal) return;

		errorTitle = 'Connection failed';
		errorText = '';
		statusMessage = '';
		hostKeyPrompt = null;

		const id = crypto.randomUUID();
		sessionId = id;
		status = 'connecting';

		try {
			await connectSsh({
				sessionId: id,
				host: host.trim(),
				port: Number(port),
				username: username.trim(),
				auth,
				cols: terminal.cols,
				rows: terminal.rows,
				password: auth === 'password' ? password : undefined,
				privateKey: auth === 'privateKey' ? privateKey : undefined,
				passphrase: auth === 'privateKey' && passphrase.length > 0 ? passphrase : undefined
			});
		} catch (error) {
			showError('Connection failed', error);
			applyStatus('error', '');
			sessionId = undefined;
		}
	}

	async function disconnect() {
		const id = sessionId;
		sessionId = undefined;
		hostKeyPrompt = null;
		applyStatus('disconnected', '');
		if (id) {
			await disconnectSsh(id).catch(handleFailure);
		}
	}

	async function answerHostKey(accepted: boolean) {
		const prompt = hostKeyPrompt;
		hostKeyPrompt = null;
		if (!prompt) return;
		await respondHostKey(prompt.sessionId, accepted).catch(handleFailure);
	}

	function fitTerminal() {
		if (!fitAddon || !terminal) return;
		fitAddon.fit();
		if (sessionId && isConnected(status)) {
			void resizeSsh(sessionId, terminal.cols, terminal.rows).catch(handleFailure);
		}
	}

	function handleSubmit(event: SubmitEvent) {
		event.preventDefault();
		if (!busy && !connected) void connect();
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
			<span
				class="size-2 rounded-full"
				class:bg-emerald-500={connected}
				class:bg-amber-500={busy}
				class:bg-rose-500={status === 'error'}
				class:bg-muted-foreground={!connected && !busy && status !== 'error'}
				aria-hidden="true"
			></span>
			<span aria-live="polite">{statusLabel(status)}</span>
		</div>
	</header>

	<!-- The panels scroll next to the terminal instead of squeezing it: the
	     window keeps its height on every screen size. -->
	<div class="grid min-h-0 flex-1 auto-rows-max content-start gap-4 overflow-y-auto">
		<VaultPanel bind:status={vaultStatus} />

		<InventoryPanel status={vaultStatus} />

		<Card>
			<CardHeader>
				<CardTitle>Quick Connect</CardTitle>
				<CardDescription>
					Connects straight from this device with credentials you type here. Saved hosts are not
					used to connect yet.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<form class="grid gap-3 sm:grid-cols-2" onsubmit={handleSubmit}>
					<div class="grid gap-1.5">
						<Label for="host">Hostname or IP</Label>
						<Input id="host" bind:value={host} placeholder="server.example.com" disabled={busy} />
					</div>
					<div class="grid gap-1.5">
						<Label for="port">Port</Label>
						<Input id="port" type="number" min="1" max="65535" bind:value={port} disabled={busy} />
					</div>
					<div class="grid gap-1.5">
						<Label for="username">Username</Label>
						<Input id="username" bind:value={username} placeholder="root" disabled={busy} />
					</div>
					<div class="grid gap-1.5">
						<Label for="auth">Authentication</Label>
						<Select type="single" bind:value={auth} items={authMethods} disabled={busy}>
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
							<Input id="password" type="password" bind:value={password} disabled={busy} />
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
								disabled={busy}
							/>
						</div>
						<div class="grid gap-1.5 sm:col-span-2">
							<Label for="passphrase">Passphrase (only if the key is encrypted)</Label>
							<Input id="passphrase" type="password" bind:value={passphrase} disabled={busy} />
						</div>
					{/if}

					<div class="flex items-center gap-2 sm:col-span-2">
						<Button type="submit" disabled={busy || connected}>
							{busy ? 'Connecting…' : 'Connect'}
						</Button>
						<Button type="button" variant="outline" onclick={disconnect} disabled={!sessionId}>
							Disconnect
						</Button>
					</div>
				</form>
			</CardContent>
		</Card>

		{#if errorText}
			<Alert variant="destructive">
				<AlertTitle>{errorTitle}</AlertTitle>
				<AlertDescription>{errorText}</AlertDescription>
			</Alert>
		{:else if statusMessage}
			<Alert>
				<AlertDescription>{statusMessage}</AlertDescription>
			</Alert>
		{/if}
	</div>

	<Separator />

	<div
		bind:this={terminalContainer}
		class="min-h-0 flex-1 overflow-hidden rounded-lg border bg-terminal-background p-2"
	></div>

	<Dialog open={hostKeyPrompt !== null}>
		<DialogContent
			showCloseButton={false}
			interactOutsideBehavior="ignore"
			escapeKeydownBehavior="ignore"
			class="sm:max-w-md"
		>
			<DialogHeader>
				<DialogTitle>Verify the host key</DialogTitle>
				<DialogDescription>
					This is the first connection to this host. Compare the fingerprint with the server
					operator before continuing, or cancel to stop the connection.
				</DialogDescription>
			</DialogHeader>

			{#if hostKeyPrompt}
				<dl class="grid gap-2 text-sm">
					<div class="grid gap-0.5">
						<dt class="text-xs text-muted-foreground">Host</dt>
						<dd class="font-mono">
							{hostKeyPrompt.host}:{hostKeyPrompt.port}
						</dd>
					</div>
					<div class="grid gap-0.5">
						<dt class="text-xs text-muted-foreground">Algorithm</dt>
						<dd class="font-mono">{hostKeyPrompt.algorithm}</dd>
					</div>
					<div class="grid gap-0.5">
						<dt class="text-xs text-muted-foreground">SHA-256 fingerprint</dt>
						<dd class="font-mono break-all">{hostKeyPrompt.fingerprint}</dd>
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
