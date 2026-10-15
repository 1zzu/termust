<script lang="ts">
	import BackupWorkspace from '#lib/components/inventory/backup-workspace.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
	import { vaultStore } from '#lib/stores/vault.svelte';
</script>

<div class="-m-4 flex h-[calc(100%+2rem)] min-h-0 flex-col">
	{#if inventoryStore.errorText}
		<div class="shrink-0 p-4 pb-0">
			<Alert variant="destructive">
				<AlertTitle>Inventory error</AlertTitle>
				<AlertDescription>{inventoryStore.errorText}</AlertDescription>
			</Alert>
		</div>
	{/if}

	<div class="h-full min-h-0 flex-1">
		<BackupWorkspace
			vaultStatus={vaultStore.status}
			hasLiveSessions={terminalStore.anyLiveSession}
			hosts={inventoryStore.hosts}
			credentials={inventoryStore.credentials}
			groups={inventoryStore.groups}
			snippets={inventoryStore.snippets}
			knownHosts={inventoryStore.knownHosts}
			busy={inventoryStore.busy}
			onrestored={() => inventoryStore.refresh()}
			onopenvault={() => vaultStore.openGate()}
		/>
	</div>
</div>
