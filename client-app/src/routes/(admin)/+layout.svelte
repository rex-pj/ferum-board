<script lang="ts">
	import '../../app.css';
	import SiteHeader from '$lib/components/organisms/SiteHeader.svelte';
	import AdminSidebar from '$lib/components/organisms/AdminSidebar.svelte';
	import SiteFooter from '$lib/components/organisms/SiteFooter.svelte';

	let { data, children }: { data: any; children: any } = $props();

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
	<SiteHeader user={data?.user} siteName={data?.siteName} siteSlogan={data?.siteSlogan} logoUrl={data?.logoUrl} />

	<div class="fr-body">
		<aside class="fr-sidebar d-none d-lg-block">
			<AdminSidebar />
		</aside>

		<main class="fr-main">
			<div class="px-4 py-4">
				{@render children()}
			</div>
		</main>
	</div>

	<SiteFooter siteName={data?.siteName} siteSlogan={data?.siteSlogan} logoUrl={data?.logoUrl} compact />
</div>
