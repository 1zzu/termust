<script lang="ts">
	import KeyIcon from '@lucide/svelte/icons/key';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import ServerIcon from '@lucide/svelte/icons/server';
	import { Card } from '#lib/components/ui/card';
	import type { CredentialSummary, Host } from '#lib/inventory';

	let {
		credential,
		hosts = [],
		selected = false,
		busy = false,
		onselect
	}: {
		credential: CredentialSummary;
		hosts?: Host[];
		selected?: boolean;
		busy?: boolean;
		onselect: (credential: CredentialSummary) => void;
	} = $props();

	const isKey = $derived(credential.kind === 'private_key');
	const linkedHosts = $derived(hosts.filter((h) => h.credentialId === credential.id));

	const subtitle = $derived.by(() => {
		const typeLabel = isKey ? 'Private key' : 'Password';
		if (linkedHosts.length > 0) {
			return `${typeLabel} • ${linkedHosts.length} ${linkedHosts.length === 1 ? 'host' : 'hosts'}`;
		}
		return `${typeLabel} • Unused`;
	});

	function handleClick() {
		onselect(credential);
	}

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onselect(credential);
		}
	}
</script>

<li class="list-none">
	<Card
		role="button"
		tabindex={0}
		onclick={handleClick}
		onkeydown={handleKeyDown}
		aria-pressed={selected}
		aria-label="{credential.name}{selected ? ' (selected)' : ''}. Click to view details."
		title="Click to view details"
		class="cursor-pointer flex-row items-center gap-3 p-3 transition-all duration-150 outline-none {selected
			? 'bg-blue-500/10 shadow-xs ring-2 ring-blue-500 dark:bg-blue-500/10 dark:ring-blue-500'
			: 'ring-1 ring-border/70 hover:bg-muted/40 hover:ring-border'}"
	>
		<!-- Avatar bulat untuk jenis credential -->
		<div
			class="flex size-10 shrink-0 items-center justify-center rounded-full text-white shadow-xs {isKey
				? 'bg-indigo-600'
				: 'bg-emerald-600'}"
			aria-hidden="true"
		>
			{#if isKey}
				<KeyRoundIcon class="size-5" />
			{:else}
				<KeyIcon class="size-5" />
			{/if}
		</div>

		<!-- Info Credential: Judul & Subtitle -->
		<div class="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
			<div class="flex items-center justify-between gap-2">
				<span class="truncate text-sm font-semibold text-foreground">{credential.name}</span>
			</div>

			<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
				<span class="truncate text-[11px]">{subtitle}</span>
				{#if linkedHosts.length > 0}
					<span
						class="py-0.2 inline-flex items-center gap-1 rounded border border-border/70 bg-muted/50 px-1.5 text-[9px] font-medium text-foreground/80"
					>
						<ServerIcon class="size-2.5" />
						{linkedHosts.length}
					</span>
				{/if}
			</div>
		</div>
	</Card>
</li>
