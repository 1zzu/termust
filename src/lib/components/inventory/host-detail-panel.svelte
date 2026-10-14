<script lang="ts">
	import type { CredentialSummary, Host } from '#lib/inventory';
	import { addedAt, canConnectHost, hostConnectionHint } from '#lib/inventory';
	import { Button } from '#lib/components/ui/button';
	import XIcon from '@lucide/svelte/icons/x';

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

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<aside
	class="flex w-full shrink-0 flex-col gap-4 rounded-lg border border-border bg-card p-4 lg:w-context-panel"
	aria-label="Host details"
>
	<div class="flex items-start justify-between gap-2 border-b border-border/50 pb-3">
		<div class="min-w-0">
			<h4 class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
				Host Details
			</h4>
			<p class="truncate text-base font-semibold text-foreground" title={host.label}>
				{host.label}
			</p>
		</div>
		<Button
			variant="ghost"
			size="sm"
			class="size-7 shrink-0 p-0 text-muted-foreground hover:text-foreground"
			onclick={onclose}
			aria-label="Close details"
			title="Close details"
		>
			<XIcon class="size-4" />
		</Button>
	</div>

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
			<dd class="col-span-2 break-words text-foreground">{groupName || 'None'}</dd>
		</div>
		<div class="grid grid-cols-3 gap-1">
			<dt class="text-muted-foreground">Credential</dt>
			<dd class="col-span-2 break-words text-foreground">
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

	<div class="mt-auto flex flex-col gap-2 border-t border-border/50 pt-3">
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
	</div>
</aside>
