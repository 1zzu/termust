import { describe, expect, it } from 'vitest';
import {
	belongsToSession,
	decodeOutput,
	errorMessage,
	hostKeyPromise,
	isConnected,
	statusLabel
} from './ssh';
import type { ConnectionStatus } from './types';

describe('belongsToSession', () => {
	it('accepts output meant for the same session', () => {
		expect(belongsToSession({ sessionId: 'a', data: 'x' }, 'a')).toBe(true);
	});

	it('rejects output from another session so terminals cannot mix', () => {
		expect(belongsToSession({ sessionId: 'b', data: 'x' }, 'a')).toBe(false);
	});

	it('rejects payloads that carry no session id', () => {
		expect(belongsToSession({ data: 'x' }, 'a')).toBe(false);
		expect(belongsToSession(null, 'a')).toBe(false);
		expect(belongsToSession('a', 'a')).toBe(false);
		expect(belongsToSession({ sessionId: 1 }, 'a')).toBe(false);
	});
});

describe('decodeOutput', () => {
	it('decodes plain ASCII bytes', () => {
		expect(new TextDecoder().decode(decodeOutput('aGVsbG8='))).toBe('hello');
	});

	it('keeps multi-byte UTF-8 sequences intact', () => {
		// Base64 of the bytes 68 c3 a9 6c 6c 6f, which is "héllo" in UTF-8.
		const bytes = decodeOutput('aMOpbGxv');
		expect(Array.from(bytes)).toEqual([0x68, 0xc3, 0xa9, 0x6c, 0x6c, 0x6f]);
		expect(new TextDecoder().decode(bytes)).toBe('héllo');
	});

	it('preserves escape sequences rather than stripping them', () => {
		// Base64 of ESC [ 0 m, the SGR reset a shell sends constantly.
		expect(Array.from(decodeOutput('G1swbQ=='))).toEqual([0x1b, 0x5b, 0x30, 0x6d]);
	});

	it('returns an empty array for empty output', () => {
		expect(decodeOutput('').length).toBe(0);
	});
});

describe('statusLabel', () => {
	it('has a label for every status the UI can show', () => {
		const statuses: ConnectionStatus[] = [
			'idle',
			'connecting',
			'verifying_host_key',
			'authenticating',
			'connected',
			'disconnected',
			'error'
		];

		for (const status of statuses) {
			expect(statusLabel(status).length).toBeGreaterThan(0);
		}
		expect(statusLabel('verifying_host_key')).toBe('Checking host key');
	});
});

describe('isConnected', () => {
	it('only treats a live shell as connected', () => {
		expect(isConnected('connected')).toBe(true);
		expect(isConnected('authenticating')).toBe(false);
		expect(isConnected('disconnected')).toBe(false);
	});
});

describe('hostKeyPromise', () => {
	it('promises only what the vault can keep', () => {
		const stored = hostKeyPromise(true);
		const temporary = hostKeyPromise(false);

		expect(stored).toContain('local vault');
		expect(stored).not.toContain('until the app closes');
		expect(temporary).toContain('until the app closes');
		expect(temporary).toContain('Unlock the vault');
		expect(temporary).not.toContain('will not ask again');
	});
});

describe('errorMessage', () => {
	it('uses the message the backend returned', () => {
		expect(errorMessage('Authentication failed.')).toBe('Authentication failed.');
	});

	it('falls back to an Error message', () => {
		expect(errorMessage(new Error('boom'))).toBe('boom');
	});

	it('never surfaces an empty or non-text failure', () => {
		expect(errorMessage('')).toBe('Something went wrong.');
		expect(errorMessage('   ')).toBe('Something went wrong.');
		expect(errorMessage({ unexpected: true })).toBe('Something went wrong.');
		expect(errorMessage(undefined)).toBe('Something went wrong.');
	});
});
