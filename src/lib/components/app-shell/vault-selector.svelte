<script lang="ts">
	import VaultIcon from '@lucide/svelte/icons/vault';
	import CloudOffIcon from '@lucide/svelte/icons/cloud-off';
	import UsersIcon from '@lucide/svelte/icons/users';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import CheckIcon from '@lucide/svelte/icons/check';

	import * as DropdownMenu from '#lib/components/ui/dropdown-menu';

	interface VaultOption {
		id: string;
		name: string;
		type: 'personal' | 'team';
		isLocal: boolean;
		disabled?: boolean;
	}

	const vaults: VaultOption[] = [
		{
			id: 'personal',
			name: 'Personal',
			type: 'personal',
			isLocal: true
		},
		{
			id: 'team',
			name: 'Team',
			type: 'team',
			isLocal: false,
			disabled: true
		}
	];

	let activeVaultId = $state<string>('personal');
	let isOpen = $state(false);

	const activeVault = $derived(vaults.find((v) => v.id === activeVaultId) ?? vaults[0]);
</script>

<DropdownMenu.Root bind:open={isOpen}>
	<DropdownMenu.Trigger
		class="group/vault inline-flex h-7 items-center rounded-md border border-border/80 bg-secondary/70 px-2 py-0.5 text-xs font-medium text-secondary-foreground shadow-xs transition-colors outline-none select-none hover:bg-secondary focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 data-[state=open]:bg-secondary"
		aria-label="Pilih vault"
	>
		<div class="flex min-w-0 items-center gap-1.5">
			<VaultIcon class="size-3.5 shrink-0 text-foreground/90" aria-hidden="true" />
			<span class="truncate text-[11.5px] font-medium">Vaults</span>
			{#if activeVault.isLocal}
				<span title="Local only / offline" class="flex shrink-0 items-center">
					<CloudOffIcon class="size-3 shrink-0 text-muted-foreground/80" aria-hidden="true" />
				</span>
			{/if}
		</div>
		<div class="mx-1.5 h-3 w-px shrink-0 bg-border" aria-hidden="true"></div>
		<ChevronDownIcon
			class="size-3 shrink-0 text-muted-foreground transition-transform duration-200 group-data-[state=open]/vault:rotate-180"
			aria-hidden="true"
		/>
	</DropdownMenu.Trigger>

	<DropdownMenu.Content align="start" sideOffset={6} class="w-48 p-1">
		{#each vaults as vault (vault.id)}
			{#if vault.disabled}
				<DropdownMenu.Item
					class="flex cursor-not-allowed items-center justify-between gap-2 px-2 py-1.5 text-xs opacity-60"
					disabled
				>
					<div class="flex min-w-0 items-center gap-2">
						<VaultIcon class="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
						<span class="truncate font-medium">{vault.name}</span>
					</div>
					<div class="flex shrink-0 items-center gap-1.5 text-muted-foreground">
						<UsersIcon class="size-3" aria-hidden="true" />
						<span
							class="py-0.2 rounded bg-muted px-1 text-[9px] font-medium tracking-wide uppercase"
							>Soon</span
						>
					</div>
				</DropdownMenu.Item>
			{:else}
				<DropdownMenu.Item
					class="flex cursor-pointer items-center justify-between gap-2 px-2 py-1.5 text-xs hover:bg-accent hover:text-accent-foreground"
					onclick={() => {
						activeVaultId = vault.id;
					}}
				>
					<div class="flex min-w-0 items-center gap-2">
						<VaultIcon class="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
						<span class="truncate font-medium">{vault.name}</span>
					</div>
					<div class="flex shrink-0 items-center gap-1.5 text-muted-foreground">
						{#if vault.isLocal}
							<span title="Offline / Local only" class="flex items-center">
								<CloudOffIcon class="size-3" aria-hidden="true" />
							</span>
						{/if}
						{#if activeVaultId === vault.id}
							<CheckIcon class="size-3.5 text-foreground" aria-hidden="true" />
						{/if}
					</div>
				</DropdownMenu.Item>
			{/if}
		{/each}
	</DropdownMenu.Content>
</DropdownMenu.Root>
