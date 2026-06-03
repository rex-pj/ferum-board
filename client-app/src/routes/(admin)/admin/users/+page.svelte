<svelte:head>
	<title>Users | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { ROUTES } from '$lib/routes';
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';

	let { data }: { data: any } = $props();

	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 20)));
	const currentPage = $derived(data.meta?.page ?? 1);

	function pageHref(p: number) {
		const params = new URLSearchParams();
		params.set('page', String(p));
		if (data.q) params.set('q', data.q);
		return `?${params}`;
	}
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-users fr-page-icon"></i>
	<h1 class="h5 mb-0">Users</h1>
</div>

<div class="card">
	<div class="card-header py-2 d-flex align-items-center gap-2">
		<form method="GET" class="d-flex align-items-center gap-2 flex-grow-1">
			<div class="input-group input-group-sm filter-search">
				<input
					type="search"
					name="q"
					class="form-control"
					placeholder="Search username or email…"
					value={data.q}
				/>
				<button type="submit" class="btn btn-outline-secondary" aria-label="Search">
					<i class="fa-solid fa-magnifying-glass"></i>
				</button>
			</div>
		</form>
		<span class="badge rounded-pill bg-secondary">{data.meta?.total ?? 0}</span>
	</div>
	<div class="table-responsive">
		<table class="table table-hover align-middle mb-0">
		<thead>
			<tr>
				<th>User</th>
				<th>Role</th>
				<th>Trust</th>
				<th>Status</th>
				<th>Joined</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each data.users as user}
				<tr>
					<td>
						<div class="d-flex align-items-center gap-2">
							<Avatar src={user.avatar_url} username={user.username} size={32} />
							<div>
								<div class="fw-semibold">{user.username}</div>
								{#if user.display_name}
									<div class="text-muted small">{user.display_name}</div>
								{/if}
							</div>
						</div>
					</td>
					<td>{#if user.primary_role_slug}<Badge role={user.primary_role_slug} />{/if}</td>
					<td><Badge trust={user.trust_level} /></td>
					<td>
						{#if user.is_banned}
							<span class="badge bg-danger">banned</span>
						{:else}
							<span class="badge bg-success">active</span>
						{/if}
					</td>
					<td><Timestamp date={user.created_at} /></td>
					<td>
						<a href={ROUTES.ADMIN.USER(user.id)} class="btn btn-outline-secondary btn-sm" title="Edit user">
							<i class="fa-solid fa-pen"></i>
						</a>
					</td>
				</tr>
			{/each}
			{#if data.users.length === 0}
				<tr>
					<td colspan="6" class="p-0">
						<div class="fr-empty-state">
							<div class="fr-empty-icon">
								<i class="fa-solid fa-users"></i>
							</div>
							<h2 class="fr-empty-title">No users found</h2>
							<p class="fr-empty-sub">Try a different search term.</p>
						</div>
					</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
</div>

<Pagination {currentPage} {totalPages} buildHref={pageHref} />

<style>
	.filter-search { max-width: 360px; }
</style>