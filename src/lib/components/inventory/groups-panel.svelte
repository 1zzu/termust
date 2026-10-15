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
	import { Input } from '#lib/components/ui/input';
	import { Label } from '#lib/components/ui/label';
	import { createGroup, deleteGroup, renameGroup, type Group } from '#lib/inventory';
	import type { InventoryPageProps } from './page-props';
	import PageShell from './page-shell.svelte';

	/**
	 * Groups page: the labels hosts are kept together under, and the create,
	 * rename and delete flows behind their dialog.
	 */
	let {
		groups,
		busy,
		onmutate,
		onerror
	}: InventoryPageProps & {
		groups: Group[];
	} = $props();

	// Group dialog. `renamingGroup` is null while creating a new one.
	let groupOpen = $state(false);
	let renamingGroup = $state<string | null>(null);
	let groupNameInput = $state('');

	function openCreateGroup() {
		renamingGroup = null;
		groupNameInput = '';
		groupOpen = true;
	}

	function openRenameGroup(group: Group) {
		renamingGroup = group.id;
		groupNameInput = group.name;
		groupOpen = true;
	}

	async function submitGroup(event: SubmitEvent) {
		event.preventDefault();
		if (groupNameInput.trim() === '') {
			onerror('Enter a group name.');
			return;
		}

		const renaming = renamingGroup;
		await onmutate(async () => {
			if (renaming === null) {
				await createGroup(groupNameInput.trim());
			} else {
				await renameGroup(renaming, groupNameInput.trim());
			}
			groupOpen = false;
			groupNameInput = '';
		});
	}

	async function removeGroup(group: Group) {
		await onmutate(() => deleteGroup(group.id));
	}
</script>

<PageShell title="Groups">
	{#snippet actions()}
		<Button size="sm" onclick={openCreateGroup} disabled={busy}>New group</Button>
	{/snippet}
	{#if groups.length === 0}
		<p class="text-xs text-muted-foreground">No groups yet.</p>
	{:else}
		<ul class="grid gap-2">
			{#each groups as group (group.id)}
				<li class="flex items-center justify-between gap-3 rounded-lg border p-2">
					<p class="truncate text-sm font-medium">{group.name}</p>
					<div class="flex shrink-0 gap-2">
						<Button
							variant="outline"
							size="sm"
							onclick={() => openRenameGroup(group)}
							disabled={busy}
						>
							Rename
						</Button>
						<Button variant="outline" size="sm" onclick={() => removeGroup(group)} disabled={busy}>
							Delete
						</Button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</PageShell>

<Dialog bind:open={groupOpen}>
	<DialogContent class="sm:max-w-md">
		<DialogHeader>
			<DialogTitle>{renamingGroup === null ? 'New group' : 'Rename group'}</DialogTitle>
			<DialogDescription>Groups keep related hosts together.</DialogDescription>
		</DialogHeader>

		<form class="grid gap-3" onsubmit={submitGroup}>
			<div class="grid gap-1.5">
				<Label for="group-name">Name</Label>
				<Input id="group-name" bind:value={groupNameInput} disabled={busy} />
			</div>
			<DialogFooter>
				<Button type="button" variant="outline" onclick={() => (groupOpen = false)}>Cancel</Button>
				<Button type="submit" disabled={busy}>
					{renamingGroup === null ? 'Create group' : 'Save name'}
				</Button>
			</DialogFooter>
		</form>
	</DialogContent>
</Dialog>
