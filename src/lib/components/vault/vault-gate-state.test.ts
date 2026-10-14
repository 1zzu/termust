import { describe, expect, it } from 'vitest';
import tauriConf from '../../../../src-tauri/tauri.conf.json';
import { getVisualActiveSessionId, isVaultGateVisible } from './vault-gate-state';
import { inventoryVisible } from '#lib/inventory';

describe('vault-gate-state helpers', () => {
	it('shows gate when status is undefined (checking) and not dismissed', () => {
		expect(isVaultGateVisible(undefined, false)).toBe(true);
	});

	it('shows gate when uninitialized and not dismissed', () => {
		expect(isVaultGateVisible('uninitialized', false)).toBe(true);
	});

	it('shows gate when locked and not dismissed', () => {
		expect(isVaultGateVisible('locked', false)).toBe(true);
	});

	it('hides gate when unlocked regardless of dismissed state', () => {
		expect(isVaultGateVisible('unlocked', false)).toBe(false);
		expect(isVaultGateVisible('unlocked', true)).toBe(false);
	});

	it('hides gate when dismissed (Continue with Quick Connect bypass)', () => {
		expect(isVaultGateVisible('locked', true)).toBe(false);
		expect(isVaultGateVisible('uninitialized', true)).toBe(false);
		expect(isVaultGateVisible(undefined, true)).toBe(false);
	});

	it('shows gate again after lock action resets dismissed to false', () => {
		let dismissed = true;
		expect(isVaultGateVisible('locked', dismissed)).toBe(false);

		// Lock action resets dismissed to false
		dismissed = false;
		expect(isVaultGateVisible('locked', dismissed)).toBe(true);
	});

	it('keeps inventory invisible when vault is locked or uninitialized', () => {
		expect(inventoryVisible('locked')).toBe(false);
		expect(inventoryVisible('uninitialized')).toBe(false);
		expect(inventoryVisible(undefined)).toBe(false);
		expect(inventoryVisible('unlocked')).toBe(true);
	});
});

describe('visual terminal tab mapping', () => {
	it('maps activeSessionId to null when Hosts workspace is active', () => {
		const internalSessionId = 'session-123';
		expect(getVisualActiveSessionId('inventory', internalSessionId)).toBeNull();
	});

	it('maps activeSessionId to the actual session ID when Terminal workspace is active', () => {
		const internalSessionId = 'session-123';
		expect(getVisualActiveSessionId('terminal', internalSessionId)).toBe('session-123');
	});

	it('preserves underlying session ID when switching back from Hosts to Terminal', () => {
		const internalSessionId = 'session-456';
		// User on terminal
		expect(getVisualActiveSessionId('terminal', internalSessionId)).toBe('session-456');
		// User navigates to Hosts
		expect(getVisualActiveSessionId('inventory', internalSessionId)).toBeNull();
		// User returns to Terminal: underlying ID is restored visually
		expect(getVisualActiveSessionId('terminal', internalSessionId)).toBe('session-456');
	});
});

describe('Tauri window configuration for trafficLightPosition', () => {
	it('contains explicit trafficLightPosition with x: 14, y: 15 in tauri.conf.json', () => {
		const mainWindow = tauriConf.app.windows[0];

		expect(mainWindow.titleBarStyle).toBe('Overlay');
		expect(mainWindow.decorations).toBe(true);
		expect(mainWindow.hiddenTitle).toBe(true);
		expect(mainWindow.trafficLightPosition).toEqual({
			x: 14,
			y: 15
		});
	});
});
