import type { Host } from '#lib/inventory';

export const ALL_GROUPS_FILTER = 'all';
export const UNGROUPED_FILTER = 'ungrouped';

export interface HostFilterOptions {
	query?: string;
	groupId?: string;
}

/**
 * Filters a list of hosts by group and search query without mutating the original array.
 * Search matching is case-insensitive and checks label, address, and username.
 */
export function filterHosts(hosts: readonly Host[], options: HostFilterOptions = {}): Host[] {
	const { query = '', groupId = ALL_GROUPS_FILTER } = options;
	const trimmedQuery = query.trim().toLowerCase();

	return hosts.filter((host) => {
		// Group filter
		if (groupId === UNGROUPED_FILTER) {
			if (host.groupId !== null && host.groupId !== '') {
				return false;
			}
		} else if (groupId !== ALL_GROUPS_FILTER && groupId !== '') {
			if (host.groupId !== groupId) {
				return false;
			}
		}

		// Search query filter (matches label, address, or username)
		if (trimmedQuery !== '') {
			const labelMatch = host.label.toLowerCase().includes(trimmedQuery);
			const addressMatch = host.address.toLowerCase().includes(trimmedQuery);
			const usernameMatch = host.username.toLowerCase().includes(trimmedQuery);
			if (!labelMatch && !addressMatch && !usernameMatch) {
				return false;
			}
		}

		return true;
	});
}
