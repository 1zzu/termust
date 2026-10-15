<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.png';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<svelte:head>
	<link rel="icon" type="image/png" href={favicon} />
	<title>Termust</title>
</svelte:head>

{@render children()}
