<svelte:head>
	<title>Dashboard | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { ROUTES } from '$lib/routes';

	let { data }: { data: any } = $props();
	const s = data.stats;
</script>

<h1 class="h4 mb-4">Dashboard</h1>

<div class="row g-3 mb-4">
	{#each [
		{ label: 'Total Users', value: s?.users_total ?? 0, icon: 'fa-users', color: 'primary', href: ROUTES.ADMIN.USERS },
		{ label: 'Total Threads', value: s?.threads_total ?? 0, icon: 'fa-comments', color: 'success', href: ROUTES.ADMIN.CATEGORIES },
		{ label: 'Total Posts', value: s?.posts_total ?? 0, icon: 'fa-message', color: 'info', href: null },
		{ label: 'Pending Reports', value: s?.reports_pending ?? 0, icon: 'fa-flag', color: 'danger', href: ROUTES.ADMIN.REPORTS },
	] as stat}
		<div class="col-sm-6 col-xl-3">
			<div class="card h-100">
				<div class="card-body d-flex align-items-center gap-3">
					<div class="rounded-circle d-flex align-items-center justify-content-center bg-{stat.color} bg-opacity-10 flex-shrink-0" style="width:48px;height:48px;">
						<i class="fa-solid {stat.icon} text-{stat.color}"></i>
					</div>
					<div>
						<div class="h4 mb-0 fw-bold">{stat.value.toLocaleString()}</div>
						<div class="text-muted small">{stat.label}</div>
					</div>
				</div>
				{#if stat.href}
					<a href={stat.href} class="stretched-link" aria-label={stat.label}></a>
				{/if}
			</div>
		</div>
	{/each}
</div>

<div class="row g-3">
	<div class="col-md-6">
		<div class="card">
			<div class="card-header">Today</div>
			<div class="card-body">
				<div class="d-flex justify-content-between py-2 border-bottom">
					<span>New users</span>
					<strong>{s?.new_users_today ?? 0}</strong>
				</div>
				<div class="d-flex justify-content-between py-2">
					<span>New threads</span>
					<strong>{s?.new_threads_today ?? 0}</strong>
				</div>
			</div>
		</div>
	</div>
	<div class="col-md-6">
		<div class="card">
			<div class="card-header">Quick Actions</div>
			<div class="list-group list-group-flush">
				<a href={ROUTES.ADMIN.CATEGORIES} class="list-group-item list-group-item-action">
					<i class="fa-solid fa-folder me-2"></i>Manage Categories
				</a>
				<a href={ROUTES.ADMIN.USERS} class="list-group-item list-group-item-action">
					<i class="fa-solid fa-users me-2"></i>Manage Users
				</a>
				<a href={ROUTES.ADMIN.REPORTS} class="list-group-item list-group-item-action">
					<i class="fa-solid fa-flag me-2"></i>Review Reports
					{#if s?.reports_pending > 0}
						<span class="badge bg-danger float-end">{s.reports_pending}</span>
					{/if}
				</a>
				<a href={ROUTES.ADMIN.SETTINGS} class="list-group-item list-group-item-action">
					<i class="fa-solid fa-gear me-2"></i>Site Settings
				</a>
			</div>
		</div>
	</div>
</div>
