<script lang="ts">
	import KnownHostsPanel from '#lib/components/inventory/known-hosts-panel.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
	{#if inventoryStore.errorText}
		<Alert variant="destructive" class="shrink-0">
			<AlertTitle>Inventory error</AlertTitle>
			<AlertDescription>{inventoryStore.errorText}</AlertDescription>
		</Alert>
	{/if}

	<KnownHostsPanel
		knownHosts={inventoryStore.knownHosts}
		busy={inventoryStore.busy}
		onmutate={(work) => inventoryStore.mutate(work)}
		onerror={(message) => inventoryStore.reportError(message)}
	/>
</div>
