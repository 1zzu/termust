<script lang="ts">
	import { Button } from '#lib/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle
	} from '#lib/components/ui/dialog';
	import { addedAt, forgetKnownHost, type KnownHost } from '#lib/inventory';
	import type { InventoryPageProps } from './page-props';
	import PageShell from './page-shell.svelte';

	/**
	 * Known hosts page: the host keys a connection has been approved for, and
	 * the one thing this page can do to them, forgetting one.
	 */
	// Forgetting a key is the only change this page makes, and the workspace
	// already reports whether it went through, so `onerror` stays unused here.
	let {
		knownHosts,
		busy,
		onmutate
	}: InventoryPageProps & {
		knownHosts: KnownHost[];
	} = $props();

	// Known-host dialog: forgetting is the only thing it can do.
	let forgetTarget = $state<KnownHost | null>(null);

	async function confirmForget() {
		const target = forgetTarget;
		if (!target) return;
		await onmutate(async () => {
			await forgetKnownHost(target.id);
			forgetTarget = null;
		});
	}
</script>

<PageShell title="Known hosts">
	{#if knownHosts.length === 0}
		<p class="text-xs text-muted-foreground">
			No host keys are trusted yet. Approving one while connecting adds it here.
		</p>
	{:else}
		<ul class="grid gap-2">
			{#each knownHosts as known (known.id)}
				<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
					<div class="min-w-0">
						<p class="truncate font-mono text-sm font-medium">{known.hostKeyId}</p>
						<p class="truncate text-xs text-muted-foreground">
							{known.keyAlgorithm} · added {addedAt(known.createdAt)}
						</p>
						<p class="truncate font-mono text-xs text-muted-foreground">
							{known.fingerprintSha256}
						</p>
					</div>
					<div class="shrink-0">
						<Button
							variant="outline"
							size="sm"
							onclick={() => (forgetTarget = known)}
							disabled={busy}
						>
							Forget
						</Button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</PageShell>

<Dialog open={forgetTarget !== null} onOpenChange={(open) => !open && (forgetTarget = null)}>
	<DialogContent class="sm:max-w-md">
		<DialogHeader>
			<DialogTitle>Forget this host key?</DialogTitle>
			<DialogDescription>
				The next connection to this host will ask you to check its fingerprint again. Nothing else
				is removed.
			</DialogDescription>
		</DialogHeader>

		{#if forgetTarget}
			<dl class="grid gap-2 text-sm">
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">Host</dt>
					<dd class="font-mono">{forgetTarget.hostKeyId}</dd>
				</div>
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">SHA-256 fingerprint</dt>
					<dd class="font-mono break-all">{forgetTarget.fingerprintSha256}</dd>
				</div>
			</dl>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={() => (forgetTarget = null)}>Cancel</Button>
			<Button variant="destructive" onclick={confirmForget} disabled={busy}>Forget</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>
