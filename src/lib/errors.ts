/**
 * Tauri rejects a command with whatever error it returned as a string. Anything
 * else reaching here is a bug rather than something the user can act on.
 */
export function errorMessage(error: unknown): string {
	if (typeof error === 'string' && error.trim().length > 0) {
		return error;
	}
	if (error instanceof Error && error.message.trim().length > 0) {
		return error.message;
	}
	return 'Something went wrong.';
}
