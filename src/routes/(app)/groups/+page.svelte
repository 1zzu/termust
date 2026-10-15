<script lang="ts">
	import GroupsPanel from '#lib/components/inventory/groups-panel.svelte';
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

	<GroupsPanel
		groups={inventoryStore.groups}
		busy={inventoryStore.busy}
		onmutate={(work) => inventoryStore.mutate(work)}
		onerror={(message) => inventoryStore.reportError(message)}
	/>
</div>
