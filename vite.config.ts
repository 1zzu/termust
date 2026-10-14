import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			// SvelteKit 3 dropped the built-in `$lib` alias in favour of `#lib` from
			// package.json imports. shadcn-svelte still generates `$lib` imports, and
			// re-adding the alias is the migration path SvelteKit documents.
			alias: { $lib: 'src/lib' },

			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			// The Tauri shell loads a static frontend from disk, so we emit a single-page build.
			adapter: adapter({ fallback: 'index.html' })
		})
	]
});
