import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	server: {
		// Keep this in sync with `build.devUrl` in `src-tauri/tauri.conf.json`.
		// `strictPort` prevents Vite from silently moving to another port, which
		// would leave Tauri waiting forever for a frontend that never appears.
		port: 1420,
		strictPort: true
	},
	plugins: [
		tailwindcss(),
		sveltekit({
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
