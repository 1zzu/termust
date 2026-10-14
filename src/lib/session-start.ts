/**
 * The start of one session, as its own little lifecycle.
 *
 * A tab's terminal has to exist and the window's event listeners have to be in
 * place before the backend is asked to connect, and all of that happens after
 * the user has already clicked Connect. In that window the tab can be closed
 * again, so the start has to be cancellable without ever letting the backend
 * register a session that no tab can reach.
 *
 * The control is deliberately small: it owns one promise, one cancellation
 * signal and one phase, and it is given everything else it needs. Nothing here
 * touches a terminal, a tab list or the IPC layer.
 *
 * It also owns one plan — for Quick Connect, the credentials the user typed. The
 * plan arrives as this module's own binding rather than as a field of the
 * dependency object, because that object is closed over by the callbacks the
 * returned control keeps alive and would therefore hold a credential for as long
 * as the control exists.
 */

/** Where a start is in its life. */
export type StartPhase = 'queued' | 'invoking' | 'finished';

export interface SessionStartDeps<Plan> {
	/** The id the session is being started under. */
	readonly sessionId: string;
	/**
	 * Resolves when the window's event listeners are all in place, and rejects
	 * when they are not: without them a session would run invisibly, so it is
	 * never started at all.
	 */
	readonly listenersReady: Promise<void>;
	/** Whether the tab is still open and not on its way out. */
	readonly tabExists: (sessionId: string) => boolean;
	/** The backend call. It is handed the plan so that nothing else holds it. */
	readonly invoke: (plan: Plan) => Promise<void>;
	/** A failure of a start that was neither cancelled nor closed. */
	readonly onError: (sessionId: string, error: unknown) => void;
	/**
	 * Runs exactly once, however the start ends: cancellation before the call,
	 * a refusal, a success, or a cancellation while the call was in flight. It is
	 * told which session ended and nothing else, so it cannot hold on to a plan
	 * — and to the credential a Quick Connect one carries — by accident.
	 */
	readonly onFinished: (sessionId: string) => void;
}

export interface SessionStart<Plan> {
	readonly sessionId: string;
	/** Resolves once the start has settled, whichever way it did. Waiting on it
	 * is how a close knows an in-flight request is over. */
	readonly done: Promise<void>;
	readonly phase: StartPhase;
	/** Whether the backend may have been asked to start a session. */
	readonly invoked: boolean;
	/** Whether this start was cancelled — by a close, or by window teardown. */
	readonly cancelled: boolean;
	/** The plan while this start still owns it, and `null` once it is finished. */
	heldPlan(): Plan | null;
	/** The terminal this tab needs exists now. */
	ready(): void;
	cancel(): void;
}

export function beginSessionStart<Plan>(
	ownedPlan: Plan | null,
	deps: SessionStartDeps<Plan>
): SessionStart<Plan> {
	let phase: StartPhase = 'queued';
	let cancelled = false;
	let invoked = false;

	let wakeCancelled!: () => void;
	// Resolving this is what lets a start that is still waiting give up at once,
	// instead of holding a close open for as long as the listeners take.
	const cancelledSignal = new Promise<void>((resolve) => {
		wakeCancelled = resolve;
	});

	let signalReady!: () => void;
	const terminalReady = new Promise<void>((resolve) => {
		signalReady = resolve;
	});

	let settle!: () => void;
	const done = new Promise<void>((resolve) => {
		settle = resolve;
	});

	async function run() {
		try {
			// Whichever happens first decides: the tab and the listeners being
			// ready, or the tab being closed.
			await Promise.race([Promise.all([terminalReady, deps.listenersReady]), cancelledSignal]);
			// A start that was cancelled, closed or whose listeners never came up
			// must not reach the backend at all.
			if (cancelled || !deps.tabExists(deps.sessionId)) return;

			phase = 'invoking';
			invoked = true;
			// The plan goes to the backend call and nowhere else: it is not copied
			// into a local, so the binding emptied in the `finally` below is the
			// last reference this module has of it.
			if (ownedPlan !== null) await deps.invoke(ownedPlan);
		} catch (error) {
			// A failure for a tab the user has closed is nobody's business any
			// more, and it is never reported on another tab.
			if (!cancelled && deps.tabExists(deps.sessionId)) deps.onError(deps.sessionId, error);
		} finally {
			phase = 'finished';
			// Let go of the plan — and of any credential in it — before the caller
			// is told the start is over. This binding is the only place this module
			// ever kept it, and the callbacks below are the only closures over it, so
			// emptying it here is what makes sure nothing this start leaves behind
			// — a tab, a finish callback, a later `heldPlan()` — can reach the
			// credentials. That is a deliberate release of the reference, not
			// zeroization: JavaScript cannot promise the bytes are gone.
			ownedPlan = null;
			deps.onFinished(deps.sessionId);
			settle();
		}
	}

	void run();

	return {
		sessionId: deps.sessionId,
		done,
		get phase() {
			return phase;
		},
		get invoked() {
			return invoked;
		},
		get cancelled() {
			return cancelled;
		},
		heldPlan: () => ownedPlan,
		ready: () => signalReady(),
		cancel() {
			// Cancelling twice is not an error, and a start that already finished
			// has nothing left to cancel.
			cancelled = true;
			wakeCancelled();
		}
	};
}
