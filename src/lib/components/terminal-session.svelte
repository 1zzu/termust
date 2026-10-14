<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { FitAddon } from '@xterm/addon-fit';
	import { Terminal } from '@xterm/xterm';
	import '@xterm/xterm/css/xterm.css';

	import { resizeSsh, writeSsh } from '#lib/ssh';
	import {
		inputToSend,
		panelElementId,
		resizeToSend,
		tabElementId,
		type TerminalRuntime,
		type TerminalSession
	} from '#lib/session-tabs';

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
		onready
	}: {
		session: TerminalSession;
		/** Whether this tab is the one on screen. */
		active?: boolean;
		/** Called once the terminal exists, so the page can start the session. */
		onready?: (sessionId: string, runtime: TerminalRuntime) => void;
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
		fitAddon.fit();

		const outgoing = resizeToSend(session, terminal.cols, terminal.rows);
		if (!outgoing) return;
		if (outgoing.cols === reported.cols && outgoing.rows === reported.rows) return;
		reported = { cols: outgoing.cols, rows: outgoing.rows };
		void resizeSsh(outgoing.sessionId, outgoing.cols, outgoing.rows).catch(() => {
			// As above: the session's status is where a failure shows up.
		});
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

	// A shell that just came up is told the current size too, in case the window
	// was resized while the session was still connecting.
	$effect(() => {
		if (active && session.status === 'connected') {
			void tick().then(() => fit());
		}
	});
</script>

<div
	bind:this={container}
	id={panelElementId(session.id)}
	role="tabpanel"
	aria-labelledby={tabElementId(session.id)}
	hidden={!active}
	class="absolute inset-0 overflow-hidden rounded-lg border bg-terminal-background p-2"
></div>
