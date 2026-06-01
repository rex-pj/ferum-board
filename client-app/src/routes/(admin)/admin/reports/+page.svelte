<svelte:head>
	<title>Reports | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();

	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 20)));
	const currentPage = $derived(data.meta?.page ?? 1);

	function pageHref(p: number) {
		const params = new URLSearchParams();
		params.set('page', String(p));
		if (data.status) params.set('status', data.status);
		if (data.target_type) params.set('target_type', data.target_type);
		return `?${params}`;
	}

	const STATUS_COLOR: Record<string, string> = {
		pending: 'warning',
		resolved: 'success',
		dismissed: 'secondary'
	};

	let resolvingId = $state<string | null>(null);
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-flag" style="font-size:1.1rem;opacity:.55;"></i>
	<h1 class="h5 mb-0">Reports</h1>
	{#if (data.meta?.total ?? 0) > 0}
		<span class="badge rounded-pill bg-danger ms-1">{data.meta.total}</span>
	{/if}
</div>

{#if form?.error}
	<div class="alert alert-danger" role="alert">{form.error}</div>
{/if}

<div class="card">
	<div class="card-header py-2 d-flex align-items-center gap-2 flex-wrap">
		<form method="GET" class="d-flex align-items-center gap-2 flex-grow-1 flex-wrap">
			<select name="status" class="form-select form-select-sm" style="width:auto;">
				<option value="" selected={!data.status}>All statuses</option>
				<option value="pending" selected={data.status === 'pending'}>Pending</option>
				<option value="resolved" selected={data.status === 'resolved'}>Resolved</option>
				<option value="dismissed" selected={data.status === 'dismissed'}>Dismissed</option>
			</select>
			<select name="target_type" class="form-select form-select-sm" style="width:auto;">
				<option value="" selected={!data.target_type}>All targets</option>
				<option value="post" selected={data.target_type === 'post'}>Posts only</option>
				<option value="thread" selected={data.target_type === 'thread'}>Threads only</option>
			</select>
			<button type="submit" class="btn btn-sm btn-outline-secondary">
				<i class="fa-solid fa-filter me-1"></i>Filter
			</button>
			{#if data.status || data.target_type}
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
					<th>Status</th>
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
						<td>
							<span class="badge bg-{STATUS_COLOR[report.status] ?? 'secondary'}">{report.status}</span>
						</td>
						<td class="small">
							{#if report.thread_id}
								<span class="badge bg-secondary">Thread</span>
								<code class="ms-1">{report.thread_id.slice(0, 8)}…</code>
							{:else if report.post_id}
								<span class="badge bg-secondary">Post</span>
								<code class="ms-1">{report.post_id.slice(0, 8)}…</code>
							{/if}
						</td>
						<td class="small" style="max-width:300px;">
							<span class="text-truncate d-block" title={report.reason}>{report.reason}</span>
						</td>
						<td class="small"><code>{report.reporter_id.slice(0, 8)}…</code></td>
						<td class="small"><Timestamp date={report.created_at} /></td>
						<td>
							{#if report.status === 'pending'}
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
							{/if}
						</td>
					</tr>
				{/each}
				{#if data.reports.length === 0}
					<tr>
						<td colspan="6" class="p-0">
							<div class="fr-empty-state">
								<div class="fr-empty-icon"><i class="fa-solid fa-flag"></i></div>
								<h2 class="fr-empty-title">No reports</h2>
								<p class="fr-empty-sub">No reports match the current filter.</p>
							</div>
						</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>

<Pagination {currentPage} {totalPages} buildHref={pageHref} />
