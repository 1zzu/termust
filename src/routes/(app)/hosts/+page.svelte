<script lang="ts">
	import HostWorkspace from '#lib/components/inventory/host-workspace.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
	import { vaultStore } from '#lib/stores/vault.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
	import { goto } from '$app/navigation';
</script>

<div class="flex h-[calc(100%+2rem)] min-h-0 flex-col -m-4">
	{#if inventoryStore.errorText}
		<div class="p-4 pb-0 shrink-0">
			<Alert variant="destructive">
				<AlertTitle>Inventory error</AlertTitle>
				<AlertDescription>{inventoryStore.errorText}</AlertDescription>
			</Alert>
		</div>
	{/if}

	<div class="flex-1 min-h-0 h-full">
		<HostWorkspace
		hosts={inventoryStore.hosts}
		groups={inventoryStore.groups}
		credentials={inventoryStore.credentials}
		busy={inventoryStore.busy}
		vaultStatus={vaultStore.status}
		hostConnecting={(hostId) => terminalStore.isHostConnecting(hostId)}
		onconnect={(host) => {
			terminalStore.connectSavedHost(host);
			void goto('/terminal');
		}}
		onmutate={(work) => inventoryStore.mutate(work)}
		onerror={(message) => inventoryStore.reportError(message)}
		onrefresh={() => inventoryStore.refresh()}
		bind:quickHost={terminalStore.quickHost}
		bind:quickPort={terminalStore.quickPort}
		bind:quickUsername={terminalStore.quickUsername}
		bind:quickAuth={terminalStore.quickAuth}
		bind:quickPassword={terminalStore.quickPassword}
		bind:quickPrivateKey={terminalStore.quickPrivateKey}
		bind:quickPassphrase={terminalStore.quickPassphrase}
		quickSubmitting={terminalStore.quickSubmitting}
		onquicksubmit={(event) => {
			terminalStore.submitQuickConnect(event);
			void goto('/terminal');
		}}
		onquickclear={() => terminalStore.clearCredentials()}
		bind:openQuickConnect={terminalStore.openQuickConnect}
		onopenvault={() => vaultStore.openGate()}
	/>
	</div>
</div>
