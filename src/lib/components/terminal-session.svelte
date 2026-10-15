<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';

	import { disconnectSsh, resizeSsh, writeSsh } from '#lib/ssh';
	import {
		inputToSend,
		panelElementId,
		resizeToSend,
		tabElementId,
		type TerminalRuntime,
		type TerminalSession
	} from '#lib/session-tabs';
	import ConnectingOverlay from './connecting-overlay.svelte';

	/**
	 * One tab's terminal.
	 *
	 * Every tab has its own `Terminal`, its own `FitAddon`, its own container and
	 * its own resize handling, so nothing one session does can reach another.
	 * The tab component owns the xterm instance for its whole life: switching
	 * tabs hides it instead of disposing it, which is what keeps its scrollback.
	 */
	let {
		session,
		active = false,
		onready,
		onclose,
		onretry
	}: {
		session: TerminalSession;
		/** Whether this tab is the one on screen. */
		active?: boolean;
		/** Called once the terminal exists, so the page can start the session. */
		onready?: (sessionId: string, runtime: TerminalRuntime) => void;
		onclose?: (sessionId: string) => void;
		onretry?: (sessionId: string) => void;
	} = $props();

	let container: HTMLDivElement | null = $state(null);
	let terminal: Terminal | undefined;
	let fitAddon: FitAddon | undefined;
	let resizeObserver: ResizeObserver | undefined;
	let disposed = false;
	/** The last size the backend was told about, so only a real change is sent. */
	let reported = { cols: 0, rows: 0 };

	const runtime: TerminalRuntime = {
		write(bytes) {
			if (disposed) return;
			terminal?.write(bytes);
		},
		fit,
		focus() {
			if (disposed) return;
			terminal?.focus();
		},
		size() {
			return { cols: terminal?.cols ?? 80, rows: terminal?.rows ?? 24 };
		}
	};

	onMount(() => {
		terminal = new Terminal({
			cursorBlink: true,
			fontSize: 13,
			scrollback: 5000,
			theme: { background: '#0a0a0a', foreground: '#fafafa', cursor: '#fafafa' }
		});
		fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);

		terminal.onData((data) => {
			// The session id comes from this tab, so a keystroke can only reach
			// the session whose terminal produced it.
			const outgoing = inputToSend(session, data);
			if (!outgoing) return;
			void writeSsh(outgoing.sessionId, outgoing.data).catch(() => {
				// A refused keystroke is reported by the session's own status;
				// there is nothing useful to add here.
			});
		});

		if (container) {
			terminal.open(container);
			// The observer is scoped to this tab's container and observes
			// nothing else.
			resizeObserver = new ResizeObserver(() => {
				if (active) fit();
			});
			resizeObserver.observe(container);
		}

		// Only now is there a terminal to write to, so this is the earliest a
		// connection may be started without dropping the first output.
		onready?.(session.id, runtime);

		return () => {
			disposed = true;
			resizeObserver?.disconnect();
			resizeObserver = undefined;
			terminal?.dispose();
			terminal = undefined;
			fitAddon = undefined;
		};
	});

	/**
	 * Fits this terminal to its own container and tells the backend the size it
	 * ended up with.
	 *
	 * A hidden tab has no size at all, and fitting it would ask the remote PTY
	 * for a zero-sized window, so that is never done.
	 */
	function fit() {
		if (disposed || !active || !fitAddon || !terminal || !container) return;
		if (container.offsetWidth === 0 || container.offsetHeight === 0) return;
		try {
			fitAddon.fit();

			const outgoing = resizeToSend(session, terminal.cols, terminal.rows);
			if (!outgoing) return;
			if (outgoing.cols === reported.cols && outgoing.rows === reported.rows) return;
			reported = { cols: outgoing.cols, rows: outgoing.rows };
			void resizeSsh(outgoing.sessionId, outgoing.cols, outgoing.rows).catch(() => {
				// As above: the session's status is where a failure shows up.
			});
		} catch {
			// Ignore intermediate measurement errors during layout shifts
		}
	}

	// Coming forward is what makes a tab usable: fit it, focus it, and report
	// the size the layout gave it.
	$effect(() => {
		if (!active) return;
		void tick().then(() => {
			fit();
			runtime.focus();
		});
	});

	const isConnectingPhase = $derived(
		session.status === 'idle' ||
		session.status === 'connecting' ||
		session.status === 'verifying_host_key' ||
		session.status === 'authenticating' ||
		session.status === 'error'
	);

	let transitionDone = $state(false);

	$effect(() => {
		if (session.status === 'connected') {
			const timer = setTimeout(() => {
				transitionDone = true;
				void tick().then(() => {
					fit();
					runtime.focus();
				});
			}, 500);
			return () => clearTimeout(timer);
		} else {
			transitionDone = false;
		}
	});

	const overlayVisible = $derived(
		isConnectingPhase || (session.status === 'connected' && !transitionDone)
	);

	// A shell that just came up is told the current size too, in case the window
	// was resized while the session was still connecting.
	$effect(() => {
		if (active && session.status === 'connected' && !overlayVisible) {
			void tick().then(() => {
				fit();
				runtime.focus();
			});
		}
	});
</script>

<div
	id={panelElementId(session.id)}
	role="tabpanel"
	aria-labelledby={tabElementId(session.id)}
	hidden={!active}
	class="absolute inset-0 overflow-hidden rounded-lg border bg-terminal-background"
>
	<div
		bind:this={container}
		class="h-full w-full p-2 transition-opacity duration-200"
		class:invisible={overlayVisible}
		class:opacity-0={overlayVisible}
		class:opacity-100={!overlayVisible}
	></div>

	{#if overlayVisible}
		<ConnectingOverlay
			{session}
			oncancel={() => {
				if (session.status === 'error') {
					onclose?.(session.id);
				} else {
					void disconnectSsh(session.id);
				}
			}}
			onclose={() => onclose?.(session.id)}
			onretry={() => onretry?.(session.id)}
		/>
	{/if}
</div>
