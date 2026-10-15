<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import type { TerminalSession } from '#lib/session-tabs';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
	import { Button } from '#lib/components/ui/button';
	import AlertCircleIcon from '@lucide/svelte/icons/alert-circle';
	import PlugIcon from '@lucide/svelte/icons/plug-2';
	import RotateCwIcon from '@lucide/svelte/icons/rotate-cw';
	import XIcon from '@lucide/svelte/icons/x';
	import { Cpu } from '@lucide/svelte';

	let {
		session,
		oncancel,
		onretry,
		onclose
	}: {
		session: TerminalSession;
		oncancel?: () => void;
		onretry?: () => void;
		onclose?: () => void;
	} = $props();

	let showLogs = $state(false);

	interface LogEntry {
		time: string;
		message: string;
	}

	let logs = $state<LogEntry[]>([]);

	function nowTime(): string {
		const d = new Date();
		return d.toTimeString().split(' ')[0];
	}

	function addLog(message: string) {
		untrack(() => {
			if (!logs.some((l) => l.message === message)) {
				logs = [...logs, { time: nowTime(), message }];
			}
		});
	}

	const savedHost = $derived(
		session.savedHostId ? inventoryStore.hosts.find((h) => h.id === session.savedHostId) : null
	);

	const displayTitle = $derived(savedHost?.label || session.title || 'SSH Connection');

	const displaySubtitle = $derived.by(() => {
		if (savedHost) {
			return `SSH ${savedHost.address}:${savedHost.port}`;
		}
		if (session.title.includes('@')) {
			return `SSH ${session.title.split('@')[1]}`;
		}
		return `SSH ${session.title}`;
	});

	const isError = $derived(session.status === 'error' || Boolean(session.errorMessage));
	const errorMessageText = $derived(
		session.errorMessage ||
			(session.status === 'error'
				? 'Authentication failed. Please check your username, password, or SSH key.'
				: '')
	);
	const canRetry = $derived(Boolean(session.savedHostId) && Boolean(onretry));

	const progress = $derived.by(() => {
		if (isError) return 100;
		switch (session.status) {
			case 'idle':
				return 15;
			case 'connecting':
				return 40;
			case 'verifying_host_key':
				return 70;
			case 'authenticating':
				return 88;
			case 'connected':
				return 100;
			default:
				return 0;
		}
	});

	onMount(() => {
		addLog(`Opening connection stream to ${displaySubtitle}`);
		addLog(`Status: ${session.status}`);
	});

	$effect(() => {
		const currentStatus = session.status;
		if (currentStatus === 'connecting') {
			addLog(`Initiating SSH connection to ${displaySubtitle}...`);
		} else if (currentStatus === 'verifying_host_key') {
			addLog('Verifying remote host key and fingerprint...');
		} else if (currentStatus === 'authenticating') {
			addLog('Authenticating with credentials...');
		} else if (currentStatus === 'connected') {
			addLog('Connection established successfully.');
		} else if (currentStatus === 'error') {
			addLog(`Connection failed: ${errorMessageText || 'Unknown error'}`);
		}
	});

	$effect(() => {
		const msg = session.statusMessage;
		if (msg) {
			addLog(msg);
		}
	});

	$effect(() => {
		const err = session.errorMessage;
		if (err) {
			addLog(`Error: ${err}`);
		}
	});
</script>

<div
	class="absolute inset-0 z-20 flex flex-col items-center justify-center bg-[#0B0F19] p-6 text-foreground select-none"
	role="status"
	aria-live="polite"
	aria-label="Connecting to {displayTitle}"
