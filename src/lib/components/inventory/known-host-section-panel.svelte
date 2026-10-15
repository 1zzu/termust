<script lang="ts">
	import ArrowRightToLineIcon from '@lucide/svelte/icons/arrow-right-to-line';
	import CheckIcon from '@lucide/svelte/icons/check';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';
	import ShieldCheckIcon from '@lucide/svelte/icons/shield-check';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';

	import { Button } from '#lib/components/ui/button';
	import { addedAt, type KnownHost } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';

	let {
		knownHost,
		busy = false,
		vaultStatus,
		onforget,
		onclose
	}: {
		knownHost: KnownHost;
		busy?: boolean;
		vaultStatus?: VaultStatus;
		onforget: (knownHost: KnownHost) => void;
		onclose: () => void;
	} = $props();

	let showMoreMenu = $state(false);
	let copied = $state(false);

	async function copyFingerprint() {
		try {
			await navigator.clipboard.writeText(knownHost.fingerprintSha256);
			copied = true;
			setTimeout(() => {
				copied = false;
			}, 2000);
		} catch {
			// ignore clipboard write failure
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			if (document.querySelector('[data-slot="dialog-content"]')) return;
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<aside
	class="flex w-full shrink-0 flex-col gap-4 border-l border-border bg-sidebar p-4 sm:p-5 lg:min-h-0 lg:w-96 lg:self-stretch xl:w-104"
	aria-label="Host Key Details"
>
	<!-- Header: Title, Vault badge, Options and Close button -->
	<header class="flex items-start justify-between gap-2 pt-0.5">
		<div class="flex flex-col gap-0.5">
			<h3 class="text-sm font-semibold text-foreground">Host Key Details</h3>
			<div class="flex items-center gap-1 text-[11px] text-muted-foreground">
				<span>{vaultStatus === 'unlocked' ? 'Personal vault' : 'Local vault'}</span>
				<ChevronDownIcon class="size-3 opacity-60" />
			</div>
		</div>

		<div class="flex items-center gap-0.5">
			<div class="relative">
				<Button
					variant="ghost"
					size="icon"
					class="size-7 text-muted-foreground hover:text-foreground"
					onclick={() => (showMoreMenu = !showMoreMenu)}
					title="More options"
					aria-label="More options"
				>
					<MoreHorizontalIcon class="size-4" />
				</Button>
				{#if showMoreMenu}
					<div
						class="fixed inset-0 z-40"
						onclick={() => (showMoreMenu = false)}
						aria-hidden="true"
					></div>
					<div
						class="absolute top-full right-0 z-50 mt-1 min-w-36 animate-in rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md fade-in-0 zoom-in-95"
					>
						<button
							type="button"
							class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-destructive hover:bg-destructive/10"
							onclick={() => {
								showMoreMenu = false;
								onforget(knownHost);
							}}
						>
							<Trash2Icon class="size-3.5" />
							<span>Forget host key</span>
						</button>
					</div>
				{/if}
			</div>

			<Button
				variant="ghost"
				size="icon"
				class="size-7 text-muted-foreground hover:text-foreground"
				onclick={onclose}
				aria-label="Close details"
				title="Close panel"
			>
				<ArrowRightToLineIcon class="size-3.5" />
			</Button>
		</div>
	</header>

	<!-- Scrollable Details Body -->
	<div class="flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5 sm:gap-5">
		<!-- 1. Host Identification -->
		<div class="flex flex-col gap-2">
			<span class="text-xs font-medium text-muted-foreground">Host Identity</span>

			<div class="rounded-md border border-border/60 bg-muted/30 p-3">
				<div class="font-mono text-xs font-semibold text-foreground">
					{knownHost.hostKeyId}
				</div>
				<div class="mt-1 flex items-center gap-2 text-[11px] text-muted-foreground">
					<span
						>Algorithm: <strong class="font-mono text-foreground">{knownHost.keyAlgorithm}</strong
						></span
					>
				</div>
				<div class="mt-1 text-[11px] text-muted-foreground">
					Added: {addedAt(knownHost.createdAt)}
				</div>
			</div>
		</div>

		<!-- 2. Fingerprint Section -->
		<div class="flex flex-col gap-1.5">
			<div class="flex items-center justify-between">
				<span class="text-xs font-medium text-muted-foreground">SHA-256 Fingerprint</span>
				<Button
					variant="ghost"
					size="sm"
					class="h-6 gap-1 px-2 text-[11px] text-muted-foreground hover:text-foreground"
					onclick={copyFingerprint}
				>
					{#if copied}
						<CheckIcon class="size-3 text-emerald-500" />
						<span class="text-emerald-500">Copied</span>
					{:else}
						<CopyIcon class="size-3" />
						<span>Copy</span>
					{/if}
				</Button>
			</div>

			<pre
				class="rounded-md border border-border/60 bg-muted/30 p-2.5 font-mono text-[11px] break-all whitespace-pre-wrap text-foreground select-all">{knownHost.fingerprintSha256}</pre>
		</div>

		<!-- 3. Security Verification Note -->
		<div class="flex flex-col gap-2 border-t border-border/50 pt-3">
			<div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
				<ShieldCheckIcon class="size-3.5 text-cyan-500" />
				<span>SSH Host Key Verification</span>
			</div>
			<p class="text-[11px] leading-relaxed text-muted-foreground">
				This fingerprint is saved in your vault. When connecting to this host, Termust compares the
				server's host key with this fingerprint to prevent man-in-the-middle attacks.
			</p>
			<p class="text-[11px] leading-relaxed text-muted-foreground">
				Forgetting this key will cause Termust to ask you to verify and approve the fingerprint
				again on your next connection.
			</p>
		</div>
	</div>

	<!-- Footer: Forget button -->
	<footer>
		<Button
			variant="outline"
			class="h-10 w-full border-destructive/40 text-xs font-semibold text-destructive hover:bg-destructive/10 hover:text-destructive"
			onclick={() => onforget(knownHost)}
			disabled={busy}
		>
			<Trash2Icon class="mr-1.5 size-3.5" />
			<span>Forget host key</span>
		</Button>
	</footer>
</aside>
