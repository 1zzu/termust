export type InventoryContext = { kind: 'host'; hostId: string } | { kind: 'quick-connect' } | null;

/**
 * Selects a host, closing Quick Connect or switching from another host.
 */
export function selectHostContext(hostId: string): InventoryContext {
	return { kind: 'host', hostId };
}

/**
 * Toggles Quick Connect panel: opens if not active, closes if already active.
 */
export function toggleQuickConnectContext(current: InventoryContext): InventoryContext {
	return current?.kind === 'quick-connect' ? null : { kind: 'quick-connect' };
}

/**
 * Closes the active context panel completely.
 */
export function closeContext(): InventoryContext {
	return null;
}

/**
 * Prunes the context if the currently selected host is no longer visible
 * (e.g. after search or group filter changes, or after host deletion).
 */
export function pruneContextAfterFilter(
	current: InventoryContext,
	visibleHostIds: readonly string[]
): InventoryContext {
	if (current?.kind === 'host' && !visibleHostIds.includes(current.hostId)) {
		return null;
	}
	return current;
}

/**
 * Determines whether double-click can trigger a connection.
 * Connect is allowed only if not busy, not already connecting, and the host has credentials.
 */
export function canTriggerDoubleClickConnect(
	busy: boolean,
	connecting: boolean,
	connectable: boolean
): boolean {
	return !busy && !connecting && connectable;
}
