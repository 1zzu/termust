<script lang="ts">
	import {
		NO_RELATION,
		type CredentialSummary,
		type Group,
		type Host,
		type HostFormValues
	} from '#lib/inventory';
	import type { VaultStatus } from '#lib/vault';
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
	import ArrowRightToLineIcon from '@lucide/svelte/icons/arrow-right-to-line';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import FolderIcon from '@lucide/svelte/icons/folder';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import MoreHorizontalIcon from '@lucide/svelte/icons/more-horizontal';
	import TagIcon from '@lucide/svelte/icons/tag';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import UserIcon from '@lucide/svelte/icons/user';

	let {
		mode = 'edit',
		host = null,
		form = $bindable(),
		groups = [],
		credentials = [],
		busy = false,
		connecting = false,
		vaultStatus,
		onsubmit,
		onconnect,
		ondelete,
		onclose
	}: {
		mode: 'create' | 'edit';
		host?: Host | null;
		form: HostFormValues;
		groups: Group[];
		credentials: CredentialSummary[];
		busy?: boolean;
		connecting?: boolean;
		vaultStatus?: VaultStatus;
		onsubmit: (event?: SubmitEvent) => void;
		onconnect?: () => void;
		ondelete?: () => void;
		onclose: () => void;
	} = $props();

	let showMoreMenu = $state(false);

	const groupOptions = $derived([
		{ value: NO_RELATION, label: 'Parent Group (None)' },
		...groups.map((group) => ({ value: group.id, label: group.name }))
	]);

	const credentialOptions = $derived([
		{ value: NO_RELATION, label: 'Manual auth (No credential)' },
		...credentials.map((cred) => ({
			value: cred.id,
			label: `${cred.name} (${cred.kind === 'password' ? 'Password' : 'Key'})`
		}))
	]);

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
	aria-label={mode === 'create' ? 'New Host' : 'Host Details'}
