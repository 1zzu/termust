import { listen } from '@tauri-apps/api/event';
import { connectSsh, disconnectSsh, errorMessage, respondHostKey, startSavedSsh } from '#lib/ssh';
import { createSessionLifecycle, type SessionLifecycle } from '#lib/session-lifecycle';
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
import { inventoryStore } from '#lib/stores/inventory-data.svelte';

class TerminalStore {
	sessions = $state<TerminalSession[]>([]);
	activeSessionId = $state<string | null>(null);
	quickSubmitting = $state(false);
	eventsError = $state('');

	// Quick Connect Form State
	quickHost = $state('localhost');
	quickPort = $state(22);
	quickUsername = $state('');
	quickAuth = $state<AuthMethod>('password');
	quickPassword = $state('');
	quickPrivateKey = $state('');
	quickPassphrase = $state('');
	openQuickConnect = $state(false);

	activeSession = $derived(findSession(this.sessions, this.activeSessionId));
	anyLiveSession = $derived(hasLiveSessions(this.sessions));
	canDisconnect = $derived(
		this.activeSession !== undefined && sessionCanDisconnect(this.activeSession)
	);
	activePrompt = $derived(this.activeSession?.hostKeyPrompt ?? null);

	private runtimes: Record<string, TerminalRuntime> = {};
	private lifecycle: SessionLifecycle | null = null;
	private attached = false;

	private getLifecycle(): SessionLifecycle {
		if (!this.lifecycle) {
			this.lifecycle = createSessionLifecycle({
				read: () => this.sessions,
				write: (next) => {
					this.sessions = next;
				},
				readActive: () => this.activeSessionId,
				writeActive: (sessionId) => {
					this.activeSessionId = sessionId;
				},
				listen: <P>(event: string, handler: (payload: P) => void) =>
					listen<P>(event, ({ payload }) => handler(payload)),
				connect: (request) => connectSsh(request),
				startSaved: (request) => startSavedSsh(request),
				disconnect: (sessionId) => disconnectSsh(sessionId),
				size: (sessionId) => this.runtimes[sessionId]?.size() ?? { cols: 80, rows: 24 },
				writeTerminal: (sessionId, bytes) => {
					this.runtimes[sessionId]?.write(bytes);
				},
				onEventsUnavailable: (error) => {
					this.eventsError = errorMessage(error);
				},
				onQuickStartFinished: () => {
					this.clearCredentials();
					this.quickSubmitting = false;
				}
			});
		}
		return this.lifecycle;
	}

	init() {
		const lc = this.getLifecycle();
		if (this.attached) return;
		this.attached = true;
		lc.attach();
	}

	async destroy() {
		if (!this.attached) return;
		this.attached = false;
		if (this.lifecycle) {
			const lc = this.lifecycle;
			this.lifecycle = null;
			await lc.teardown();
		}
	}

	clearCredentials() {
		this.quickPassword = '';
		this.quickPrivateKey = '';
		this.quickPassphrase = '';
	}

	openTab(session: TerminalSession) {
		this.sessions = [...this.sessions, session];
		this.activeSessionId = session.id;
	}

	selectTab(sessionId: string) {
		this.activeSessionId = sessionId;
	}

	handleReady(sessionId: string, runtime: TerminalRuntime) {
		if (!findSession(this.sessions, sessionId)) return;
		this.runtimes[sessionId] = runtime;
		this.getLifecycle().terminalReady(sessionId);
	}

	async closeTab(sessionId: string) {
		await this.getLifecycle().close(sessionId);
		if (!findSession(this.sessions, sessionId)) delete this.runtimes[sessionId];
	}

	async reconnectSession(sessionId: string) {
		const session = findSession(this.sessions, sessionId);
		if (!session) return;
		const savedHostId = session.savedHostId;
		await this.closeTab(sessionId);
		if (savedHostId) {
			const host = inventoryStore.hosts.find((h) => h.id === savedHostId);
			if (host) {
				this.connectSavedHost(host);
			}
		}
	}

	async disconnectActive() {
		await this.getLifecycle().disconnect();
	}

	connectSavedHost(savedHost: Host) {
		this.init();
		if (savedHostConnecting(this.sessions, savedHost.id)) return;

		const session = createSession({
			title: savedHost.label,
			source: 'saved',
			savedHostId: savedHost.id
		});
		this.openTab(session);
		this.getLifecycle().begin(session.id, { kind: 'saved', hostId: savedHost.id });
	}

	isHostConnecting(hostId: string): boolean {
		return savedHostConnecting(this.sessions, hostId);
	}

	submitQuickConnect(event?: SubmitEvent) {
		event?.preventDefault();
		if (this.quickSubmitting) return;
		this.init();
		this.quickSubmitting = true;

		const trimmedUser = this.quickUsername.trim();
		const trimmedHost = this.quickHost.trim();
		const session = createSession({
			title: quickConnectTitle(trimmedUser, trimmedHost),
			source: 'quick'
		});

		this.openTab(session);
		this.getLifecycle().begin(session.id, {
			kind: 'quick',
			request: {
				sessionId: session.id,
				host: trimmedHost,
				port: Number(this.quickPort),
				username: trimmedUser,
				auth: this.quickAuth,
				password: this.quickAuth === 'password' ? this.quickPassword : undefined,
				privateKey: this.quickAuth === 'privateKey' ? this.quickPrivateKey : undefined,
				passphrase:
					this.quickAuth === 'privateKey' && this.quickPassphrase.length > 0
						? this.quickPassphrase
						: undefined
			}
		});
	}

	async answerHostKey(accepted: boolean) {
		const prompt = this.activePrompt;
		if (!prompt) return;

		this.sessions = clearHostKeyPrompt(this.sessions, prompt.sessionId);
		try {
			await respondHostKey(prompt.sessionId, accepted);
		} catch (error) {
			this.sessions = setSessionError(this.sessions, prompt.sessionId, errorMessage(error));
		}

		const waiting = firstPromptSessionId(this.sessions);
		if (waiting !== null) this.activeSessionId = waiting;
	}
}

export const terminalStore = new TerminalStore();
