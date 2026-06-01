<svelte:head>
	<title>Audit Log | Mod | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';

	let { data }: { data: any } = $props();

	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 50)));
	const currentPage = $derived(data.meta?.page ?? 1);

	function pageHref(p: number) {
		const params = new URLSearchParams();
		params.set('page', String(p));
		if (data.actor_id) params.set('actor_id', data.actor_id);
		if (data.target_type) params.set('target_type', data.target_type);
		return `?${params}`;
	}
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-scroll" style="font-size:1.1rem;opacity:.55;"></i>
	<h1 class="h5 mb-0">Audit Log</h1>
</div>

<div class="card">
	<div class="card-header py-2 d-flex align-items-center gap-2 flex-wrap">
		<form method="GET" class="d-flex align-items-center gap-2 flex-grow-1 flex-wrap">
			<div class="input-group input-group-sm" style="max-width:260px;">
				<span class="input-group-text"><i class="fa-solid fa-user"></i></span>
				<input
					type="search"
					name="actor_id"
					class="form-control"
					placeholder="Actor ID…"
					value={data.actor_id}
				/>
			</div>
			<select name="target_type" class="form-select form-select-sm" style="width:auto;">
				<option value="" selected={!data.target_type}>All targets</option>
				<option value="Thread" selected={data.target_type === 'Thread'}>Thread</option>
				<option value="Post" selected={data.target_type === 'Post'}>Post</option>
				<option value="User" selected={data.target_type === 'User'}>User</option>
				<option value="Report" selected={data.target_type === 'Report'}>Report</option>
			</select>
			<button type="submit" class="btn btn-sm btn-outline-secondary">
				<i class="fa-solid fa-filter me-1"></i>Filter
			</button>
			{#if data.actor_id || data.target_type}
				<a href="?" class="btn btn-sm btn-link text-muted p-0">
					<i class="fa-solid fa-xmark me-1"></i>Clear
				</a>
			{/if}
		</form>
		{#if data.meta?.total !== undefined}
			<span class="badge rounded-pill bg-secondary">{data.meta.total}</span>
		{/if}
	</div>
	{#if data.logs?.length > 0}
		<div class="table-responsive">
			<table class="table table-sm align-middle small mb-0">
				<thead>
					<tr>
						<th>Actor</th>
						<th>Action</th>
						<th>Target</th>
						<th>When</th>
					</tr>
				</thead>
				<tbody>
					{#each data.logs as log}
						<tr>
							<td><code>{log.actor_id?.slice(0, 8) ?? '—'}…</code></td>
							<td><span class="badge bg-secondary">{log.action}</span></td>
							<td class="text-muted">{log.target_type} <code>{log.target_id.slice(0, 8)}…</code></td>
							<td><Timestamp date={log.created_at} /></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else}
		<div class="card-body p-0">
			<div class="fr-empty-state">
				<div class="fr-empty-icon">
					<i class="fa-solid fa-scroll"></i>
				</div>
				<h2 class="fr-empty-title">No log entries</h2>
				<p class="fr-empty-sub">Moderation actions will be recorded here as they happen.</p>
			</div>
		</div>
	{/if}
</div>

<Pagination {currentPage} {totalPages} buildHref={pageHref} />
