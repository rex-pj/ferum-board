<svelte:head>
	<title>Report Queue | Mod | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let resolvingId = $state<string | null>(null);

	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 30)));
	const currentPage = $derived(data.meta?.page ?? 1);

	function pageHref(p: number) {
		const params = new URLSearchParams();
		params.set('page', String(p));
		if (data.target_type) params.set('target_type', data.target_type);
		return `?${params}`;
	}
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-flag fr-page-icon"></i>
	<h1 class="h5 mb-0">Pending Reports</h1>
	{#if (data.meta?.total ?? 0) > 0}
		<span class="badge rounded-pill bg-danger ms-1">{data.meta.total}</span>
	{/if}
</div>

{#if form?.error}
	<div class="alert alert-danger">{form.error}</div>
{/if}

<div class="card">
	<div class="card-header py-2 d-flex align-items-center gap-2 flex-wrap">
		<form method="GET" class="d-flex align-items-center gap-2 flex-grow-1 flex-wrap">
			<select name="target_type" class="form-select form-select-sm w-auto">
				<option value="" selected={!data.target_type}>All targets</option>
				<option value="post" selected={data.target_type === 'post'}>Posts only</option>
				<option value="thread" selected={data.target_type === 'thread'}>Threads only</option>
			</select>
			<button type="submit" class="btn btn-sm btn-outline-secondary">
				<i class="fa-solid fa-filter me-1"></i>Filter
			</button>
			{#if data.target_type}
				<a href="?" class="btn btn-sm btn-link text-muted p-0">
					<i class="fa-solid fa-xmark me-1"></i>Clear
				</a>
			{/if}
		</form>
	</div>
	<div class="table-responsive">
		<table class="table align-middle mb-0">
			<thead>
				<tr>
					<th>Target</th>
					<th>Reason</th>
					<th>Reporter</th>
					<th>Date</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each data.reports as report}
					<tr>
						<td class="small">
							{#if report.post_id}
								<span class="badge bg-secondary">Post</span>
								<code class="ms-1">{report.post_id.slice(0, 8)}…</code>
							{:else if report.thread_id}
								<span class="badge bg-secondary">Thread</span>
								<code class="ms-1">{report.thread_id.slice(0, 8)}…</code>
							{:else}
								<span class="badge bg-danger-subtle text-danger-emphasis">Deleted</span>
								{#if report.target_deleted_at}
									<span class="text-muted ms-1" title="Content was deleted">
										<i class="fa-solid fa-trash-can fa-xs"></i>
									</span>
								{/if}
							{/if}
						</td>
						<td class="small reason-col">
							<span class="text-truncate d-block" title={report.reason}>{report.reason}</span>
						</td>
						<td class="small"><code>{report.reporter_id.slice(0, 8)}…</code></td>
						<td class="small"><Timestamp date={report.created_at} /></td>
						<td>
							<div class="d-flex gap-1">
								<form
									method="POST"
									action="?/resolve"
									use:enhance={({ formData }) => {
										resolvingId = report.id;
										const status = formData.get('status') as string;
										return async ({ result, update }) => {
											resolvingId = null;
											if (result.type === 'success') {
												toast.success(status === 'resolved' ? 'Report resolved.' : 'Report dismissed.');
											}
											await update();
										};
									}}
								>
									<input type="hidden" name="id" value={report.id} />
									<input type="hidden" name="status" value="resolved" />
									<button
										type="submit"
										class="btn btn-success btn-sm"
										disabled={resolvingId === report.id}
										title="Resolve"
									>
										<i class="fa-solid fa-check"></i>
									</button>
								</form>
								<form
									method="POST"
									action="?/resolve"
									use:enhance={({ formData }) => {
										resolvingId = report.id;
										const status = formData.get('status') as string;
										return async ({ result, update }) => {
											resolvingId = null;
											if (result.type === 'success') {
												toast.success(status === 'resolved' ? 'Report resolved.' : 'Report dismissed.');
											}
											await update();
										};
									}}
								>
									<input type="hidden" name="id" value={report.id} />
									<input type="hidden" name="status" value="dismissed" />
									<button
										type="submit"
										class="btn btn-outline-secondary btn-sm"
										disabled={resolvingId === report.id}
										title="Dismiss"
									>
										<i class="fa-solid fa-xmark"></i>
									</button>
								</form>
							</div>
						</td>
					</tr>
				{/each}
				{#if data.reports.length === 0}
					<tr>
						<td colspan="5" class="p-0">
							<div class="fr-empty-state">
								<div class="fr-empty-icon"><i class="fa-solid fa-inbox"></i></div>
								<h2 class="fr-empty-title">All clear!</h2>
								<p class="fr-empty-sub">No pending reports to review. Great work.</p>
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
	.reason-col { max-width: 300px; }
</style>