import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
	{
		ignores: ['build/', '.svelte-kit/', 'node_modules/', 'src-tauri/target/', 'dist/']
	},
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node }
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: {
			parserOptions: {
				projectService: true,
				extraFileExtensions: ['.svelte'],
				parser: ts.parser
			}
		}
	},
	{
		// The copied shadcn-svelte building blocks pass `href` straight through from
		// their caller instead of linking to a route of this app, so route
		// resolution does not apply to them.
		files: ['src/lib/components/ui/**/*.svelte'],
		rules: {
			'svelte/no-navigation-without-resolve': 'off'
		}
	}
);
