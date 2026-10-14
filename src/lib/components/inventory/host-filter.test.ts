import { describe, expect, it } from 'vitest';
import type { Host } from '#lib/inventory';
import { ALL_GROUPS_FILTER, UNGROUPED_FILTER, filterHosts } from './host-filter';

function makeHost(overrides: Partial<Host> = {}): Host {
	return {
		id: 'host-1',
		label: 'Production Web',
		address: 'web.example.com',
		port: 22,
		username: 'deploy',
		groupId: null,
		credentialId: 'cred-1',
		createdAt: 1000,
		updatedAt: 1000,
		...overrides
	};
}

describe('filterHosts', () => {
	const host1 = makeHost({
		id: 'h1',
		label: 'Production Web Server',
		address: 'web.example.com',
		username: 'ubuntu',
		groupId: 'group-prod'
	});
	const host2 = makeHost({
		id: 'h2',
		label: 'Staging Database',
		address: '10.0.0.50',
		username: 'postgres',
		groupId: 'group-stage'
	});
	const host3 = makeHost({
		id: 'h3',
		label: 'Dev Bastion Jump',
		address: 'bastion.dev.internal',
		username: 'admin',
		groupId: null
	});

	const sampleHosts: Host[] = [host1, host2, host3];

	it('returns all hosts when query is empty and group is all', () => {
		const result = filterHosts(sampleHosts, { query: '', groupId: ALL_GROUPS_FILTER });
		expect(result).toEqual(sampleHosts);
	});

	it('performs case-insensitive search', () => {
		const lowerResult = filterHosts(sampleHosts, { query: 'production' });
		const upperResult = filterHosts(sampleHosts, { query: 'PRODUCTION' });
		const mixedResult = filterHosts(sampleHosts, { query: 'pRoDuCtIoN' });

		expect(lowerResult).toHaveLength(1);
		expect(lowerResult[0].id).toBe('h1');
		expect(upperResult).toEqual(lowerResult);
		expect(mixedResult).toEqual(lowerResult);
	});

	it('matches search query across label, address, and username', () => {
		// Matches label
		const labelMatch = filterHosts(sampleHosts, { query: 'database' });
		expect(labelMatch).toHaveLength(1);
		expect(labelMatch[0].id).toBe('h2');

		// Matches address
		const addressMatch = filterHosts(sampleHosts, { query: '10.0.0.50' });
		expect(addressMatch).toHaveLength(1);
		expect(addressMatch[0].id).toBe('h2');

		// Matches username
		const usernameMatch = filterHosts(sampleHosts, { query: 'admin' });
		expect(usernameMatch).toHaveLength(1);
		expect(usernameMatch[0].id).toBe('h3');
	});

	it('filters by specific group', () => {
		const prodHosts = filterHosts(sampleHosts, { groupId: 'group-prod' });
		expect(prodHosts).toHaveLength(1);
		expect(prodHosts[0].id).toBe('h1');

		const stageHosts = filterHosts(sampleHosts, { groupId: 'group-stage' });
		expect(stageHosts).toHaveLength(1);
		expect(stageHosts[0].id).toBe('h2');

		const nonExistent = filterHosts(sampleHosts, { groupId: 'group-unknown' });
		expect(nonExistent).toHaveLength(0);
	});

	it('filters ungrouped hosts', () => {
		const ungrouped = filterHosts(sampleHosts, { groupId: UNGROUPED_FILTER });
		expect(ungrouped).toHaveLength(1);
		expect(ungrouped[0].id).toBe('h3');
	});

	it('combines search query and group filter', () => {
		// 'web' exists in prod
		const matchBoth = filterHosts(sampleHosts, {
			query: 'web',
			groupId: 'group-prod'
		});
		expect(matchBoth).toHaveLength(1);
		expect(matchBoth[0].id).toBe('h1');

		// 'web' does not exist in staging
		const matchQueryWrongGroup = filterHosts(sampleHosts, {
			query: 'web',
			groupId: 'group-stage'
		});
		expect(matchQueryWrongGroup).toHaveLength(0);

		// 'bastion' is ungrouped
		const matchUngrouped = filterHosts(sampleHosts, {
			query: 'bastion',
			groupId: UNGROUPED_FILTER
		});
		expect(matchUngrouped).toHaveLength(1);
		expect(matchUngrouped[0].id).toBe('h3');
	});

	it('does not mutate the input hosts array', () => {
		const originalArray = [host1, host2, host3];
		const frozenCopy = Object.freeze([...originalArray]);

		const result = filterHosts(frozenCopy, { query: 'database', groupId: 'group-stage' });

		expect(result).toHaveLength(1);
		expect(originalArray).toHaveLength(3);
		expect(originalArray[0]).toBe(host1);
		expect(originalArray[1]).toBe(host2);
		expect(originalArray[2]).toBe(host3);
	});
});
