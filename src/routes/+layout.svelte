<script lang="ts">
	import { dev } from '$app/environment';
	import '../app.css';
	import Sonner from '$lib/components/ui/sonner/sonner.svelte';
	import { onMount } from 'svelte';

	let { children } = $props();

	onMount(() => {
		if (dev) return;

		const preventBrowserContextMenu = (event: MouseEvent) => event.preventDefault();
		document.addEventListener('contextmenu', preventBrowserContextMenu);

		return () => {
			document.removeEventListener('contextmenu', preventBrowserContextMenu);
		};
	});
</script>

{@render children()}
<Sonner expand closeButton closeButtonAriaLabel="Close notification" />
