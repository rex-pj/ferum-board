<script lang="ts">
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';
	import { isAdmin, type UserWithRoles } from '$lib/utils/permissions';

	interface Category {
		id: string;
		name: string;
		slug: string;
		color?: string | null;
		parent_id?: string | null;
	}

	interface Props {
		categories?: Category[];
		user?: UserWithRoles | null;
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

	<!-- Forum index -->
	<a href={ROUTES.FORUM_INDEX} class="fr-nav-link {isActive(ROUTES.FORUM_INDEX, true) ? 'active' : ''}">
		<i class="fa-solid fa-layer-group fr-nav-icon"></i>
		Forum
	</a>

	<div class="fr-nav-divider"></div>

	<!-- Categories -->
	<div class="fr-nav-section">Categories</div>

	{#if topLevel.length > 0}
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
	{:else}
		<div class="fr-nav-empty">
			<div class="fr-nav-empty-icon-wrap">
				<i class="fa-solid fa-folder-open"></i>
			</div>
			<p class="fr-nav-empty-text">No categories yet</p>
			{#if isAdmin(user)}
				<a href={ROUTES.ADMIN.CATEGORIES} class="fr-nav-empty-action">
					Set up categories
					<i class="fa-solid fa-arrow-right"></i>
				</a>
			{/if}
		</div>
	{/if}
</nav>

<style>
	.fr-nav-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		padding: 1rem 0.75rem 0.875rem;
		gap: 0.3125rem;
	}

	.fr-nav-empty-icon-wrap {
		width: 2rem;
		height: 2rem;
		border-radius: 50%;
		background: var(--bs-tertiary-bg);
		border: 1px solid var(--bs-border-color);
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.75rem;
		color: var(--bs-tertiary-color);
		margin-bottom: 0.125rem;
	}

	.fr-nav-empty-text {
		font-size: 0.75rem;
		color: var(--bs-tertiary-color);
		line-height: 1.4;
		margin: 0;
	}

	.fr-nav-empty-action {
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--bs-primary);
		text-decoration: none;
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		margin-top: 0.125rem;
		transition: opacity 0.1s;
	}

	.fr-nav-empty-action:hover {
		opacity: 0.75;
		text-decoration: underline;
	}

	.fr-nav-empty-action i {
		font-size: 0.625rem;
	}
</style>
