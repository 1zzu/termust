<script lang="ts">
	import type { AuthMethod } from '#lib/types';
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
	import { Textarea } from '#lib/components/ui/textarea';
	import XIcon from '@lucide/svelte/icons/x';

	let {
		host = $bindable('localhost'),
		port = $bindable(22),
		username = $bindable(''),
		auth = $bindable<AuthMethod>('password'),
		password = $bindable(''),
		privateKey = $bindable(''),
		passphrase = $bindable(''),
		submitting = false,
		onsubmit,
		onclose
	}: {
		host: string;
		port: number;
		username: string;
		auth: AuthMethod;
		password: string;
		privateKey: string;
		passphrase: string;
		submitting?: boolean;
		onsubmit: (event: SubmitEvent) => void;
		onclose: () => void;
	} = $props();

	const authMethods = [
		{ value: 'password', label: 'Password' },
		{ value: 'privateKey', label: 'Private key' }
	];

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<aside
	class="flex w-full shrink-0 flex-col gap-4 overflow-y-auto rounded-lg border border-border bg-card p-4 lg:w-context-panel"
	aria-label="Quick Connect"
>
	<div class="flex items-start justify-between gap-2 border-b border-border/50 pb-3">
		<div class="min-w-0">
			<h4 class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
				Quick Connect
			</h4>
			<p class="truncate text-base font-semibold text-foreground">Direct Session</p>
		</div>
		<Button
			variant="ghost"
			size="sm"
			class="size-7 shrink-0 p-0 text-muted-foreground hover:text-foreground"
			onclick={onclose}
			aria-label="Close Quick Connect"
			title="Close Quick Connect"
		>
			<XIcon class="size-4" />
		</Button>
	</div>

	<p class="text-xs text-muted-foreground">
		Connects directly from this device with temporary credentials. Secrets are cleared after
		connection.
	</p>

	<form class="flex flex-col gap-3" {onsubmit}>
		<div class="grid gap-1">
			<Label for="quick-host" class="text-xs">Hostname or IP</Label>
			<Input
				id="quick-host"
				bind:value={host}
				placeholder="server.example.com"
				disabled={submitting}
				class="h-8 text-xs"
			/>
		</div>

		<div class="grid gap-1">
			<Label for="quick-port" class="text-xs">Port</Label>
			<Input
				id="quick-port"
				type="number"
				min="1"
				max="65535"
				bind:value={port}
				disabled={submitting}
				class="h-8 font-mono text-xs"
			/>
		</div>

		<div class="grid gap-1">
			<Label for="quick-username" class="text-xs">Username</Label>
			<Input
				id="quick-username"
				bind:value={username}
				placeholder="root"
				disabled={submitting}
				class="h-8 font-mono text-xs"
			/>
		</div>

		<div class="grid gap-1">
			<Label for="quick-auth" class="text-xs">Authentication</Label>
			<Select type="single" bind:value={auth} items={authMethods} disabled={submitting}>
				<SelectTrigger
					id="quick-auth"
					class="h-8 w-full text-xs"
					aria-label="Authentication method"
				>
					<SelectValue placeholder="Choose a method" />
				</SelectTrigger>
				<SelectContent>
					<SelectItem value="password">Password</SelectItem>
					<SelectItem value="privateKey">Private key</SelectItem>
				</SelectContent>
			</Select>
		</div>

		{#if auth === 'password'}
			<div class="grid gap-1">
				<Label for="quick-password" class="text-xs">Password</Label>
				<Input
					id="quick-password"
					type="password"
					bind:value={password}
					disabled={submitting}
					class="h-8 text-xs"
				/>
			</div>
		{:else}
			<div class="grid gap-1">
				<Label for="quick-private-key" class="text-xs">Private key (OpenSSH format)</Label>
				<Textarea
					id="quick-private-key"
					rows={4}
					class="font-mono text-xs"
					placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
					bind:value={privateKey}
					disabled={submitting}
				/>
			</div>
			<div class="grid gap-1">
				<Label for="quick-passphrase" class="text-xs">Passphrase (optional)</Label>
				<Input
					id="quick-passphrase"
					type="password"
					bind:value={passphrase}
					disabled={submitting}
					class="h-8 text-xs"
				/>
			</div>
		{/if}

		<div class="mt-2 flex flex-col gap-2 border-t border-border/50 pt-3">
			<Button
				type="submit"
				class="w-full text-xs"
				disabled={submitting}
				aria-label="Connect session"
			>
				{submitting ? 'Connecting…' : 'Connect'}
			</Button>
		</div>
	</form>
</aside>
