<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';

	import { Button } from '#lib/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle
	} from '#lib/components/ui/dialog';
	import { Input } from '#lib/components/ui/input';
	import { forgetKnownHost, type KnownHost } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import KnownHostGrid from './known-host-grid.svelte';
	import KnownHostSectionPanel from './known-host-section-panel.svelte';
	import ListDetail from './list-detail.svelte';
	import type { InventoryPageProps } from './page-props';

	let {
		knownHosts,
		busy = false,
		vaultStatus,
		onmutate,
		onerror,
		onrefresh,
		onopenvault
	}: InventoryPageProps & {
		knownHosts: KnownHost[];
		vaultStatus?: VaultStatus;
		onrefresh?: () => void | Promise<void>;
		onopenvault?: () => void;
	} = $props();

	let searchQuery = $state('');

	// Inspector panel state
	let selectedHostKeyId = $state<string | null>(null);

	// Forget confirmation dialog
	let forgetTarget = $state<KnownHost | null>(null);

	$effect(() => {
		// Close inspector if vault locks
		if (vaultStatus !== 'unlocked' && selectedHostKeyId !== null) {
			selectedHostKeyId = null;
		}
	});

	const filteredKnownHosts = $derived(
		knownHosts.filter((known) => {
			if (!searchQuery.trim()) return true;
			const q = searchQuery.toLowerCase().trim();
			return (
				known.hostKeyId.toLowerCase().includes(q) ||
				known.keyAlgorithm.toLowerCase().includes(q) ||
				known.fingerprintSha256.toLowerCase().includes(q)
			);
		})
	);

	// Prune selection if host key was forgotten
	$effect(() => {
		if (selectedHostKeyId !== null && !knownHosts.some((k) => k.id === selectedHostKeyId)) {
			selectedHostKeyId = null;
		}
	});

	const selectedKnownHost = $derived(
		selectedHostKeyId ? (knownHosts.find((k) => k.id === selectedHostKeyId) ?? null) : null
	);

	function handleSelect(known: KnownHost) {
		selectedHostKeyId = known.id;
	}

	function handleClosePanel() {
		selectedHostKeyId = null;
	}

	function promptForget(known: KnownHost) {
		forgetTarget = known;
	}

	async function confirmForget() {
		const target = forgetTarget;
		if (!target) return;
		await onmutate(async () => {
			await forgetKnownHost(target.id);
			if (selectedHostKeyId === target.id) {
				selectedHostKeyId = null;
			}
			forgetTarget = null;
		});
	}
</script>

<ListDetail open={selectedHostKeyId !== null}>
	{#snippet header()}
		<!-- Known Hosts Toolbar: search on top -->
		<div class="flex flex-col gap-3">
			<div class="relative w-full">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					placeholder="Find trusted host, algorithm, or fingerprint..."
					bind:value={searchQuery}
					class="h-9 w-full rounded-lg border-border/70 bg-card/60 pr-4 pl-9 text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-primary/40"
					aria-label="Find trusted host key"
					disabled={busy}
				/>
			</div>
		</div>
	{/snippet}

	<div class="flex flex-col gap-2">
		<div class="flex items-center justify-between px-0.5">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Known Hosts</h3>
			{#if filteredKnownHosts.length > 0}
				<span class="text-[11px] text-muted-foreground">
					{filteredKnownHosts.length}
					{filteredKnownHosts.length === 1 ? 'host key' : 'host keys'}
				</span>
			{/if}
		</div>

		<KnownHostGrid
			knownHosts={filteredKnownHosts}
			totalCount={knownHosts.length}
			selectedHostId={selectedHostKeyId}
			{busy}
			{vaultStatus}
			onselect={handleSelect}
			{onopenvault}
		/>
	</div>

	{#snippet panel()}
		{#if selectedKnownHost}
			<KnownHostSectionPanel
				knownHost={selectedKnownHost}
				{busy}
				{vaultStatus}
				onforget={promptForget}
				onclose={handleClosePanel}
			/>
		{/if}
	{/snippet}
</ListDetail>

<!-- Confirmation Dialog when forgetting host key -->
<Dialog open={forgetTarget !== null} onOpenChange={(open) => !open && (forgetTarget = null)}>
	<DialogContent class="sm:max-w-md">
		<DialogHeader>
			<DialogTitle>Forget this host key?</DialogTitle>
			<DialogDescription>
				The next connection to this host will ask you to verify its fingerprint again. Nothing else
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
					<dt class="text-xs text-muted-foreground">Algorithm</dt>
					<dd class="font-mono">{forgetTarget.keyAlgorithm}</dd>
				</div>
				<div class="grid gap-0.5">
					<dt class="text-xs text-muted-foreground">SHA-256 fingerprint</dt>
					<dd class="font-mono break-all">{forgetTarget.fingerprintSha256}</dd>
				</div>
			</dl>
		{/if}

		<DialogFooter>
			<Button variant="outline" onclick={() => (forgetTarget = null)} disabled={busy}>Cancel</Button
			>
			<Button variant="destructive" onclick={confirmForget} disabled={busy}>Forget</Button>
		</DialogFooter>
	</DialogContent>
</Dialog>
