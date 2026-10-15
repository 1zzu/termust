/**
 * The part of the inventory workspace that every page is handed.
 *
 * The workspace owns the vault watch, the decrypted lists, the busy flag and
 * the one alert failures are shown in. A page owns its list view, its dialogs
 * and its forms, and changes the vault only through `onmutate`, so one place
 * stays responsible for re-reading the lists and for what the user is told.
 */
export interface InventoryPageProps {
	/** True while a vault change is on its way: a page disables its actions. */
	busy: boolean;
	/**
	 * Runs one change against the vault: the workspace goes busy, clears the
	 * alert, re-reads every list, and reports a failure in that alert instead
	 * of throwing. Returns whether the change itself went through.
	 */
	onmutate: (work: () => Promise<void>) => Promise<boolean>;
	/**
	 * Reports a problem the page found itself, such as a form that is not
	 * ready. An empty message clears the alert.
	 */
	onerror: (message: string) => void;
}
