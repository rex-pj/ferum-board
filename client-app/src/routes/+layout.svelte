<svelte:head>
	<link rel="icon" href={data?.faviconUrl || '/favicon.svg'} />
</svelte:head>

<script lang="ts">
	import { onMount } from 'svelte';
	import '../app.css';
	import ToastContainer from '$lib/components/organisms/ToastContainer.svelte';
	import LoadingBar from '$lib/components/atoms/LoadingBar.svelte';

	let { data, children }: { data: any; children: any } = $props();

	$effect(() => {
		const color = data?.primaryColor;
		if (color && /^#[0-9a-fA-F]{6}$/.test(color)) {
			document.documentElement.style.setProperty('--bs-primary', color);
		} else {
			document.documentElement.style.removeProperty('--bs-primary');
		}
	});

	onMount(async () => {
		// Bootstrap JS must be loaded client-side only — it accesses window/document.
		// Placed here so every route group gets dropdown/modal/collapse support.
		await import('bootstrap/dist/js/bootstrap.bundle.min.js');
	});
</script>

<LoadingBar />
{@render children()}
<ToastContainer />
