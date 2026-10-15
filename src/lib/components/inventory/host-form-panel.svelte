<script lang="ts">
	import {
		NO_RELATION,
		type CredentialSummary,
		type Group,
		type HostFormValues
	} from '#lib/inventory';
	import { Button } from '#lib/components/ui/button';
	import { Input } from '#lib/components/ui/input';
	import { Label } from '#lib/components/ui/label';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue
	} from '#lib/components/ui/select';
	import SidePanel from './side-panel.svelte';

	/**
	 * The host form, in the right column: create a new host or edit the
	 * selected one. It only edits the values it is handed; the workspace
	 * validates and saves them.
	 */
	let {
		mode,
		form = $bindable(),
		groups,
		credentials,
		busy = false,
		onsubmit,
		onclose
	}: {
		mode: 'create' | 'edit';
		form: HostFormValues;
		groups: Group[];
		credentials: CredentialSummary[];
		busy?: boolean;
		onsubmit: (event: SubmitEvent) => void;
		onclose: () => void;
	} = $props();

	// bits-ui reads a closed trigger's text from these, so a select shows a name
	// instead of the raw id it actually stores.
	const groupOptions = $derived([
		{ value: NO_RELATION, label: 'None' },
		...groups.map((group) => ({ value: group.id, label: group.name }))
	]);
	const credentialOptions = $derived([
		{ value: NO_RELATION, label: 'None' },
		...credentials.map((credential) => ({ value: credential.id, label: credential.name }))
	]);
</script>

<SidePanel
	label="Host form"
	kicker={mode === 'create' ? 'New Host' : 'Edit Host'}
	title={form.label.trim() || 'Untitled host'}
	{onclose}
>
	<p class="text-xs text-muted-foreground">
		The Credential chosen here is the one its Connect button uses. Secrets stay encrypted and are
		never shown again.
	</p>

	<form id="host-form" class="grid gap-3" {onsubmit}>
		<div class="grid gap-1.5">
			<Label for="host-label" class="text-xs">Label</Label>
			<Input
				id="host-label"
				bind:value={form.label}
				disabled={busy}
				class="h-8 text-xs"
				placeholder="prod-web-01"
			/>
		</div>
		<div class="grid grid-cols-3 gap-2">
			<div class="col-span-2 grid gap-1.5">
				<Label for="host-address" class="text-xs">Address</Label>
				<Input
					id="host-address"
					bind:value={form.address}
					disabled={busy}
					class="h-8 font-mono text-xs"
					placeholder="10.0.0.12"
				/>
			</div>
			<div class="grid gap-1.5">
				<Label for="host-port" class="text-xs">Port</Label>
				<Input
					id="host-port"
					type="number"
					bind:value={form.port}
					disabled={busy}
					class="h-8 font-mono text-xs"
				/>
			</div>
		</div>
		<div class="grid gap-1.5">
			<Label for="host-username" class="text-xs">Username</Label>
			<Input
				id="host-username"
				bind:value={form.username}
				disabled={busy}
				class="h-8 font-mono text-xs"
				placeholder="deploy"
			/>
		</div>
		<div class="grid gap-1.5">
			<Label for="host-group" class="text-xs">Group</Label>
			<Select type="single" bind:value={form.groupId} items={groupOptions} disabled={busy}>
				<SelectTrigger id="host-group" class="h-8 w-full text-xs">
					<SelectValue placeholder="None" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value={NO_RELATION}>None</SelectItem>
					{#each groups as group (group.id)}
						<SelectItem value={group.id}>{group.name}</SelectItem>
					{/each}
				</SelectContent>
			</Select>
		</div>
		<div class="grid gap-1.5">
			<Label for="host-credential" class="text-xs">Credential</Label>
			<Select
				type="single"
				bind:value={form.credentialId}
				items={credentialOptions}
				disabled={busy}
			>
				<SelectTrigger id="host-credential" class="h-8 w-full text-xs">
					<SelectValue placeholder="None" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value={NO_RELATION}>None</SelectItem>
					{#each credentials as credential (credential.id)}
						<SelectItem value={credential.id}>{credential.name}</SelectItem>
					{/each}
				</SelectContent>
			</Select>
		</div>
	</form>

	{#snippet footer()}
		<Button type="submit" form="host-form" class="w-full text-xs" disabled={busy}>
			{mode === 'create' ? 'Create host' : 'Save host'}
		</Button>
		<Button
			type="button"
			variant="ghost"
			class="w-full text-xs text-muted-foreground"
			onclick={onclose}
		>
			Cancel
		</Button>
	{/snippet}
</SidePanel>
