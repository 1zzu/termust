<script lang="ts">
	import Disc3Icon from '@lucide/svelte/icons/disc-3';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import XIcon from '@lucide/svelte/icons/x';

	import { cn } from '#lib/utils';
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
		class: className,
		onselect,
		onclose
	}: {
		sessions: TerminalSession[];
		activeSessionId?: string | null;
		class?: string;
		onselect: (sessionId: string) => void;
		onclose: (sessionId: string) => void;
	} = $props();

	/** The colour that tells one session state from another at a glance. */
	function statusDot(session: TerminalSession): string {
		if (session.closing) return 'bg-status-idle';
		if (session.disconnecting) return 'bg-status-disconnecting';
		if (session.status === 'connected') return 'bg-status-connected';
		if (session.status === 'error') return 'bg-status-error';
		if (session.status === 'disconnected') return 'bg-status-idle';
		return 'bg-status-connecting';
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
		requestAnimationFrame(() => document.getElementById(tabElementId(target.id))?.focus());
	}

	function handleWheel(event: WheelEvent) {
		const target = event.currentTarget as HTMLElement;
		if (event.deltaY !== 0 && target.scrollWidth > target.clientWidth) {
			event.preventDefault();
			target.scrollLeft += event.deltaY;
		}
	}
</script>

<div
	role="tablist"
	aria-label="SSH sessions"
	onwheel={handleWheel}
	class={cn(
		'flex min-h-0 scrollbar-none items-center gap-1.5 overflow-x-auto p-0.5 select-none',
		className
	)}
>
	{#if sessions.length === 0}
		<p class="px-2 py-1 text-xs text-muted-foreground">No sessions yet. Connect to open a tab.</p>
	{:else}
		{#each sessions as session, index (session.id)}
			{@const selected = session.id === activeSessionId}
			<div
				class={cn(
					'group/tab flex h-7.5 shrink-0 items-center rounded-lg border px-2.5 text-xs transition-all select-none',
					selected
						? 'border-border/50 bg-sidebar-accent font-semibold text-sidebar-accent-foreground shadow-xs'
						: 'border-transparent bg-transparent text-sidebar-foreground/80 hover:border-border/30 hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground',
					session.closing && 'opacity-60'
				)}
			>
				{#if selected}
					<!-- Active tab: close X on the left -->
					<button
						type="button"
						class="mr-1.5 -ml-1 flex size-4 shrink-0 items-center justify-center rounded text-sidebar-accent-foreground/70 transition-colors hover:bg-sidebar-accent-foreground/15 hover:text-sidebar-accent-foreground"
						aria-label={`Close ${session.title}`}
						title={`Close ${session.title}`}
						disabled={session.closing}
						onclick={(e) => {
							e.stopPropagation();
							onclose(session.id);
						}}
					>
						<XIcon class="size-3 stroke-[2.5]" aria-hidden="true" />
					</button>
				{:else}
					<!-- Inactive tab: OS/Host icon, turns into close X on hover -->
					<button
						type="button"
						class="mr-1.5 -ml-1 flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-sidebar-accent-foreground"
						aria-label={`Close ${session.title}`}
						title={`Close ${session.title}`}
						disabled={session.closing}
						onclick={(e) => {
							e.stopPropagation();
							onclose(session.id);
						}}
					>
						<Disc3Icon
							class="size-3.5 text-muted-foreground/80 group-hover/tab:hidden"
							aria-hidden="true"
						/>
						<XIcon class="hidden size-3 stroke-2 group-hover/tab:block" aria-hidden="true" />
					</button>
				{/if}

				<!-- Title button for tab selection -->
				<button
					type="button"
					role="tab"
					id={tabElementId(session.id)}
					tabindex={selected ? 0 : -1}
					aria-selected={selected}
					aria-controls={panelElementId(session.id)}
					class="flex max-w-52 min-w-0 items-center gap-1.5 text-left outline-none"
					onclick={() => onselect(session.id)}
					onkeydown={(event) => moveTo(event, index)}
				>
					<span class="truncate">{session.title}</span>
					<span class="sr-only">{statusLabel(session.status)}</span>
				</button>

				<!-- {#if selected}
					<div class="ml-2 flex shrink-0 items-center text-sidebar-accent-foreground/70" aria-hidden="true">
						<SquareTerminalIcon class="size-3.5" />
					</div>
				{/if} -->
			</div>
		{/each}
	{/if}
</div>

<style>
	div[role='tablist'] {
		scrollbar-width: none;
		-ms-overflow-style: none;
	}
	div[role='tablist']::-webkit-scrollbar {
		display: none !important;
		width: 0 !important;
		height: 0 !important;
	}
</style>
