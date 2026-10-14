import { defineConfig } from 'vitest/config';

// Unit tests cover the small pure helpers in `src/lib`. They deliberately do not
// load the SvelteKit plugin, so they stay fast and run without a build step.
export default defineConfig({
	test: {
		environment: 'node',
		include: ['src/**/*.test.ts']
	}
});
