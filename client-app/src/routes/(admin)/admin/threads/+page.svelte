<svelte:head>
	<title>Threads | Admin | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();

	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 30)));
	const currentPage = $derived(data.meta?.page ?? 1);

	let movingThread = $state<{ id: string; title: string; currentCategoryId: string } | null>(null);
	let selectedCategoryId = $state('');
	let actingId = $state<string | null>(null);

	function pageHref(p: number) {
		const params = new URLSearchParams();
		params.set('page', String(p));
		if (data.category_slug) params.set('category_slug', data.category_slug);
		return `?${params}`;
	}

	function openMoveModal(thread: { id: string; title: string; category_id?: string }) {
		movingThread = { id: thread.id, title: thread.title, currentCategoryId: thread.category_id ?? '' };
		selectedCategoryId = thread.category_id ?? '';
	}

	$effect(() => {
		if (form?.success) {
			movingThread = null;
			actingId = null;
		}
	});
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-comments fr-page-icon"></i>
	<h1 class="h5 mb-0">Threads</h1>
</div>

{#if form?.error}
	<div class="alert alert-danger">{form.error}</div>
{/if}

<div class="card">
	<div class="card-header py-2 d-flex align-items-center gap-2 flex-wrap">
		<form method="GET" class="d-flex align-items-center gap-2 flex-grow-1 flex-wrap">
			<select name="category_slug" class="form-select form-select-sm cat-select">
				<option value="" selected={!data.category_slug}>All categories</option>
				{#each data.categories as cat}
					<option value={cat.slug} selected={data.category_slug === cat.slug}>{cat.name}</option>
				{/each}
			</select>
			<button type="submit" class="btn btn-sm btn-outline-secondary">
				<i class="fa-solid fa-filter me-1"></i>Filter
			</button>
		</form>
		<span class="badge rounded-pill bg-secondary">{data.meta?.total ?? 0}</span>
	</div>
	<div class="table-responsive">
		<table class="table table-hover align-middle mb-0">
		<thead>
			<tr>
				<th>Title</th>
				<th>Category</th>
				<th>Author</th>
				<th>Status</th>
				<th>Replies</th>
				<th>Created</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each data.threads as thread}
				<tr>
					<td class="title-col">
						<a
							href={ROUTES.THREAD(thread.slug)}
							class="fw-semibold text-truncate d-block text-decoration-none text-body title-link"
							title={thread.title}
						>
							{#if thread.is_pinned}
								<i class="fa-solid fa-thumbtack text-primary me-1 icon-xs" title="Pinned"></i>
							{/if}
							{thread.title}
						</a>
					</td>
					<td class="small text-muted">{thread.category?.name ?? 'N/A'}</td>
					<td class="small">{thread.author?.username ?? 'N/A'}</td>
					<td>
						{#if thread.status === 'deleted'}
							<span class="badge bg-danger">deleted</span>
						{:else if thread.status === 'locked'}
							<span class="badge bg-warning text-dark">locked</span>
						{:else}
							<span class="badge bg-success">open</span>
						{/if}
						{#if thread.is_solved}
							<span class="badge bg-info ms-1">solved</span>
						{/if}
					</td>
					<td class="small">{thread.reply_count ?? 0}</td>
					<td class="small"><Timestamp date={thread.created_at} /></td>
					<td>
						<div class="d-flex gap-1">
							<!-- Pin -->
							<form
								method="POST"
								action="?/pin"
								use:enhance={() => {
									actingId = thread.id;
									return async ({ result, update }) => {
										if (result.type === 'success') {
											toast.success(thread.is_pinned ? 'Thread unpinned.' : 'Thread pinned.');
										} else if (result.type === 'failure') {
											toast.error((result.data as any)?.error ?? 'Action failed.');
										}
										actingId = null;
										await update();
									};
								}}
							>
								<input type="hidden" name="id" value={thread.id} />
								<input type="hidden" name="is_pinned" value={String(thread.is_pinned)} />
								<button
									type="submit"
									class="btn btn-sm {thread.is_pinned ? 'btn-primary' : 'btn-outline-secondary'}"
									title={thread.is_pinned ? 'Unpin' : 'Pin'}
									disabled={actingId === thread.id}
								>
									<i class="fa-solid fa-thumbtack"></i>
								</button>
							</form>

							<!-- Lock -->
							<form
								method="POST"
								action="?/lock"
								use:enhance={() => {
									actingId = thread.id;
									return async ({ result, update }) => {
										if (result.type === 'success') {
											toast.success(thread.status === 'locked' ? 'Thread unlocked.' : 'Thread locked.');
										} else if (result.type === 'failure') {
											toast.error((result.data as any)?.error ?? 'Action failed.');
										}
										actingId = null;
										await update();
									};
								}}
							>
								<input type="hidden" name="id" value={thread.id} />
								<input type="hidden" name="status" value={thread.status} />
								<button
									type="submit"
									class="btn btn-sm {thread.status === 'locked' ? 'btn-warning' : 'btn-outline-secondary'}"
									title={thread.status === 'locked' ? 'Unlock' : 'Lock'}
									disabled={actingId === thread.id}
								>
									<i class="fa-solid fa-lock"></i>
								</button>
							</form>

							<!-- Move -->
							<button
								type="button"
								class="btn btn-sm btn-outline-secondary"
								title="Move to category"
								onclick={() => openMoveModal(thread)}
							>
								<i class="fa-solid fa-folder-open"></i>
							</button>

							<!-- Delete -->
							{#if thread.status !== 'deleted'}
								<form
									method="POST"
									action="?/delete"
									use:enhance={() => {
										actingId = thread.id;
										return async ({ result, update }) => {
											if (result.type === 'success') {
												toast.success('Thread deleted.');
											} else if (result.type === 'failure') {
												toast.error((result.data as any)?.error ?? 'Delete failed.');
											}
											actingId = null;
											await update();
										};
									}}
								>
									<input type="hidden" name="id" value={thread.id} />
									<button
										type="submit"
										class="btn btn-sm btn-outline-danger"
										title="Delete thread"
										disabled={actingId === thread.id}
										onclick={(e) => { if (!confirm('Delete this thread?')) e.preventDefault(); }}
									>
										<i class="fa-solid fa-trash"></i>
									</button>
								</form>
							{/if}
						</div>
					</td>
				</tr>
			{/each}
			{#if data.threads.length === 0}
				<tr>
					<td colspan="7" class="p-0">
						<div class="fr-empty-state">
							<div class="fr-empty-icon"><i class="fa-solid fa-comments"></i></div>
							<h2 class="fr-empty-title">No threads found</h2>
							<p class="fr-empty-sub">Try adjusting the filters.</p>
						</div>
					</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
</div>

<Pagination {currentPage} {totalPages} buildHref={pageHref} />

<!-- Move modal -->
{#if movingThread}
	<div
		class="modal d-block modal-backdrop-dark"
		tabindex="-1"
		role="dialog"
		aria-modal="true"
		aria-labelledby="moveModalLabel"
	>
		<div class="modal-dialog modal-dialog-centered">
			<div class="modal-content">
				<div class="modal-header">
					<h5 class="modal-title" id="moveModalLabel">
						<i class="fa-solid fa-folder-open me-2"></i>Move Thread
					</h5>
					<button
						type="button"
						class="btn-close"
						aria-label="Close"
						onclick={() => (movingThread = null)}
					></button>
				</div>
				<form
					method="POST"
					action="?/move"
					use:enhance={() => {
						return async ({ result, update }) => {
							if (result.type === 'success') {
								toast.success('Thread moved.');
								movingThread = null;
							} else if (result.type === 'failure') {
								toast.error((result.data as any)?.error ?? 'Move failed.');
							}
							await update();
						};
					}}
				>
					<div class="modal-body">
						<p class="text-muted small mb-3">
							Moving: <strong>{movingThread.title}</strong>
						</p>
						<input type="hidden" name="id" value={movingThread.id} />
						<label for="moveCategoryId" class="form-label">Destination category</label>
						<select
							id="moveCategoryId"
							name="category_id"
							class="form-select"
							bind:value={selectedCategoryId}
							required
						>
							<option value="">Select a category</option>
							{#each data.categories as cat}
								<option value={cat.id}>{cat.name}</option>
							{/each}
						</select>
					</div>
					<div class="modal-footer">
						<button
							type="button"
							class="btn btn-outline-secondary"
							onclick={() => (movingThread = null)}
						>
							Cancel
						</button>
						<button type="submit" class="btn btn-primary" disabled={!selectedCategoryId || selectedCategoryId === movingThread?.currentCategoryId}>
							Move Thread
						</button>
					</div>
				</form>
			</div>
		</div>
	</div>
{/if}

<style>
	.cat-select { width: auto; min-width: 160px; }
	.title-col { max-width: 260px; }
	.title-link { max-width: 260px; }
	.icon-xs { font-size: 0.75rem; }
	.modal-backdrop-dark { background: rgba(0, 0, 0, 0.4); }
</style>
