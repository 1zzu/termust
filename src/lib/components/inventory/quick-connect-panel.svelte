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
	import SidePanel from './side-panel.svelte';

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
</script>

<SidePanel label="Quick Connect" kicker="Quick Connect" title="Direct Session" {onclose}>
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
</SidePanel>
