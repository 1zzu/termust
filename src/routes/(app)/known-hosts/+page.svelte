<script lang="ts">
	import KnownHostsWorkspace from '#lib/components/inventory/known-hosts-workspace.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
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
		<KnownHostsWorkspace
			knownHosts={inventoryStore.knownHosts}
			busy={inventoryStore.busy}
			vaultStatus={vaultStore.status}
			onmutate={(work) => inventoryStore.mutate(work)}
			onerror={(message) => inventoryStore.reportError(message)}
			onrefresh={() => inventoryStore.refresh()}
			onopenvault={() => vaultStore.openGate()}
		/>
	</div>
</div>
