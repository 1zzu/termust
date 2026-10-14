import { describe, expect, it } from 'vitest';
import {
	canTriggerDoubleClickConnect,
	closeContext,
	pruneContextAfterFilter,
	selectHostContext,
	toggleQuickConnectContext,
	type InventoryContext
} from './context-state';

describe('context-state helpers', () => {
	it('selects a host context from null or another context', () => {
		const selected = selectHostContext('host-1');
		expect(selected).toEqual({ kind: 'host', hostId: 'host-1' });

		// Switching to another host
		expect(selectHostContext('host-2')).toEqual({ kind: 'host', hostId: 'host-2' });
		expect(selectHostContext('host-3')).toEqual({ kind: 'host', hostId: 'host-3' });
	});

	it('toggles quick-connect panel open and closed', () => {
		expect(toggleQuickConnectContext(null)).toEqual({ kind: 'quick-connect' });
		expect(toggleQuickConnectContext({ kind: 'host', hostId: 'h1' })).toEqual({
			kind: 'quick-connect'
		});
		expect(toggleQuickConnectContext({ kind: 'quick-connect' })).toBeNull();
	});

	it('closes context completely', () => {
		expect(closeContext()).toBeNull();
	});

	it('prunes host context when selected host is no longer visible', () => {
		const context: InventoryContext = { kind: 'host', hostId: 'host-1' };

		// Still visible
		expect(pruneContextAfterFilter(context, ['host-1', 'host-2'])).toEqual(context);

		// Not visible
		expect(pruneContextAfterFilter(context, ['host-2', 'host-3'])).toBeNull();

		// Quick connect context is not pruned by host visibility
		const qcContext: InventoryContext = { kind: 'quick-connect' };
		expect(pruneContextAfterFilter(qcContext, [])).toEqual(qcContext);

		// Null context remains null
		expect(pruneContextAfterFilter(null, [])).toBeNull();
	});

	it('validates double click connect conditions', () => {
		// Normal connectable state
		expect(canTriggerDoubleClickConnect(false, false, true)).toBe(true);

		// Busy with an inventory operation
		expect(canTriggerDoubleClickConnect(true, false, true)).toBe(false);

		// Already connecting
		expect(canTriggerDoubleClickConnect(false, true, true)).toBe(false);

		// Credential required (not connectable)
		expect(canTriggerDoubleClickConnect(false, false, false)).toBe(false);

		// Multiple blocking conditions
		expect(canTriggerDoubleClickConnect(true, true, false)).toBe(false);
	});
});
