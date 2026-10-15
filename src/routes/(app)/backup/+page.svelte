<script lang="ts">
	import BackupPanel from '#lib/components/inventory/backup-panel.svelte';
	import { vaultStore } from '#lib/stores/vault.svelte';
	import { terminalStore } from '#lib/stores/terminal.svelte';
	import { inventoryStore } from '#lib/stores/inventory-data.svelte';
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
	<BackupPanel
		vaultStatus={vaultStore.status}
		hasLiveSessions={terminalStore.anyLiveSession}
		onrestored={() => inventoryStore.refresh()}
	/>
</div>
