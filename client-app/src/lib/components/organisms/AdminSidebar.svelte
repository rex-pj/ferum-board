<script lang="ts">
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	const groups = [
		{
			label: 'Overview',
			items: [
				{ href: ROUTES.ADMIN.DASHBOARD, icon: 'fa-chart-line', label: 'Dashboard' }
			]
		},
		{
			label: 'Content',
			items: [
				{ href: ROUTES.ADMIN.CATEGORIES, icon: 'fa-folder', label: 'Categories' },
				{ href: ROUTES.ADMIN.THREADS, icon: 'fa-comments', label: 'Threads' }
			]
		},
		{
			label: 'Users & Access',
			items: [
				{ href: ROUTES.ADMIN.USERS, icon: 'fa-users', label: 'Users' },
				{ href: ROUTES.ADMIN.ROLES, icon: 'fa-shield-halved', label: 'Roles' },
				{ href: ROUTES.ADMIN.PERMISSIONS, icon: 'fa-key', label: 'Permissions' }
			]
		},
		{
			label: 'Moderation',
			items: [
				{ href: ROUTES.ADMIN.REPORTS, icon: 'fa-flag', label: 'Reports' },
				{ href: ROUTES.ADMIN.LOG, icon: 'fa-scroll', label: 'Audit Log' }
			]
		},
		{
			label: 'System',
			items: [
				{ href: ROUTES.ADMIN.SETTINGS, icon: 'fa-gear', label: 'Settings' },
				{ href: ROUTES.ADMIN.PLUGINS, icon: 'fa-puzzle-piece', label: 'Plugins' }
			]
		}
	];

	function isActive(href: string) {
		return $page.url.pathname.startsWith(href);
	}
</script>

<nav class="fr-nav" aria-label="Administration">
	{#each groups as group}
		<div class="fr-nav-section">{group.label}</div>
		{#each group.items as item}
			<a
				href={item.href}
				class="fr-nav-link {isActive(item.href) ? 'active' : ''}"
				aria-current={isActive(item.href) ? 'page' : undefined}
			>
				<i class="fa-solid {item.icon} fr-nav-icon"></i>
				{item.label}
			</a>
		{/each}
	{/each}
	<div class="fr-nav-divider"></div>
	<a href={ROUTES.HOME} class="fr-nav-link">
		<i class="fa-solid fa-arrow-left fr-nav-icon"></i>
		Back to Forum
	</a>
</nav>
