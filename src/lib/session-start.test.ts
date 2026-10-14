import { describe, expect, it, vi } from 'vitest';

import { beginSessionStart, type SessionStartDeps } from './session-start';

/** A promise a test resolves or rejects whenever it wants, with no timers. */
function controllable() {
	let resolve!: () => void;
	let reject!: (error: unknown) => void;
	const promise = new Promise<void>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

/** Lets the promise chains that are already settled run to their end. */
async function settle(): Promise<void> {
	for (let turn = 0; turn < 50; turn += 1) await Promise.resolve();
}

type Plan = { kind: 'quick'; secret: string };

const PLAN: Plan = { kind: 'quick', secret: 'correct-horse-battery-staple' };

/** A start whose every dependency the test drives, one step at a time. */
function harness(options: { invoke?: (plan: Plan) => Promise<void>; plan?: Plan } = {}) {
	const calls: string[] = [];
	const errors: unknown[] = [];
	const finished = vi.fn();
	let tabExists = true;

	const terminal = controllable();
	const listeners = controllable();

	const deps: SessionStartDeps<Plan> = {
		sessionId: 'session-a',
		listenersReady: listeners.promise,
		tabExists: () => tabExists,
		invoke: async (plan) => {
			calls.push('invoke');
			if (options.invoke) await options.invoke(plan);
		},
		onError: (_sessionId, error) => errors.push(error),
		onFinished: finished
	};

	// The plan is handed over as this start's own value, never as a field of the
	// dependency object — that object outlives the start through the control's
	// callbacks.
	const control = beginSessionStart<Plan>(options.plan ?? PLAN, deps);
	// The tab's terminal becomes usable when the test says so: resolving it is
	// what tells the start its pane exists.
	void terminal.promise.then(() => control.ready());

	return {
		control,
		deps,
		calls,
		errors,
		finished,
		terminal,
		listeners,
		giveUpTab: () => {
			tabExists = false;
		}
	};
}

describe('a queued start', () => {
	it('does not reach the backend until the terminal and the listeners are in place', async () => {
		const test = harness();

		expect(test.control.phase).toBe('queued');
		test.terminal.resolve();
		await settle();
		expect(test.calls).toEqual([]);

		test.listeners.resolve();
		await test.control.done;

		expect(test.calls).toEqual(['invoke']);
		expect(test.control.invoked).toBe(true);
		expect(test.control.phase).toBe('finished');
		expect(test.finished).toHaveBeenCalledTimes(1);
	});

	it('is cancelled without waiting for the listeners, and never calls the backend', async () => {
		const test = harness();

		test.control.cancel();
		// The listeners never resolve: a close must not be held open by them.
		await test.control.done;

		test.terminal.resolve();
		await settle();

		expect(test.control.cancelled).toBe(true);
		expect(test.control.invoked).toBe(false);
		expect(test.calls).toEqual([]);
		expect(test.errors).toEqual([]);
		expect(test.finished).toHaveBeenCalledTimes(1);
	});

	it('does not reach the backend once the listeners fail', async () => {
		const test = harness();
		const failure = new Error('no events');

		test.terminal.resolve();
		test.listeners.reject(failure);
		await test.control.done;

		// The listeners' rejection is what stops the session, not a silent no-op.
		expect(test.calls).toEqual([]);
		expect(test.errors).toEqual([failure]);
		expect(test.finished).toHaveBeenCalledTimes(1);
	});

	it('does not reach the backend for a tab that is gone', async () => {
		const test = harness();

		test.giveUpTab();
		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		expect(test.calls).toEqual([]);
		expect(test.errors).toEqual([]);
		expect(test.finished).toHaveBeenCalledTimes(1);
	});
});

describe('a start that is already on its way', () => {
	it('waits for its call to settle before it is done', async () => {
		const call = controllable();
		const test = harness({ invoke: () => call.promise });

		test.terminal.resolve();
		test.listeners.resolve();
		await settle();
		expect(test.control.phase).toBe('invoking');

		let settled = false;
		void test.control.done.then(() => {
			settled = true;
		});

		test.control.cancel();
		await settle();
		expect(settled).toBe(false);
		expect(test.control.cancelled).toBe(true);

		call.resolve();
		await test.control.done;
		expect(settled).toBe(true);
		expect(test.finished).toHaveBeenCalledTimes(1);
	});

	it('does not report a failure of a start that was cancelled', async () => {
		const call = controllable();
		const test = harness({ invoke: () => call.promise });

		test.terminal.resolve();
		test.listeners.resolve();
		await settle();

		test.control.cancel();
		call.reject(new Error('refused'));
		await test.control.done;

		expect(test.errors).toEqual([]);
		expect(test.finished).toHaveBeenCalledTimes(1);
	});

	it('does not report a failure on a tab that is already gone', async () => {
		const call = controllable();
		const test = harness({ invoke: () => call.promise });

		test.terminal.resolve();
		test.listeners.resolve();
		await settle();

		test.giveUpTab();
		call.reject(new Error('refused'));
		await test.control.done;

		expect(test.errors).toEqual([]);
	});

	it('reports a failure once, and only for its own tab', async () => {
		const failure = new Error('the vault is locked');
		const test = harness({ invoke: () => Promise.reject(failure) });

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		expect(test.errors).toEqual([failure]);
		expect(test.finished).toHaveBeenCalledTimes(1);
	});
});

describe('what a start owns', () => {
	it('holds the plan while it runs, and lets go of it when it is over', async () => {
		const test = harness();
		expect(test.control.heldPlan()).toBe(PLAN);

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		// The credentials the plan carried are not referenced by this start any
		// more: nothing can read them through it after it finished.
		expect(test.control.heldPlan()).toBeNull();
	});

	it('still owns the plan while its call is in flight, and gives it up after', async () => {
		const call = controllable();
		const test = harness({ invoke: () => call.promise });

		test.terminal.resolve();
		test.listeners.resolve();
		await settle();

		// The call is still running, so the start still owns the plan it was
		// started with: a cancellation has to be able to wait for that call.
		expect(test.control.phase).toBe('invoking');
		expect(test.control.heldPlan()).toBe(PLAN);

		test.control.cancel();
		call.resolve();
		await test.control.done;

		expect(test.control.heldPlan()).toBeNull();
	});

	it('lets go of the plan on a cancellation too', async () => {
		const test = harness();

		test.control.cancel();
		await test.control.done;

		expect(test.control.heldPlan()).toBeNull();
	});

	it('lets go of the plan when the call fails', async () => {
		const failure = new Error('the vault is locked');
		const test = harness({ invoke: () => Promise.reject(failure) });

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		expect(test.errors).toEqual([failure]);
		expect(test.control.heldPlan()).toBeNull();
	});

	it('never keeps the plan on the dependency object the callbacks close over', async () => {
		const test = harness();

		// `deps` is captured by the callbacks the control keeps for as long as the
		// start lives, so it must not carry the plan at all — neither before nor
		// after the start finishes.
		expect(Object.keys(test.deps)).not.toContain('plan');
		expect(Object.values(test.deps)).not.toContain(PLAN);
		expect(Object.values(test.deps)).not.toContain(PLAN.secret);

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		expect(test.control.heldPlan()).toBeNull();
		expect(Object.values(test.deps)).not.toContain(PLAN);
	});

	it('tells the finish callback which session ended, and nothing else', async () => {
		const test = harness();

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		// One argument, the session id: the callback cannot need the plan, so it
		// cannot hold a credential through a captured argument either.
		expect(test.finished.mock.calls).toEqual([['session-a']]);
	});

	it('does not hand the plan back once it has finished', async () => {
		const test = harness();

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		test.control.cancel();
		void test.control.ready();
		await settle();

		expect(test.control.heldPlan()).toBeNull();
		expect(test.calls).toEqual(['invoke']);
	});

	it('hands the plan to the backend call, and nothing else', async () => {
		const seen: Plan[] = [];
		const test = harness({
			invoke: async (plan) => {
				seen.push(plan);
			}
		});

		test.terminal.resolve();
		test.listeners.resolve();
		await test.control.done;

		expect(seen).toEqual([PLAN]);
	});
});
