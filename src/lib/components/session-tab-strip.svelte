<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';

	import { Button } from '#lib/components/ui/button';
	import { statusLabel } from '#lib/ssh';
	import {
		panelElementId,
		tabElementId,
		tabKeyboardTarget,
		type TerminalSession
	} from '#lib/session-tabs';

	/**
	 * The tab strip above the terminal: one tab per session.
	 *
	 * It only selects and closes sessions; it holds no session state of its own,
	 * and it never renders anything secret. The page decides what a click means.
	 */
	let {
		sessions,
		activeSessionId = null,
		onselect,
		onclose
	}: {
		sessions: TerminalSession[];
		activeSessionId?: string | null;
		onselect: (sessionId: string) => void;
		onclose: (sessionId: string) => void;
	} = $props();

	/** The colour that tells one session state from another at a glance. */
	function statusDot(session: TerminalSession): string {
		if (session.closing) return 'bg-muted-foreground';
		// Still connected, but on its way out: it counts as live until the backend
		// has really stopped it, so it is not shown as settled yet.
		if (session.disconnecting) return 'bg-amber-500';
		if (session.status === 'connected') return 'bg-emerald-500';
		if (session.status === 'error') return 'bg-rose-500';
		if (session.status === 'disconnected') return 'bg-muted-foreground';
		// Connecting, checking the host key, authenticating.
		return 'bg-amber-500';
	}

	/**
	 * Arrow keys, Home and End move between tabs. The prompt is not automatic,
	 * so a keyboard that is not on the strip is never moved by this.
	 */
	function moveTo(event: KeyboardEvent, index: number) {
		const next = tabKeyboardTarget(event.key, index, sessions.length);
		if (next === null) return;
		const target = sessions[next];
		if (!target) return;

		event.preventDefault();
		onselect(target.id);
		// Coming forward focuses the terminal. A keyboard user is still on the
		// strip, so the tab keeps focus once that has happened.
		requestAnimationFrame(() => document.getElementById(tabElementId(target.id))?.focus());
	}
</script>

<div
	role="tablist"
	aria-label="SSH sessions"
	class="flex min-h-0 items-center gap-1 overflow-x-auto rounded-lg border p-1"
>
	{#if sessions.length === 0}
		<p class="px-2 py-1 text-xs text-muted-foreground">No sessions yet. Connect to open a tab.</p>
	{:else}
		{#each sessions as session, index (session.id)}
			{@const selected = session.id === activeSessionId}
			<div
				class="flex shrink-0 items-center rounded-md border border-transparent"
				class:border-border={selected}
				class:bg-muted={selected}
				class:opacity-60={session.closing}
			>
				<button
					type="button"
					role="tab"
					id={tabElementId(session.id)}
					tabindex={selected ? 0 : -1}
					aria-selected={selected}
					aria-controls={panelElementId(session.id)}
					class="flex max-w-52 items-center gap-2 px-2 py-1 text-xs whitespace-nowrap"
					onclick={() => onselect(session.id)}
					onkeydown={(event) => moveTo(event, index)}
				>
					<span class="size-2 shrink-0 rounded-full {statusDot(session)}" aria-hidden="true"></span>
					<span class="truncate">{session.title}</span>
					<span class="sr-only">{statusLabel(session.status)}</span>
				</button>
				<Button
					variant="ghost"
					size="icon-xs"
					class="mr-0.5 text-muted-foreground"
					aria-label={`Close ${session.title}`}
					title={`Close ${session.title}`}
					disabled={session.closing}
					onclick={() => onclose(session.id)}
				>
					<XIcon />
				</Button>
			</div>
		{/each}
	{/if}
</div>
