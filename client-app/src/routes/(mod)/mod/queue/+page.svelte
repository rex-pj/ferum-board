<svelte:head>
	<title>Approval Queue | Mod | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let actingId = $state<string | null>(null);

	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 20)));
	const currentPage = $derived(data.meta?.page ?? 1);
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-hourglass-half fr-page-icon"></i>
	<h1 class="h5 mb-0">Approval Queue</h1>
	{#if (data.meta?.total ?? 0) > 0}
		<span class="badge rounded-pill bg-warning text-dark ms-1">{data.meta.total}</span>
	{/if}
</div>

{#if form?.error}
	<div class="alert alert-danger">{form.error}</div>
{/if}
{#if form?.success}
	<div class="alert alert-success">{form.success}</div>
{/if}

<div class="card">
	<div class="table-responsive">
		<table class="table align-middle mb-0">
			<thead>
				<tr>
					<th>Author</th>
					<th>Thread</th>
					<th>Content preview</th>
					<th>Submitted</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each data.posts as post}
					<tr>
						<td class="small">
							<code>{(post.author_username ?? post.author_id).slice(0, 16)}</code>
						</td>
						<td class="small">
							<code>{post.thread_id.slice(0, 8)}…</code>
						</td>
						<td class="small content-col">
							<span class="text-truncate d-block text-muted" title={post.content_md}>
								{post.content_md.slice(0, 120)}{post.content_md.length > 120 ? '…' : ''}
							</span>
						</td>
						<td class="small"><Timestamp date={post.created_at} /></td>
						<td>
							<div class="d-flex gap-1">
								<form
									method="POST"
									action="?/approve"
									use:enhance={() => {
										actingId = post.id;
										return async ({ result, update }) => {
											actingId = null;
											if (result.type === 'success') toast.success('Post approved and published.');
											await update();
										};
									}}
								>
									<input type="hidden" name="id" value={post.id} />
									<button
										type="submit"
										class="btn btn-success btn-sm"
										disabled={actingId === post.id}
										title="Approve"
									>
										<i class="fa-solid fa-check"></i>
									</button>
								</form>
								<form
									method="POST"
									action="?/reject"
									use:enhance={() => {
										actingId = post.id;
										return async ({ result, update }) => {
											actingId = null;
											if (result.type === 'success') toast.success('Post rejected.');
											await update();
										};
									}}
								>
									<input type="hidden" name="id" value={post.id} />
									<button
										type="submit"
										class="btn btn-outline-danger btn-sm"
										disabled={actingId === post.id}
										title="Reject"
									>
										<i class="fa-solid fa-xmark"></i>
									</button>
								</form>
							</div>
						</td>
					</tr>
				{/each}
				{#if data.posts.length === 0}
					<tr>
						<td colspan="5" class="p-0">
							<div class="fr-empty-state">
								<div class="fr-empty-icon"><i class="fa-solid fa-inbox"></i></div>
								<h2 class="fr-empty-title">Queue is empty</h2>
								<p class="fr-empty-sub">No posts waiting for approval.</p>
							</div>
						</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>

<Pagination {currentPage} {totalPages} buildHref={(p) => `?page=${p}`} />

<style>
	.content-col { max-width: 320px; }
</style>
