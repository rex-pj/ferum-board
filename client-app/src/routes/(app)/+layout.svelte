<script lang="ts">
	import '../../app.css';
	import { onMount } from 'svelte';
	import SiteHeader from '$lib/components/organisms/SiteHeader.svelte';
	import CategoryNav from '$lib/components/organisms/CategoryNav.svelte';
	import SiteFooter from '$lib/components/organisms/SiteFooter.svelte';
	import { theme } from '$lib/stores/theme';

	let { data, children }: { data: any; children: any } = $props();

	onMount(() => {
		if (data?.theme && ['auto', 'light', 'dark'].includes(data.theme)) {
			theme.set(data.theme);
		}
	});

	$effect(() => {
		const color = data?.primaryColor;
		if (color && /^#[0-9a-fA-F]{6}$/.test(color)) {
			document.documentElement.style.setProperty('--bs-primary', color);
		} else {
			document.documentElement.style.removeProperty('--bs-primary');
		}
	});
</script>

<div class="fr-app">
	<SiteHeader user={data?.user} siteName={data?.siteName} logoUrl={data?.logoUrl} categories={data?.categories} />

	<div class="fr-body">
		<aside class="fr-sidebar d-none d-lg-block">
			<CategoryNav categories={data?.categories} user={data?.user} />
		</aside>

		<div
			class="offcanvas offcanvas-start"
			tabindex="-1"
			id="frSidebar"
			aria-labelledby="frSidebarLabel"
			style="width: var(--fr-sidebar-width); background: var(--fr-sidebar-bg); border-right: 1px solid var(--fr-sidebar-border);"
		>
			<div
				class="offcanvas-header"
				style="height: var(--fr-topbar-height); border-bottom: 1px solid var(--bs-border-color); padding: 0 1rem;"
			>
				<span class="fw-bold" id="frSidebarLabel">{data?.siteName ?? 'Ferum Board'}</span>
				<button type="button" class="btn-close" data-bs-dismiss="offcanvas" aria-label="Close"></button>
			</div>
			<div class="offcanvas-body p-0">
				<CategoryNav categories={data?.categories} user={data?.user} />
			</div>
		</div>

		<main class="fr-main">
			{@render children()}
		</main>
	</div>

	<SiteFooter siteName={data?.siteName} logoUrl={data?.logoUrl} />
</div>
