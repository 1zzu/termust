<script lang="ts">
	import HostWorkspace from '#lib/components/inventory/host-workspace.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
	import { vaultStore } from '#lib/stores/vault.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
	import { goto } from '$app/navigation';
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
	{#if inventoryStore.errorText}
		<Alert variant="destructive" class="shrink-0">
			<AlertTitle>Inventory error</AlertTitle>
			<AlertDescription>{inventoryStore.errorText}</AlertDescription>
		</Alert>
	{/if}

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
