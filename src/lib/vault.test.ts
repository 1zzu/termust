import { describe, expect, it } from 'vitest';

import { createPasswordProblem, vaultStatusLabel, type VaultStatus } from './vault';

describe('createPasswordProblem', () => {
	it('accepts a non-empty password whose confirmation matches', () => {
		expect(createPasswordProblem('correct-horse', 'correct-horse')).toBeNull();
	});

	it('requires a master password', () => {
		expect(createPasswordProblem('', '')).toBe('Enter a master password.');
	});

	it('requires the confirmation to match', () => {
		expect(createPasswordProblem('one', 'two')).toBe(
			'The master password and confirmation do not match.'
		);
	});
});

describe('vaultStatusLabel', () => {
	it('has a label for every vault state', () => {
		const statuses: VaultStatus[] = ['uninitialized', 'locked', 'unlocked'];

		for (const status of statuses) {
			expect(vaultStatusLabel(status).length).toBeGreaterThan(0);
		}
	});
});
