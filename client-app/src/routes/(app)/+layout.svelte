<script lang="ts">
	import '../../app.css';
	import { onMount } from 'svelte';
	import SiteHeader from '$lib/components/organisms/SiteHeader.svelte';
	import CategoryNav from '$lib/components/organisms/CategoryNav.svelte';
	import SiteFooter from '$lib/components/organisms/SiteFooter.svelte';
	import BottomNav from '$lib/components/organisms/BottomNav.svelte';
	import { theme } from '$lib/stores/theme';

	let { data, children }: { data: any; children: any } = $props();

	onMount(() => {
		if (data?.theme && ['auto', 'light', 'dark'].includes(data.theme)) {
			theme.set(data.theme);
		}
		if (data?.fontSizePref) {
			document.documentElement.setAttribute('data-font-size', data.fontSizePref);
		}
		if (data?.layoutPref) {
			document.documentElement.setAttribute('data-layout', data.layoutPref);
		}
	});

</script>

<div class="fr-app fr-has-bottom-nav">
	<SiteHeader user={data?.user} siteName={data?.siteName} siteSlogan={data?.siteSlogan} logoUrl={data?.logoUrl} categories={data?.categories} />

	<div class="fr-body">
		<aside class="fr-sidebar d-none d-lg-block">
			<CategoryNav categories={data?.categories} user={data?.user} />
		</aside>

		<div
			class="offcanvas offcanvas-start mobile-sidebar"
			tabindex="-1"
			id="frSidebar"
			aria-labelledby="frSidebarLabel"
		>
			<div class="offcanvas-header sidebar-header">
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

	<SiteFooter siteName={data?.siteName} siteSlogan={data?.siteSlogan} logoUrl={data?.logoUrl} />
	<BottomNav user={data?.user} />
</div>

<style>
	.mobile-sidebar {
		width: var(--fr-sidebar-width);
		background: var(--fr-sidebar-bg);
		border-right: 1px solid var(--fr-sidebar-border);
	}

	.sidebar-header {
		height: var(--fr-topbar-height);
		border-bottom: 1px solid var(--bs-border-color);
		padding: 0 1rem;
	}
</style>
