<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	import * as Sidebar from '#lib/components/ui/sidebar';
	import AppTitlebar from '#lib/components/app-shell/app-titlebar.svelte';
	import AppSidebar from '#lib/components/app-shell/app-sidebar.svelte';
	import TerminalSessionPane from '#lib/components/terminal-session.svelte';
	import VaultGate from '#lib/components/vault/vault-gate.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { Button } from '#lib/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle
	} from '#lib/components/ui/dialog';
	import { hostKeyPromise } from '#lib/ssh';

	import { vaultStore } from '#lib/stores/vault.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';

	let { children } = $props();

	const isTerminalRoute = $derived(page.url.pathname === '/terminal');
	const activeWorkspace = $derived<'terminal' | 'inventory'>(
		isTerminalRoute ? 'terminal' : 'inventory'
	);

	onMount(() => {
		terminalStore.init();
		void vaultStore.checkStatus();
	});

	// Sync inventory data with vault unlock status
	$effect(() => {
		if (vaultStore.status === 'unlocked') {
			void inventoryStore.refresh();
		} else {
			inventoryStore.clear();
		}
	});

	// Guard locked routes: redirect back to /hosts if vault is locked
	const lockedRoutes = ['/credentials', '/groups', '/known-hosts', '/snippets', '/backup'];
	$effect(() => {
		if (
			vaultStore.status !== undefined &&
			vaultStore.status !== 'unlocked' &&
			lockedRoutes.some((route) => page.url.pathname.startsWith(route))
		) {
			void goto('/hosts');
		}
	});
</script>

<Sidebar.Provider class="flex h-screen w-screen overflow-hidden select-none">
	<!-- Sidebar navigation: full height on left with empty traffic lights header -->
	<AppSidebar />

	<!-- Main content Inset area -->
	<Sidebar.Inset class="relative flex min-w-0 flex-1 flex-col overflow-hidden select-text">
		<!-- Inset header: sidebar trigger, tabs, status, and disconnect button -->
		<AppTitlebar
			{activeWorkspace}
			sessions={terminalStore.sessions}
			activeSessionId={terminalStore.activeSessionId}
			canDisconnect={terminalStore.canDisconnect}
			onnavigateinventory={() => void goto('/hosts')}
			onselectsession={(sessionId) => {
				terminalStore.selectTab(sessionId);
				void goto('/terminal');
			}}
			onclosesession={(sessionId) => void terminalStore.closeTab(sessionId)}
			ondisconnect={() => void terminalStore.disconnectActive()}
		/>

		<!-- Backend events failure banner -->
		{#if terminalStore.eventsError}
			<div class="shrink-0 p-4 pb-0">
				<Alert variant="destructive">
					<AlertTitle>Session events unavailable</AlertTitle>
					<AlertDescription>{terminalStore.eventsError}</AlertDescription>
				</Alert>
			</div>
		{/if}

		<div class="relative min-h-0 flex-1 overflow-hidden">
			<!-- Persistent Terminal Workspace: kept mounted in DOM to preserve xterm instances & SSH connection -->
			<div
				class="relative h-full w-full flex-col overflow-hidden p-2"
				style:display={isTerminalRoute ? 'flex' : 'none'}
			>
				{#if terminalStore.activeSession?.errorMessage && terminalStore.activeSession?.status !== 'error'}
					<div class="mb-2 shrink-0">
						<Alert variant="destructive">
							<AlertTitle>Connection failed ({terminalStore.activeSession.title})</AlertTitle>
							<AlertDescription>{terminalStore.activeSession.errorMessage}</AlertDescription>
						</Alert>
					</div>
				{/if}

				<div class="relative min-h-0 flex-1">
					{#if terminalStore.sessions.length === 0}
						<div class="flex h-full items-center justify-center rounded-lg border border-dashed">
							<div class="flex flex-col items-center gap-2 p-4 text-center">
								<p class="text-xs text-muted-foreground">No active terminal sessions.</p>
								<Button
									variant="outline"
									size="sm"
									class="h-7 text-xs"
									onclick={() => void goto('/hosts')}
								>
									Go to Hosts
								</Button>
							</div>
						</div>
					{/if}
					{#each terminalStore.sessions as session (session.id)}
						<TerminalSessionPane
							{session}
							active={isTerminalRoute && session.id === terminalStore.activeSessionId}
							onready={(id, runtime) => terminalStore.handleReady(id, runtime)}
							onclose={(id) => {
								void terminalStore.closeTab(id);
								if (terminalStore.sessions.length <= 1) {
									void goto('/hosts');
								}
							}}
							onretry={(id) => void terminalStore.reconnectSession(id)}
						/>
					{/each}
				</div>
			</div>

			<!-- Routed Page Content: for inventory routes (/hosts, /credentials, etc.) -->
			<div
				class="h-full w-full overflow-y-auto p-4"
				style:display={isTerminalRoute ? 'none' : 'block'}
			>
				{@render children()}
			</div>
		</div>
	</Sidebar.Inset>
</Sidebar.Provider>

<!-- Host key prompt dialog -->
<Dialog open={terminalStore.activePrompt !== null}>
	<DialogContent
		showCloseButton={false}
		interactOutsideBehavior="ignore"
		escapeKeydownBehavior="ignore"
		class="sm:max-w-md"
	>
		<DialogHeader>
			<DialogTitle>Verify the host key</DialogTitle>
			<DialogDescription>
				This is the first connection to {terminalStore.activeSession?.title ?? 'this host'}. Compare
				the fingerprint with the server operator before continuing, or cancel to stop the
				connection.
				{#if terminalStore.activePrompt}
					{hostKeyPromise(terminalStore.activePrompt.persistent)}
				{/if}
			</DialogDescription>
		</DialogHeader>

		{#if terminalStore.activePrompt}
			<dl class="grid gap-2 text-sm">
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">Host</dt>
					<dd class="font-mono">
						{terminalStore.activePrompt.host}:{terminalStore.activePrompt.port}
					</dd>
				</div>
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">Algorithm</dt>
					<dd class="font-mono">{terminalStore.activePrompt.algorithm}</dd>
				</div>
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">SHA-256 fingerprint</dt>
					<dd class="font-mono break-all">{terminalStore.activePrompt.fingerprint}</dd>
				</div>
			</dl>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={() => void terminalStore.answerHostKey(false)}>
				Cancel
			</Button>
			<Button onclick={() => void terminalStore.answerHostKey(true)}>Accept and connect</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>

<!-- VaultGate modal overlay -->
<VaultGate
	status={vaultStore.status}
	open={vaultStore.showGate}
	checkError={vaultStore.error}
	onunlocked={(nextStatus) => vaultStore.setUnlocked(nextStatus)}
	oncontinue={() => {
		vaultStore.dismissGate();
		terminalStore.openQuickConnect = true;
		void goto('/hosts');
	}}
	onretry={() => void vaultStore.checkStatus()}
/>
