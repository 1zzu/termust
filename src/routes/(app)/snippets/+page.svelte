<script lang="ts">
	import SnippetsPanel from '#lib/components/inventory/snippets-panel.svelte';
	import { Alert, AlertDescription, AlertTitle } from '#lib/components/ui/alert';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
	{#if inventoryStore.errorText}
		<Alert variant="destructive" class="shrink-0">
			<AlertTitle>Inventory error</AlertTitle>
			<AlertDescription>{inventoryStore.errorText}</AlertDescription>
		</Alert>
	{/if}

	<SnippetsPanel
		snippets={inventoryStore.snippets}
		busy={inventoryStore.busy}
		sessionStatus={terminalStore.activeSession?.status ?? 'idle'}
		sessionId={terminalStore.activeSession?.id}
		sessionTarget={terminalStore.activeSession?.title ?? ''}
		onmutate={(work) => inventoryStore.mutate(work)}
		onerror={(message) => inventoryStore.reportError(message)}
	/>
</div>
