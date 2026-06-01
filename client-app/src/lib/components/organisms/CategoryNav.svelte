<script lang="ts">
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	interface Category {
		id: string;
		name: string;
		slug: string;
		color?: string | null;
		parent_id?: string | null;
	}

	interface Props {
		categories?: Category[];
		user?: { username: string; role: string } | null;
	}

	let { categories = [], user = null }: Props = $props();

	const topLevel = $derived(categories.filter((c) => !c.parent_id));
	const subOf = (parentId: string) => categories.filter((c) => c.parent_id === parentId);

	function isActive(path: string, exact = false) {
		const pathname = $page.url.pathname;
		return exact ? pathname === path : pathname === path || pathname.startsWith(path + '/');
	}
</script>

<nav class="fr-nav" aria-label="Main navigation">
	<!-- Home -->
	<a href={ROUTES.HOME} class="fr-nav-link {isActive('/', true) ? 'active' : ''}">
		<i class="fa-solid fa-house fr-nav-icon"></i>
		Home
	</a>

	<!-- New Thread: visible in mobile rail only (hidden on desktop — it's in the top bar) -->
	{#if user}
		<a href="/new-thread" class="btn btn-primary btn-sm w-100 justify-content-center mt-1 mb-1 d-lg-none">
			<i class="fa-solid fa-plus me-1"></i>New Thread
		</a>
	{/if}

	<!-- Forum index -->
	<a href={ROUTES.FORUM_INDEX} class="fr-nav-link {isActive(ROUTES.FORUM_INDEX, true) ? 'active' : ''}">
		<i class="fa-solid fa-layer-group fr-nav-icon"></i>
		Forum
	</a>

	<div class="fr-nav-divider"></div>

	<!-- Categories -->
	{#if topLevel.length > 0}
		<div class="fr-nav-section">Categories</div>

		{#each topLevel as cat}
			<a
				href={ROUTES.CATEGORY(cat.slug)}
				class="fr-nav-link {isActive(ROUTES.CATEGORY(cat.slug)) ? 'active' : ''}"
			>
				{#if cat.color}
					<span class="fr-nav-dot" style="background: {cat.color};"></span>
				{:else}
					<i class="fa-solid fa-hashtag fr-nav-icon"></i>
				{/if}
				{cat.name}
			</a>

			{#each subOf(cat.id) as sub}
				<a
					href={ROUTES.CATEGORY(sub.slug)}
					class="fr-nav-link fr-nav-sub {isActive(ROUTES.CATEGORY(sub.slug)) ? 'active' : ''}"
				>
					<span
						class="fr-nav-dot"
						style="background: {sub.color ?? 'var(--bs-tertiary-color)'};"
					></span>
					{sub.name}
				</a>
			{/each}
		{/each}
	{/if}

	<div class="fr-nav-divider"></div>

	<!-- Search -->
	<a href={ROUTES.SEARCH} class="fr-nav-link {isActive(ROUTES.SEARCH) ? 'active' : ''}">
		<i class="fa-solid fa-magnifying-glass fr-nav-icon"></i>
		Search
	</a>
</nav>
