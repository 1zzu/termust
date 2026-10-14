import { invoke } from '@tauri-apps/api/core';

import { isConnected } from './ssh';
import type { ConnectionStatus } from './types';
import type { VaultStatus } from './vault';

/**
 * A command kept encrypted in the local vault. Mirrors `Snippet` in
 * `src-tauri/src/snippets/model.rs`.
 *
 * The content reaches the frontend because the UI has to show it in full before
 * anyone can run it, and the edit form needs it. It is never logged, never put
 * into an error, and never rendered as HTML.
 */
export interface Snippet {
	id: string;
	name: string;
	content: string;
	createdAt: number;
	updatedAt: number;
}

/** What the frontend sends to create or update a snippet. Mirrors `SnippetInput`. */
export interface SnippetInput {
	name: string;
	content: string;
}

/**
 * What `run_snippet` takes: which session and which snippet, and nothing else.
 *
 * The command never accepts content, so the text a session receives can only
 * come from the vault.
 */
export interface RunSnippetRequest {
	sessionId: string;
	snippetId: string;
}

/** The backend refuses a longer name, and this is the same limit. */
export const MAX_SNIPPET_NAME_LENGTH = 120;
/** The backend measures the content in bytes, and so does the form. */
export const MAX_SNIPPET_CONTENT_BYTES = 64 * 1024;

/** How much of the content the list shows before the preview dialog is opened. */
const PREVIEW_LENGTH = 80;

/**
 * Shown beside the snippet form. A snippet is sent to the remote shell as typed
 * input, so it is not a place for anything that has to stay secret.
 */
export const SNIPPET_SECRET_NOTE =
	'Snippets are not a secret store. They are sent to the remote shell as plain input, so keep passwords, tokens and private keys in Credentials instead.';

// --- IPC ------------------------------------------------------------------

export function listSnippets(): Promise<Snippet[]> {
	return invoke('list_snippets');
}

export function createSnippet(snippet: SnippetInput): Promise<Snippet> {
	return invoke('create_snippet', { snippet });
}

export function updateSnippet(id: string, snippet: SnippetInput): Promise<Snippet> {
	return invoke('update_snippet', { id, snippet });
}

export function deleteSnippet(id: string): Promise<void> {
	return invoke('delete_snippet', { id });
}

/**
 * Runs one stored snippet against one session.
 *
 * Nothing here runs on its own: this is only called by the confirmation button
 * of the preview dialog, and the content stays in the vault until then.
 */
export function runSnippet(request: RunSnippetRequest): Promise<void> {
	return invoke('run_snippet', { request });
}

/**
 * The payload `run_snippet` takes, built in one place so that these two ids, and
 * only these two, are what leaves the frontend.
 */
export function runSnippetRequest(sessionId: string, snippetId: string): RunSnippetRequest {
	return { sessionId, snippetId };
}

// --- the list -------------------------------------------------------------

/**
 * One or two lines of the content, as plain text, for the list.
 *
 * Whitespace is collapsed so an indented one-liner stays readable, and the
 * result is returned exactly as it is: the component interpolates it, so a
 * snippet that looks like HTML is shown as the characters it is made of.
 */
export function snippetPreview(content: string): string {
	const lines = content.split(/\r\n|\r|\n/).slice(0, 2);
	const collapsed = lines.join(' ↵ ').replace(/\s+/g, ' ').trim();
	return collapsed.length > PREVIEW_LENGTH ? `${collapsed.slice(0, PREVIEW_LENGTH)}…` : collapsed;
}

/** The content size the backend enforces, measured the way it does: in bytes. */
export function snippetByteLength(content: string): number {
	return new TextEncoder().encode(content).length;
}

/**
 * Snippets live in the inventory panel and follow the same rule: they are only
 * ever on screen while the vault is open. The panel throws its snippet list away
 * with the rest of its state when this turns false.
 */
export function snippetsVisible(status: VaultStatus | undefined): boolean {
	return status === 'unlocked';
}

// --- the form -------------------------------------------------------------

export interface SnippetFormValues {
	name: string;
	content: string;
}

export function emptySnippetForm(): SnippetFormValues {
	return { name: '', content: '' };
}

/** The form for editing a stored snippet, loaded from what the list returned. */
export function snippetFormFor(snippet: Snippet): SnippetFormValues {
	return { name: snippet.name, content: snippet.content };
}

/**
 * Checks the form before anything is sent. The message never repeats the
 * content: only what it is missing or why it cannot be stored.
 */
export function snippetFormProblem(form: SnippetFormValues): string | null {
	if (form.name.trim() === '') {
		return 'Enter a name for this snippet.';
	}
	if (form.content === '') {
		return 'Enter the command this snippet should run.';
	}
	if (snippetByteLength(form.content) > MAX_SNIPPET_CONTENT_BYTES) {
		return `This snippet is larger than ${MAX_SNIPPET_CONTENT_BYTES} bytes.`;
	}
	return null;
}

/**
 * The exact request the backend expects: the name is trimmed and the content is
 * passed through untouched, because whitespace and quoting inside a command are
 * the user's, not this code's.
 */
export function snippetInput(form: SnippetFormValues): SnippetInput {
	return { name: form.name.trim(), content: form.content };
}

// --- running one ----------------------------------------------------------

/**
 * Whether `Run snippet` may be offered at all.
 *
 * A snippet is only ever sent to a session that is really up: without a session
 * id, or while the connection is still being made, disconnected or failed, the
 * button is disabled. `running` also keeps a second click from starting a second
 * run while the first is still in flight.
 */
export function runEnabled(
	connection: ConnectionStatus,
	sessionId: string | undefined,
	running: boolean
): boolean {
	return !running && sessionId !== undefined && sessionId !== '' && isConnected(connection);
}
