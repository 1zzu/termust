<script lang="ts">
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';

	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import { createGroup, deleteGroup, renameGroup, type Group, type Host } from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
	import GroupGrid from './group-grid.svelte';
	import GroupSectionPanel from './group-section-panel.svelte';
	import ListDetail from './list-detail.svelte';
	import type { InventoryPageProps } from './page-props';

	let {
		groups,
		hosts = [],
		busy = false,
		vaultStatus,
		onmutate,
		onerror,
		onrefresh,
		onopenvault
	}: InventoryPageProps & {
		groups: Group[];
		hosts?: Host[];
		vaultStatus?: VaultStatus;
		onrefresh?: () => void | Promise<void>;
		onopenvault?: () => void;
	} = $props();

	let searchQuery = $state('');

	// Group inspector panel state
	let formOpen = $state(false);
	let editingGroupId = $state<string | null>(null);
	let groupNameInput = $state('');

	$effect(() => {
		// Close inspector if vault locks
		if (vaultStatus !== 'unlocked' && formOpen) {
			formOpen = false;
			editingGroupId = null;
			groupNameInput = '';
		}
	});

	const filteredGroups = $derived(
		groups.filter((group) => {
			if (!searchQuery.trim()) return true;
			return group.name.toLowerCase().includes(searchQuery.toLowerCase().trim());
		})
	);

	// Prune selection if group was removed
	$effect(() => {
		if (editingGroupId !== null && !groups.some((g) => g.id === editingGroupId)) {
			closeGroupForm();
		}
	});

	const selectedGroup = $derived(
		editingGroupId ? (groups.find((g) => g.id === editingGroupId) ?? null) : null
	);

	function handleSelectGroup(group: Group) {
		openEditGroup(group);
	}

	function openCreateGroup() {
		editingGroupId = null;
		groupNameInput = '';
		formOpen = true;
	}

	function openEditGroup(group: Group) {
		editingGroupId = group.id;
		groupNameInput = group.name;
		formOpen = true;
	}

	function closeGroupForm() {
		formOpen = false;
		editingGroupId = null;
		groupNameInput = '';
	}

	async function submitGroup(event?: SubmitEvent) {
		event?.preventDefault();

		if (groupNameInput.trim() === '') {
			onerror('Enter a group name.');
			return;
		}

		const editing = editingGroupId;
		const name = groupNameInput.trim();

		await onmutate(async () => {
			if (editing === null) {
				await createGroup(name);
			} else {
				await renameGroup(editing, name);
			}
			closeGroupForm();
		});
	}

	async function handleDeleteSelected() {
		const target = selectedGroup;
		if (!target) return;
		closeGroupForm();
		await onmutate(() => deleteGroup(target.id));
	}
</script>

<ListDetail open={formOpen}>
	{#snippet header()}
		<!-- Groups Toolbar: search on top, actions row below -->
		<div class="flex flex-col gap-3">
			<!-- Top Search Bar -->
			<div class="relative w-full">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					placeholder="Find a group by name..."
					bind:value={searchQuery}
					class="h-9 w-full rounded-lg border-border/70 bg-card/60 pr-4 pl-9 text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-primary/40"
					aria-label="Find a group by name"
					disabled={busy}
				/>
			</div>

			<!-- Secondary Action Bar (Single line) -->
			<div class="flex items-center justify-between gap-2">
				<div class="flex items-center gap-2">
					<Button
						size="sm"
						class="h-8 shrink-0 gap-1.5 text-xs font-medium"
						onclick={openCreateGroup}
						disabled={busy || vaultStatus !== 'unlocked'}
					>
						<PlusIcon class="size-3.5" />
						<span>New group</span>
					</Button>
				</div>
			</div>
		</div>
	{/snippet}

	<div class="flex flex-col gap-2">
		<div class="flex items-center justify-between px-0.5">
			<h3 class="text-sm font-semibold tracking-tight text-foreground">Groups</h3>
			{#if filteredGroups.length > 0}
				<span class="text-[11px] text-muted-foreground">
					{filteredGroups.length}
					{filteredGroups.length === 1 ? 'group' : 'groups'}
				</span>
			{/if}
		</div>

		<GroupGrid
			groups={filteredGroups}
			totalCount={groups.length}
			{hosts}
			selectedGroupId={editingGroupId}
			{busy}
			{vaultStatus}
			onselect={handleSelectGroup}
			{onopenvault}
		/>
	</div>

	{#snippet panel()}
		{#if formOpen}
			<GroupSectionPanel
				mode={editingGroupId === null ? 'create' : 'edit'}
				group={selectedGroup}
				bind:name={groupNameInput}
				{hosts}
				{busy}
				{vaultStatus}
				onsubmit={submitGroup}
				ondelete={handleDeleteSelected}
				onclose={closeGroupForm}
			/>
		{/if}
	{/snippet}
</ListDetail>
