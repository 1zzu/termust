<script lang="ts">
	import type { CredentialSummary, Host } from '#lib/inventory';
	import { addedAt, canConnectHost, hostConnectionHint } from '#lib/inventory';
	import { Button } from '#lib/components/ui/button';
	import SidePanel from './side-panel.svelte';

	let {
		host,
		groupName = '',
		credential = null,
		busy = false,
		connecting = false,
		onconnect,
		onedit,
		ondelete,
		onclose
	}: {
		host: Host;
		groupName?: string;
		credential?: CredentialSummary | null;
		busy?: boolean;
		connecting?: boolean;
		onconnect: (host: Host) => void;
		onedit: (host: Host) => void;
		ondelete: (host: Host) => void;
		onclose: () => void;
	} = $props();

	const connectable = $derived(canConnectHost(host));
	const hint = $derived(hostConnectionHint(host));
	const credentialType = $derived(
		credential ? (credential.kind === 'password' ? 'Password' : 'Private key') : ''
	);
</script>

<SidePanel label="Host details" kicker="Host Details" title={host.label} {onclose}>
	{#if hint}
		<div
			class="rounded-md border border-amber-500/30 bg-amber-500/10 p-2.5 text-xs text-amber-500"
			role="status"
		>
			<p class="font-medium">{hint}</p>
			<p class="mt-0.5 text-[11px] text-amber-500/80">
				Attach a credential via Edit before connecting.
			</p>
		</div>
	{/if}

	<dl class="grid gap-2.5 text-xs">
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Address</dt>
			<dd class="col-span-2 font-mono break-all text-foreground">{host.address}</dd>
		</div>
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Port</dt>
			<dd class="col-span-2 font-mono text-foreground">{host.port}</dd>
		</div>
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Username</dt>
			<dd class="col-span-2 font-mono break-all text-foreground">{host.username}</dd>
		</div>
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Group</dt>
			<dd class="col-span-2 wrap-break-word text-foreground">{groupName || 'None'}</dd>
		</div>
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Credential</dt>
			<dd class="col-span-2 wrap-break-word text-foreground">
				{credential ? credential.name : 'None'}
			</dd>
		</div>
		{#if credentialType}
			<div class="grid grid-cols-3 gap-1">
				<dt class="text-muted-foreground">Type</dt>
				<dd class="col-span-2 text-foreground">{credentialType}</dd>
			</div>
		{/if}
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Created</dt>
			<dd class="col-span-2 text-foreground">{addedAt(host.createdAt)}</dd>
		</div>
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Updated</dt>
			<dd class="col-span-2 text-foreground">{addedAt(host.updatedAt)}</dd>
		</div>
	</dl>

	{#snippet footer()}
		<Button
			class="w-full"
			disabled={busy || connecting || !connectable}
			onclick={() => onconnect(host)}
			aria-label="Connect to {host.label}"
		>
			{connecting ? 'Connecting…' : 'Connect'}
		</Button>
		<div class="flex gap-2">
			<Button
				variant="outline"
				class="flex-1 text-xs"
				disabled={busy}
				onclick={() => onedit(host)}
				aria-label="Edit {host.label}"
			>
				Edit
			</Button>
			<Button
				variant="outline"
				class="flex-1 text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
				disabled={busy}
				onclick={() => ondelete(host)}
				aria-label="Delete {host.label}"
			>
				Delete
			</Button>
		</div>
	{/snippet}
</SidePanel>
