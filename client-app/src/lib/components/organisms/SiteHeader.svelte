<script lang="ts">
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import { ROUTES } from '$lib/routes';
	import { theme } from '$lib/stores/theme';
	import { unreadCount, startSSE, stopNotifications } from '$lib/stores/notifications';
	import { onMount, onDestroy } from 'svelte';

	import { isAdmin, isModerator, type UserWithRoles } from '$lib/utils/permissions';

	interface Props {
		user?: UserWithRoles & {
			username: string;
			display_name?: string | null;
			avatar_url?: string | null;
		} | null;
		siteName?: string;
		logoUrl?: string | null;
		categories?: unknown[];
	}

	let { user = null, siteName = 'Ferum Board', logoUrl = null }: Props = $props();

	let systemDark = $state(false);

	onMount(() => {
		const mq = window.matchMedia('(prefers-color-scheme: dark)');
		systemDark = mq.matches;
		const onChange = (e: MediaQueryListEvent) => { systemDark = e.matches; };
		mq.addEventListener('change', onChange);
		if (user) startSSE();
		return () => mq.removeEventListener('change', onChange);
	});
	onDestroy(() => stopNotifications());

	// Effective theme là những gì người dùng đang nhìn thấy thực tế
	const effectiveIsDark = $derived(
		$theme === 'dark' || ($theme === 'auto' && systemDark)
	);

	function toggleTheme() {
		// Luôn toggle giữa dark ↔ light dựa trên effective theme,
		// không đi qua bước 'auto' giữa chừng gây nhầm lẫn
		theme.update((t) => {
			if (t === 'dark') return 'light';
			if (t === 'light') return 'dark';
			// t === 'auto': resolve effective rồi toggle sang ngược lại
			return systemDark ? 'light' : 'dark';
		});
	}
</script>

<header class="fr-topbar">
	<!-- Hamburger: mobile only, toggles offcanvas left rail -->
	<button
		class="fr-icon-btn d-lg-none"
		type="button"
		data-bs-toggle="offcanvas"
		data-bs-target="#frSidebar"
		aria-controls="frSidebar"
		aria-label="Open navigation"
	>
		<i class="fa-solid fa-bars"></i>
	</button>

	<!-- Brand -->
	<a href={ROUTES.HOME} class="fr-topbar-brand">
		{#if logoUrl}
			<img src={logoUrl} alt={siteName} class="brand-logo" />
		{:else}
			<i class="fa-solid fa-feather-pointed text-primary"></i>
		{/if}
		{siteName}
	</a>

	<!-- Search bar: desktop -->
	<div class="fr-topbar-search d-none d-md-block">
		<form action={ROUTES.SEARCH} method="GET">
			<div class="fr-search-wrap">
				<i class="fa-solid fa-magnifying-glass fr-search-icon"></i>
				<input
					type="search"
					name="q"
					class="fr-search-input"
					placeholder="Search discussions…"
					autocomplete="off"
				/>
			</div>
		</form>
	</div>

	<!-- Right actions -->
	<div class="fr-topbar-actions">
		<!-- Search icon: mobile only -->
		<a href={ROUTES.SEARCH} class="fr-icon-btn d-md-none" aria-label="Search">
			<i class="fa-solid fa-magnifying-glass"></i>
		</a>

		<!-- Theme toggle: sun = đang dark (click để sang light), moon = đang light (click để sang dark) -->
		<button class="fr-icon-btn" type="button" onclick={toggleTheme} aria-label="Toggle theme">
			{#if effectiveIsDark}
				<i class="fa-solid fa-sun"></i>
			{:else}
				<i class="fa-solid fa-moon"></i>
			{/if}
		</button>

		{#if user}
			<!-- Notifications -->
			<a href={ROUTES.NOTIFICATIONS} class="fr-icon-btn position-relative" aria-label="Notifications">
				<i class="fa-solid fa-bell"></i>
				{#if $unreadCount > 0}
					<span class="position-absolute badge rounded-pill bg-danger notif-badge">
						{$unreadCount > 99 ? '99+' : $unreadCount}
					</span>
				{/if}
			</a>

			<!-- New Thread: desktop -->
			<a href="/new-thread" class="btn btn-primary btn-sm gap-1 d-none d-sm-inline-flex">
				<i class="fa-solid fa-plus"></i>
				New Thread
			</a>

			<!-- User avatar dropdown -->
			<div class="dropdown">
				<button
					class="btn p-0 border-0 bg-transparent d-flex align-items-center avatar-btn"
					type="button"
					data-bs-toggle="dropdown"
					aria-expanded="false"
					aria-label="User menu"
				>
					<Avatar src={user.avatar_url} username={user.username} size={32} />
				</button>
				<ul class="dropdown-menu dropdown-menu-end user-menu">
					<li>
						<a class="dropdown-item" href={ROUTES.USER_PROFILE(user.username)}>
							<i class="fa-regular fa-user me-2 fr-dd-icon"></i>
							{user.display_name ?? user.username}
						</a>
					</li>
					<li>
						<a class="dropdown-item" href={ROUTES.ACCOUNT}>
							<i class="fa-solid fa-gear me-2 fr-dd-icon"></i>
							Settings
						</a>
					</li>
					{#if isModerator(user)}
						<li><hr class="dropdown-divider my-1" /></li>
						<li>
							<a class="dropdown-item" href={ROUTES.MOD.REPORTS}>
								<i class="fa-solid fa-shield-halved me-2 fr-dd-icon"></i>
								Mod Panel
							</a>
						</li>
					{/if}
					{#if isAdmin(user)}
						<li>
							<a class="dropdown-item" href={ROUTES.ADMIN.DASHBOARD}>
								<i class="fa-solid fa-chart-line me-2 fr-dd-icon"></i>
								Admin
							</a>
						</li>
					{/if}
					<li><hr class="dropdown-divider my-1" /></li>
					<li>
						<form method="POST" action={ROUTES.LOGOUT}>
							<button class="dropdown-item text-danger w-100" type="submit">
								<i class="fa-solid fa-right-from-bracket me-2 fr-dd-icon"></i>
								Sign out
							</button>
						</form>
					</li>
				</ul>
			</div>
		{:else}
			<a class="btn btn-sm d-none d-sm-inline-flex fr-btn-ghost" href={ROUTES.LOGIN}>
				Sign in
			</a>
			<a class="btn btn-primary btn-sm" href={ROUTES.REGISTER}>Get started</a>
		{/if}
	</div>
</header>

<style>
	.brand-logo {
		height: 24px;
		width: auto;
		object-fit: contain;
		flex-shrink: 0;
	}

	.notif-badge {
		font-size: 0.55rem;
		top: 7px;
		right: 5px;
		padding: 2px 4px;
		min-width: 0;
		line-height: 1.2;
	}

	.avatar-btn {
		min-height: var(--fr-tap-target);
		width: var(--fr-tap-target);
	}

	.user-menu {
		min-width: 192px;
	}
</style>