>
	<!-- Header: Title, Vault badge, Options and Close button -->
	<header class="flex items-start justify-between gap-2 pt-0.5">
		<div class="flex flex-col gap-0.5">
			<h3 class="text-sm font-semibold text-foreground">
				{mode === 'create' ? 'New Host' : 'Host Details'}
			</h3>
			<div class="flex items-center gap-1 text-[11px] text-muted-foreground">
				<span>{vaultStatus === 'unlocked' ? 'Personal vault' : 'Local vault'}</span>
				<ChevronDownIcon class="size-3 opacity-60" />
			</div>
		</div>

		<div class="flex items-center gap-0.5">
			{#if mode === 'edit' && ondelete}
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
									ondelete();
								}}
							>
								<Trash2Icon class="size-3.5" />
								<span>Delete host</span>
							</button>
						</div>
					{/if}
				</div>
			{/if}

			<Button
				variant="ghost"
				size="icon"
				class="size-7 text-muted-foreground hover:text-foreground"
				onclick={onclose}
				aria-label="Close host details"
				title="Close panel"
			>
				<ArrowRightToLineIcon class="size-3.5" />
			</Button>
		</div>
	</header>

	<!-- Scrollable Form Body -->
	<form
		id="host-inspector-form"
		class="flex flex-1 flex-col gap-4 overflow-y-auto pr-0.5 sm:gap-5"
		onsubmit={(e) => {
			e.preventDefault();
			if (onconnect) {
				onconnect();
			} else {
				onsubmit(e);
			}
		}}
	>
		<!-- 1. Address Section -->
		<div class="flex flex-col gap-1.5">
			<Label for="inspector-address" class="text-xs font-medium text-muted-foreground">
				Address
			</Label>
			<div class="flex items-center gap-2.5">
				<!-- Ubuntu / Linux circular badge -->
				<!-- <div
					class="flex size-9 shrink-0 items-center justify-center rounded-full bg-[#E95420] text-white shadow-xs"
					aria-hidden="true"
				>
					<svg viewBox="0 0 24 24" class="size-5 fill-current">
						<circle cx="12" cy="12" r="10" fill="#E95420" />
						<path
							fill="#ffffff"
							d="M12 4.5a7.5 7.5 0 0 0-7.3 5.8 2 2 0 0 1 .8.4 2 2 0 0 1 .7 1.3 7.5 7.5 0 0 0 0 0 2 2 0 0 1-.7 1.3 2 2 0 0 1-.8.4A7.5 7.5 0 1 0 12 4.5zm0 1.8a5.7 5.7 0 0 1 4.5 2.2 2 2 0 0 1-.9.7 2 2 0 0 1-.3 1.5 5.7 5.7 0 0 1 0 2.6 2 2 0 0 1 .3 1.5 2 2 0 0 1 .9.7 5.7 5.7 0 0 1-9 0 2 2 0 0 1 .9-.7 2 2 0 0 1 .3-1.5 5.7 5.7 0 0 1 0-2.6 2 2 0 0 1-.3-1.5 2 2 0 0 1-.9-.7A5.7 5.7 0 0 1 12 6.3z"
						/>
					</svg>
				</div> -->
				<Input
					id="inspector-address"
					bind:value={form.address}
					disabled={busy}
					placeholder="160.22.97.242 or hostname"
					class="h-9 border-border/60 bg-muted/30 font-mono text-xs focus-visible:ring-1"
					required
				/>
			</div>
		</div>

		<!-- 2. General Section (Label, Group) -->
		<div class="flex flex-col gap-2">
			<Label for="inspector-label" class="text-xs font-medium text-muted-foreground">General</Label>

			<!-- Host Name / Label -->
			<Input
				id="inspector-label"
				bind:value={form.label}
				disabled={busy}
				placeholder="Host name (e.g. sambat bunda sby)"
				class="h-9 border-border/60 bg-muted/30 text-xs"
				required
			/>

			<!-- Parent Group Picker -->
			<Select type="single" bind:value={form.groupId} items={groupOptions} disabled={busy}>
				<SelectTrigger
					id="inspector-group"
					class="h-9! w-full border-border/60 bg-muted/30 text-xs"
				>
					<div class="flex items-center gap-2 truncate">
						<FolderIcon class="size-3.5 shrink-0 text-muted-foreground" />
						<SelectValue placeholder="Parent Group" />
					</div>
				</SelectTrigger>
				<SelectContent>
					<SelectItem value={NO_RELATION}>Parent Group (None)</SelectItem>
					{#each groups as group (group.id)}
						<SelectItem value={group.id}>{group.name}</SelectItem>
					{/each}
				</SelectContent>
			</Select>
		</div>

		<!-- 3. Port Section -->
		<div class="flex flex-col gap-1.5">
			<Label for="inspector-port" class="text-xs font-medium text-muted-foreground">Port</Label>
			<Input
				id="inspector-port"
				type="number"
				bind:value={form.port}
				disabled={busy}
				placeholder="22"
				class="h-9 border-border/60 bg-muted/30 font-mono text-xs"
				min="1"
				max="65535"
				required
			/>
		</div>

		<!-- 4. Credentials Section -->
		<div class="flex flex-col gap-2">
			<span class="text-xs font-medium text-muted-foreground">Credentials</span>

			<!-- Username -->
			<div class="relative">
				<UserIcon
					class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					id="inspector-username"
					bind:value={form.username}
					disabled={busy}
					placeholder="SSH username (e.g. root, gonet)"
					class="h-9 border-border/60 bg-muted/30 pl-8 font-mono text-xs"
					required
				/>
			</div>

			<!-- Stored Credential -->
			<Select
				type="single"
				bind:value={form.credentialId}
				items={credentialOptions}
				disabled={busy}
			>
				<SelectTrigger
					id="inspector-credential"
					class="h-9! w-full border-border/60 bg-muted/30 text-xs"
				>
					<div class="flex items-center gap-2 truncate">
						<KeyRoundIcon class="size-3.5 shrink-0 text-muted-foreground" />
						<SelectValue placeholder="Stored Credential" />
					</div>
				</SelectTrigger>
				<SelectContent>
					<SelectItem value={NO_RELATION}>Manual auth (No credential)</SelectItem>
					{#each credentials as cred (cred.id)}
						<SelectItem value={cred.id}>
							{cred.name} ({cred.kind === 'password' ? 'Password' : 'Key'})
						</SelectItem>
					{/each}
				</SelectContent>
			</Select>
		</div>
	</form>

	<!-- Footer: Connect button (saves/creates and connects) -->
	<footer>
		<Button
			type="submit"
			form="host-inspector-form"
			class="h-10 w-full bg-blue-600 text-xs font-semibold text-white shadow-sm hover:bg-blue-500"
			disabled={busy || connecting}
		>
			{connecting ? 'Connecting…' : 'Connect'}
		</Button>
	</footer>
</aside>