>
	<div class="flex w-full max-w-md flex-col">
		<!-- Header: OS Avatar, Host details & Show logs button -->
		<div class="flex items-center justify-between gap-4">
			<div class="flex min-w-0 items-center gap-3">
				<!-- Rounded squircle avatar with Ubuntu logo -->
				<div
					class="flex size-11 shrink-0 items-center justify-center rounded-xl bg-[#E95420] text-white shadow-xs"
					aria-hidden="true"
				>
					<Cpu class="size-5" />
					<!-- <svg viewBox="0 0 24 24" class="size-6 fill-current">
						<circle cx="12" cy="12" r="10" fill="#E95420" />
						<path
							fill="#ffffff"
							d="M12 4.5a7.5 7.5 0 0 0-7.3 5.8 2 2 0 0 1 .8.4 2 2 0 0 1 .7 1.3 7.5 7.5 0 0 0 0 0 2 2 0 0 1-.7 1.3 2 2 0 0 1-.8.4A7.5 7.5 0 1 0 12 4.5zm0 1.8a5.7 5.7 0 0 1 4.5 2.2 2 2 0 0 1-.9.7 2 2 0 0 1-.3 1.5 5.7 5.7 0 0 1 0 2.6 2 2 0 0 1 .3 1.5 2 2 0 0 1 .9.7 5.7 5.7 0 0 1-9 0 2 2 0 0 1 .9-.7 2 2 0 0 1 .3-1.5 5.7 5.7 0 0 1 0-2.6 2 2 0 0 1-.3-1.5 2 2 0 0 1-.9-.7A5.7 5.7 0 0 1 12 6.3z"
						/>
					</svg> -->
				</div>

				<div class="flex min-w-0 flex-col">
					<h3 class="truncate text-sm font-semibold tracking-tight text-white">
						{displayTitle}
					</h3>
					<p class="truncate font-mono text-xs text-slate-400">
						{displaySubtitle}
					</p>
				</div>
			</div>

			<!-- Show logs pill button -->
			<button
				type="button"
				onclick={() => (showLogs = !showLogs)}
				class="shrink-0 cursor-pointer rounded-lg border border-slate-700/60 bg-slate-800/80 px-3 py-1.5 text-xs font-medium text-slate-200 shadow-xs transition-colors hover:bg-slate-700 hover:text-white"
			>
				{showLogs ? 'Hide logs' : 'Show logs'}
			</button>
		</div>

		<!-- Animated connection track -->
		<div class="relative mt-8 flex items-center justify-between">
			<!-- Left node -->
			<div class="relative flex size-12 shrink-0 items-center justify-center">
				{#if isError}
					<div
						class="relative flex size-9 items-center justify-center rounded-full bg-red-500 text-white shadow-md shadow-red-500/40"
					>
						<AlertCircleIcon class="size-4.5" />
					</div>
				{:else}
					<!-- Single spinning arc -->
					<svg
						class="pointer-events-none absolute inset-0 size-full animate-spin text-sky-400"
						viewBox="0 0 48 48"
						style="animation-duration: 1.8s;"
						aria-hidden="true"
					>
						<circle
							cx="24"
							cy="24"
							r="19"
							stroke="#38bdf8"
							stroke-width="3"
							stroke-dasharray="80 40"
							stroke-linecap="round"
							fill="none"
						/>
					</svg>

					<!-- Inner blue circle with Plug icon -->
					<div
						class="relative flex size-8 items-center justify-center rounded-full bg-sky-500 text-white shadow-md shadow-sky-500/30"
					>
						<PlugIcon class="size-4 -rotate-45" />
					</div>
				{/if}
			</div>

			<!-- Connecting horizontal line with data-driven progress fill -->
			<div class="relative mx-1.5 h-1 flex-1 overflow-hidden rounded-full bg-slate-700/70">
				<div
					class="h-full rounded-full transition-all duration-500 ease-out {isError
						? 'bg-red-500 shadow-[0_0_10px_rgba(239,68,68,0.7)]'
						: progress === 100
							? 'bg-emerald-400 shadow-[0_0_10px_rgba(52,211,153,0.7)]'
							: 'bg-sky-400 shadow-[0_0_8px_rgba(56,189,248,0.5)]'}"
					style="width: {progress}%;"
				></div>
			</div>

			<!-- Right node -->
			<div
				class="relative flex size-8 shrink-0 items-center justify-center rounded-full text-white transition-all duration-300 {isError
					? 'border border-red-500/50 bg-red-500/20 text-red-400 shadow-md shadow-red-500/20'
					: progress === 100
						? 'scale-105 bg-emerald-500 shadow-md shadow-emerald-500/50'
						: 'bg-sky-500 shadow-md shadow-sky-500/30'}"
			>
				{#if isError}
					<XIcon class="size-4" />
				{:else}
					<span class="font-mono text-xs font-bold tracking-tight select-none">&gt;_</span>
				{/if}
			</div>
		</div>

		<!-- Error Details Panel with Actions -->
		{#if isError}
			<div
				class="mt-6 flex flex-col gap-3 rounded-xl border border-red-500/30 bg-red-950/40 p-4 shadow-lg backdrop-blur-sm"
			>
				<div class="flex items-start gap-3">
					<AlertCircleIcon class="mt-0.5 size-4.5 shrink-0 text-red-400" />
					<div class="flex min-w-0 flex-1 flex-col gap-1">
						<h4 class="text-xs font-semibold text-red-200">Authentication or Connection Failed</h4>
						<p class="text-xs leading-relaxed break-words text-red-300/90">
							{errorMessageText}
						</p>
					</div>
				</div>

				<div class="flex items-center gap-2 border-t border-red-500/20 pt-1">
					{#if canRetry}
						<Button
							size="sm"
							class="h-7.5 gap-1.5 bg-red-600 text-xs text-white shadow-xs hover:bg-red-500"
							onclick={onretry}
						>
							<RotateCwIcon class="size-3.5" />
							Try Again
						</Button>
					{/if}

					{#if onclose}
						<Button
							variant="outline"
							size="sm"
							class="h-7.5 border-slate-700 bg-slate-800/80 text-xs text-slate-200 hover:bg-slate-700 hover:text-white"
							onclick={onclose}
						>
							Close Tab
						</Button>
					{:else if oncancel}
						<Button
							variant="outline"
							size="sm"
							class="h-7.5 border-slate-700 bg-slate-800/80 text-xs text-slate-200 hover:bg-slate-700 hover:text-white"
							onclick={oncancel}
						>
							Close
						</Button>
					{/if}
				</div>
			</div>
		{:else if oncancel}
			<div class="mt-6 flex justify-center">
				<button
					type="button"
					onclick={oncancel}
					class="cursor-pointer text-xs text-slate-400 transition-colors hover:text-slate-200"
				>
					Cancel connection
				</button>
			</div>
		{/if}

		<!-- Expandable logs panel -->
		{#if showLogs}
			<div
				class="mt-4 w-full rounded-lg border border-slate-800 bg-slate-900/90 p-3 shadow-lg backdrop-blur-sm"
			>
				<div class="flex items-center justify-between border-b border-slate-800/80 pb-2">
					<span class="text-[11px] font-medium tracking-wider text-slate-400 uppercase">
						Connection logs
					</span>
					{#if oncancel && !isError}
						<button
							type="button"
							onclick={oncancel}
							class="cursor-pointer text-[11px] font-medium text-red-400 hover:text-red-300"
						>
							Cancel connection
						</button>
					{/if}
				</div>
				<div
					class="mt-2 flex max-h-36 flex-col gap-1 overflow-y-auto font-mono text-[11px] text-slate-300"
				>
					{#each logs as log, i (i)}
						<div class="flex items-start gap-2">
							<span class="shrink-0 text-slate-500">[{log.time}]</span>
							<span class="break-all">{log.message}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}
	</div>
</div>
