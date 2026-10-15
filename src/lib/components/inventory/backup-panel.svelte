<script lang="ts">
	import BackupRestore from '#lib/components/backup-restore.svelte';
	import type { VaultStatus } from '#lib/vault';
	import PageShell from './page-shell.svelte';

	/**
	 * Backup & Restore page: the whole vault as one encrypted file, and the
	 * restore that replaces its records with the file's.
	 */
	let {
		vaultStatus,
		hasLiveSessions = false,
		onrestored
	}: {
		vaultStatus: VaultStatus | undefined;
		/** Whether any tab still has a session that is connected or connecting. */
		hasLiveSessions?: boolean;
		/** Called after a successful restore, so every list is re-read. */
		onrestored?: () => void | Promise<void>;
	} = $props();

	/** What the file holds and what a restore does, in the words the page uses. */
	const description =
		'A backup is one encrypted file, locked with a password you choose for it. It holds every group, host, credential, known host and snippet in this vault, and it can be restored into this vault or into another one with a different master password. It is not a sync: restoring replaces every record in this vault with the ones in the file.';
</script>

<PageShell title="Backup & Restore" {description}>
	<BackupRestore {vaultStatus} {hasLiveSessions} {onrestored} />
</PageShell>
