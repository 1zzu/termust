<script lang="ts">
	import ServerIcon from '@lucide/svelte/icons/server';

	import SessionTabStrip from '#lib/components/session-tab-strip.svelte';
	import { getVisualActiveSessionId } from '#lib/components/vault/vault-gate-state';
	import { Button } from '#lib/components/ui/button';
	import { statusLabel } from '#lib/ssh';
	import type { TerminalSession } from '#lib/session-tabs';

	/**
	 * Desktop app title bar.
	 *
	 * Combines macOS traffic-light safe margin, workspace navigation,
	 * integrated session tabs, draggable empty regions, and active status display.
	 */
	let {
		activeWorkspace = 'inventory',
		sessions = [],
		activeSessionId = null,
		canDisconnect = false,
		onnavigateinventory,
		onselectsession,
		onclosesession,
		ondisconnect
	}: {
		activeWorkspace?: 'inventory' | 'terminal';
		sessions: TerminalSession[];
		activeSessionId?: string | null;
		canDisconnect?: boolean;
		onnavigateinventory: () => void;
		onselectsession: (sessionId: string) => void;
		onclosesession: (sessionId: string) => void;
		ondisconnect?: () => void;
	} = $props();

	const activeSession = $derived(sessions.find((s) => s.id === activeSessionId));
	const visualActiveSessionId = $derived(
		getVisualActiveSessionId(activeWorkspace, activeSessionId)
	);
	const visualActiveSession = $derived(activeWorkspace === 'terminal' ? activeSession : undefined);

	function activeStatusDot(session?: TerminalSession): string {
		if (!session) return 'bg-status-idle';
		if (session.closing) return 'bg-status-idle';
		if (session.disconnecting) return 'bg-status-disconnecting';
		if (session.status === 'connected') return 'bg-status-connected';
		if (session.status === 'error') return 'bg-status-error';
		if (session.status === 'disconnected') return 'bg-status-idle';
		return 'bg-status-connecting';
	}
</script>

<header
	class="flex h-titlebar w-full shrink-0 items-center border-b border-border bg-background px-2 select-none"
>
	<!-- Safe area for macOS traffic lights (80px), non-interactive and draggable -->
	<div class="h-full w-20 shrink-0" data-tauri-drag-region aria-hidden="true"></div>

	<!-- Workspace navigation button (interactive, non-drag) -->
	<div class="flex shrink-0 items-center gap-1">
		<Button
			variant={activeWorkspace === 'inventory' ? 'secondary' : 'ghost'}
			size="sm"
			class="h-7 gap-1.5 px-2.5 text-xs font-medium"
			aria-current={activeWorkspace === 'inventory' ? 'page' : undefined}
			onclick={onnavigateinventory}
			title="Open Hosts Inventory"
		>
			<ServerIcon class="size-3.5" aria-hidden="true" />
			<span>Hosts</span>
		</Button>
	</div>

	<!-- Divider between workspace nav and tabs -->
	<div class="mx-1 h-4 w-px shrink-0 bg-border/60" aria-hidden="true"></div>

	<!-- Integrated session tabs (interactive, non-drag) -->
	<div class="flex max-w-2xl min-w-0 shrink items-center overflow-hidden">
		<SessionTabStrip
			{sessions}
			activeSessionId={visualActiveSessionId}
			onselect={onselectsession}
			onclose={onclosesession}
			class="border-0 bg-transparent p-0"
		/>
	</div>

	<!-- Middle draggable region for window dragging -->
	<div class="h-full min-w-4 flex-1" data-tauri-drag-region></div>

	<!-- Right area: active session summary and controls -->
	<div class="flex shrink-0 items-center gap-2 text-xs">
		{#if visualActiveSession}
			<div class="flex items-center gap-1.5 px-1 py-0.5 text-muted-foreground">
				<span class="size-2 rounded-full {activeStatusDot(visualActiveSession)}" aria-hidden="true"
				></span>
				<span
					class="max-w-32 truncate font-mono text-[11px] text-foreground"
					title={visualActiveSession.title}
				>
					{visualActiveSession.title}
				</span>
				<span class="text-[11px] text-muted-foreground"
					>({statusLabel(visualActiveSession.status)})</span
				>
			</div>
			{#if canDisconnect && ondisconnect}
				<Button
					variant="ghost"
					size="sm"
					class="h-6 px-2 text-[11px] text-muted-foreground hover:text-foreground"
					onclick={ondisconnect}
					title="Disconnect active connection"
				>
					Disconnect
				</Button>
			{/if}
		{:else}
			<div class="flex items-center gap-1.5 px-1 text-muted-foreground">
				<span class="size-2 rounded-full bg-status-idle" aria-hidden="true"></span>
				<span class="text-[11px]">{statusLabel('idle')}</span>
			</div>
		{/if}
	</div>

	<!-- Trailing draggable spacer -->
	<div class="h-full w-2 shrink-0" data-tauri-drag-region aria-hidden="true"></div>
</header>
