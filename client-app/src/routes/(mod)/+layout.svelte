<script lang="ts">
	import '../../app.css';
	import { page } from '$app/stores';
	import SiteHeader from '$lib/components/organisms/SiteHeader.svelte';
	import SiteFooter from '$lib/components/organisms/SiteFooter.svelte';
	import { ROUTES } from '$lib/routes';

	let { data, children }: { data: any; children: any } = $props();

	const modNav = [
		{ href: ROUTES.MOD.REPORTS, icon: 'fa-flag', label: 'Reports' },
		{ href: ROUTES.MOD.QUEUE, icon: 'fa-hourglass-half', label: 'Approval Queue' },
		{ href: ROUTES.MOD.THREADS, icon: 'fa-comments', label: 'Threads' },
		{ href: ROUTES.MOD.LOG, icon: 'fa-scroll', label: 'Audit Log' }
	];
</script>

<div class="fr-app">
	<SiteHeader user={data?.user} siteName={data?.siteName} siteSlogan={data?.siteSlogan} logoUrl={data?.logoUrl} />

	<div class="fr-body">
		<!-- Left rail: mod nav -->
		<aside class="fr-sidebar d-none d-lg-block">
			<nav class="fr-nav" aria-label="Moderation">
				<div class="fr-nav-section">Moderation</div>
				{#each modNav as item}
					<a
						href={item.href}
						class="fr-nav-link {$page.url.pathname === item.href ? 'active' : ''}"
						aria-current={$page.url.pathname === item.href ? 'page' : undefined}
					>
						<i class="fa-solid {item.icon} fr-nav-icon"></i>
						{item.label}
					</a>
				{/each}
				<div class="fr-nav-divider"></div>
				<a href={ROUTES.HOME} class="fr-nav-link">
					<i class="fa-solid fa-arrow-left fr-nav-icon"></i>
					Back to Forum
				</a>
			</nav>
		</aside>

		<main class="fr-main">
			<div class="px-4 py-4">
				{@render children()}
			</div>
		</main>
	</div>

	<SiteFooter siteName={data?.siteName} siteSlogan={data?.siteSlogan} logoUrl={data?.logoUrl} compact />
</div>
