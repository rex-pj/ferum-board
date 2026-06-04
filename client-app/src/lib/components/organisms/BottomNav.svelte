<script lang="ts">
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	interface Props {
		user?: {
			username: string;
			display_name?: string | null;
			avatar_url?: string | null;
		} | null;
	}

	let { user = null }: Props = $props();

	function isActive(path: string, exact = false) {
		const pathname = $page.url.pathname;
		return exact ? pathname === path : pathname === path || pathname.startsWith(path + '/');
	}
</script>

<nav class="fr-bottom-nav d-lg-none" aria-label="Mobile navigation">
	<a
		href={ROUTES.HOME}
		class="fr-bn-item"
		class:active={isActive('/', true)}
		aria-label="Home"
		aria-current={isActive('/', true) ? 'page' : undefined}
	>
		<i class="fa-solid fa-house fr-bn-icon"></i>
		<span class="fr-bn-label">Home</span>
	</a>

	<a
		href={ROUTES.FORUM_INDEX}
		class="fr-bn-item"
		class:active={isActive(ROUTES.FORUM_INDEX, true)}
		aria-label="Forum"
		aria-current={isActive(ROUTES.FORUM_INDEX, true) ? 'page' : undefined}
	>
		<i class="fa-solid fa-layer-group fr-bn-icon"></i>
		<span class="fr-bn-label">Forum</span>
	</a>

	{#if user}
		<a href={ROUTES.NEW_THREAD} class="fr-bn-item fr-bn-cta" aria-label="New Thread">
			<span class="fr-bn-cta-pill">
				<i class="fa-solid fa-plus"></i>
			</span>
			<span class="fr-bn-label">New</span>
		</a>

		<a
			href={ROUTES.SEARCH}
			class="fr-bn-item"
			class:active={isActive(ROUTES.SEARCH)}
			aria-label="Search"
			aria-current={isActive(ROUTES.SEARCH) ? 'page' : undefined}
		>
			<i class="fa-solid fa-magnifying-glass fr-bn-icon"></i>
			<span class="fr-bn-label">Search</span>
		</a>

		<a
			href={ROUTES.BOOKMARKS}
			class="fr-bn-item"
			class:active={isActive(ROUTES.BOOKMARKS)}
			aria-label="Bookmarks"
			aria-current={isActive(ROUTES.BOOKMARKS) ? 'page' : undefined}
		>
			<i class="fa-solid fa-bookmark fr-bn-icon"></i>
			<span class="fr-bn-label">Bookmarks</span>
		</a>
	{:else}
		<a
			href={ROUTES.SEARCH}
			class="fr-bn-item"
			class:active={isActive(ROUTES.SEARCH)}
			aria-label="Search"
			aria-current={isActive(ROUTES.SEARCH) ? 'page' : undefined}
		>
			<i class="fa-solid fa-magnifying-glass fr-bn-icon"></i>
			<span class="fr-bn-label">Search</span>
		</a>

		<a
			href={ROUTES.LOGIN}
			class="fr-bn-item"
			class:active={isActive(ROUTES.LOGIN)}
			aria-label="Sign in"
		>
			<i class="fa-solid fa-right-to-bracket fr-bn-icon"></i>
			<span class="fr-bn-label">Sign in</span>
		</a>

		<a
			href={ROUTES.REGISTER}
			class="fr-bn-item"
			class:active={isActive(ROUTES.REGISTER)}
			aria-label="Register"
		>
			<i class="fa-solid fa-user-plus fr-bn-icon"></i>
			<span class="fr-bn-label">Register</span>
		</a>
	{/if}
</nav>
