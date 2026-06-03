<script lang="ts">
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	const navItems = [
		{ href: ROUTES.ADMIN.DASHBOARD, icon: 'fa-chart-line', label: 'Dashboard' },
		{ href: ROUTES.ADMIN.USERS, icon: 'fa-users', label: 'Users' },
		{ href: ROUTES.ADMIN.CATEGORIES, icon: 'fa-folder', label: 'Categories' },
		{ href: ROUTES.ADMIN.THREADS, icon: 'fa-comments', label: 'Threads' },
		{ href: ROUTES.ADMIN.ROLES, icon: 'fa-shield-halved', label: 'Roles' },
		{ href: ROUTES.ADMIN.PERMISSIONS, icon: 'fa-key', label: 'Permissions' },
		{ href: ROUTES.ADMIN.REPORTS, icon: 'fa-flag', label: 'Reports' },
		{ href: ROUTES.ADMIN.LOG, icon: 'fa-scroll', label: 'Audit Log' },
		{ href: ROUTES.ADMIN.SETTINGS, icon: 'fa-gear', label: 'Settings' }
	];
</script>

<nav class="fr-nav" aria-label="Administration">
	<div class="fr-nav-section">Administration</div>
	{#each navItems as item}
		<a
			href={item.href}
			class="fr-nav-link {$page.url.pathname.startsWith(item.href) ? 'active' : ''}"
			aria-current={$page.url.pathname.startsWith(item.href) ? 'page' : undefined}
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
