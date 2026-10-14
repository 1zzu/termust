import { beforeEach, describe, expect, it, vi } from 'vitest';

// The IPC boundary is the only thing mocked here: these tests pin the command
// names and argument names the backend has to answer to.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';

import {
	NO_RELATION,
	addedAt,
	credentialFormProblem,
	emptyCredentialForm,
	emptyHostForm,
	emptyInventory,
	forgetKnownHost,
	hostFormProblem,
	hostInput,
	inventoryVisible,
	listKnownHosts,
	type CredentialSummary,
	type HostFormValues,
	type KnownHost
} from './inventory';

const SECRET_MARKER = 'marker-secret-2f7';
const PASSPHRASE_MARKER = 'marker-passphrase-9c4';

function hostForm(overrides: Partial<HostFormValues> = {}): HostFormValues {
	return { ...emptyHostForm(), ...overrides };
}

describe('inventoryVisible', () => {
	it('shows the inventory only while the vault is open', () => {
		expect(inventoryVisible('unlocked')).toBe(true);
		expect(inventoryVisible('locked')).toBe(false);
		expect(inventoryVisible('uninitialized')).toBe(false);
		expect(inventoryVisible(undefined)).toBe(false);
	});
});

describe('emptyInventory', () => {
	it('empties every list the panel holds when the vault locks', () => {
		const cleared = emptyInventory();

		expect(cleared).toEqual({ hosts: [], groups: [], credentials: [], knownHosts: [] });
		// A new object each time, so two callers never share one array.
		expect(emptyInventory()).not.toBe(cleared);
	});
});

describe('known host IPC', () => {
	beforeEach(() => {
		vi.mocked(invoke).mockReset();
	});

	it('lists known hosts with no arguments', async () => {
		vi.mocked(invoke).mockResolvedValue([]);

		await listKnownHosts();

		expect(invoke).toHaveBeenCalledTimes(1);
		expect(vi.mocked(invoke).mock.calls[0][0]).toBe('list_known_hosts');
		expect(vi.mocked(invoke).mock.calls[0][1]).toBeUndefined();
	});

	it('forgets a known host by its id', async () => {
		vi.mocked(invoke).mockResolvedValue(null);

		await forgetKnownHost('known-1');

		expect(invoke).toHaveBeenCalledWith('forget_known_host', { id: 'known-1' });
	});
});

describe('KnownHost', () => {
	it('carries no secret and no address of its own', () => {
		const known: KnownHost = {
			id: 'known-1',
			hostKeyId: 'example.com:22',
			keyAlgorithm: 'ssh-ed25519',
			fingerprintSha256: 'SHA256:abc',
			createdAt: 1,
			updatedAt: 2
		};

		expect(Object.keys(known).sort()).toEqual([
			'createdAt',
			'fingerprintSha256',
			'hostKeyId',
			'id',
			'keyAlgorithm',
			'updatedAt'
		]);
	});
});

describe('addedAt', () => {
	it('reads the backend seconds as a date', () => {
		const seconds = 1_700_000_000;

		expect(addedAt(seconds)).toBe(new Date(seconds * 1000).toLocaleString());
		expect(addedAt(seconds)).not.toBe(new Date(seconds).toLocaleString());
	});
});

describe('hostFormProblem', () => {
	it('requires a label, an address, a username and a usable port', () => {
		expect(hostFormProblem(hostForm())).toBe('Enter a label for this host.');
		expect(hostFormProblem(hostForm({ label: 'web' }))).toBe('Enter a hostname or IP address.');
		expect(hostFormProblem(hostForm({ label: 'web', address: 'a.example.com' }))).toBe(
			'Enter a username.'
		);
		expect(
			hostFormProblem(hostForm({ label: 'web', address: 'a.example.com', username: 'root' }))
		).toBeNull();
	});

	it('rejects a port outside 1..65535', () => {
		for (const port of ['0', '65536', '70000', '', 'abc']) {
			expect(
				hostFormProblem(
					hostForm({ label: 'web', address: 'a.example.com', username: 'root', port })
				)
			).toBe('Port must be between 1 and 65535.');
		}
	});
});

describe('hostInput', () => {
	it('sends the chosen group and credential ids', () => {
		const input = hostInput(
			hostForm({
				label: '  web-1  ',
				address: ' web.example.com ',
				port: '2222',
				username: ' root ',
				groupId: 'group-1',
				credentialId: 'credential-1'
			})
		);

		expect(input).toEqual({
			label: 'web-1',
			address: 'web.example.com',
			port: 2222,
			username: 'root',
			groupId: 'group-1',
			credentialId: 'credential-1'
		});
	});

	it('sends no relationship when the selects are left at None', () => {
		const input = hostInput(hostForm({ groupId: NO_RELATION, credentialId: NO_RELATION }));

		expect(input.groupId).toBeNull();
		expect(input.credentialId).toBeNull();
	});
});

describe('emptyHostForm', () => {
	it('starts with no label and no relationships', () => {
		expect(emptyHostForm()).toEqual({
			label: '',
			address: '',
			port: '22',
			username: '',
			groupId: NO_RELATION,
			credentialId: NO_RELATION
		});
	});
});

describe('credentialFormProblem', () => {
	it('requires a name and a secret when creating', () => {
		expect(credentialFormProblem('create', emptyCredentialForm())).toBe(
			'Enter a name for this credential.'
		);
		expect(credentialFormProblem('create', { ...emptyCredentialForm(), name: 'Prod' })).toBe(
			'Enter a password.'
		);
		expect(
			credentialFormProblem('create', {
				...emptyCredentialForm(),
				name: 'Prod',
				kind: 'private_key'
			})
		).toBe('Paste the private key.');
		expect(
			credentialFormProblem('create', { ...emptyCredentialForm(), name: 'Prod', secret: 'x' })
		).toBeNull();
	});

	it('does not ask for the secret when merely renaming', () => {
		expect(credentialFormProblem('rename', { ...emptyCredentialForm(), name: 'Prod' })).toBeNull();
		expect(credentialFormProblem('rename', emptyCredentialForm())).toBe(
			'Enter a name for this credential.'
		);
	});

	it('does not ask for the name when merely replacing the secret', () => {
		expect(credentialFormProblem('replace', { ...emptyCredentialForm(), secret: 'x' })).toBeNull();
	});

	it('never repeats the secret in a validation message', () => {
		const problem = credentialFormProblem('create', {
			...emptyCredentialForm(),
			name: '',
			secret: SECRET_MARKER,
			passphrase: PASSPHRASE_MARKER
		});

		expect(problem).not.toBeNull();
		expect(problem).not.toContain(SECRET_MARKER);
		expect(problem).not.toContain(PASSPHRASE_MARKER);
	});
});

describe('emptyCredentialForm', () => {
	it('carries no secret, so a reset leaves nothing behind', () => {
		const form = emptyCredentialForm();

		expect(form.name).toBe('');
		expect(form.secret).toBe('');
		expect(form.passphrase).toBe('');
	});
});

describe('CredentialSummary', () => {
	it('has no field that could hold a secret', () => {
		const summary: CredentialSummary = {
			id: 'x',
			name: 'Prod',
			kind: 'password',
			createdAt: 1,
			updatedAt: 2
		};

		expect(Object.keys(summary).sort()).toEqual(['createdAt', 'id', 'kind', 'name', 'updatedAt']);
	});
});
