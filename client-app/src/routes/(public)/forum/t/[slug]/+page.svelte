<svelte:head>
	<title>{data.thread?.title ?? 'Thread'} | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="description" content={data.excerpt ?? data.thread?.title} />
	<meta property="og:title" content={data.thread?.title} />
	<meta property="og:description" content={data.excerpt ?? data.thread?.title} />
	<meta property="og:type" content="article" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import PostBody from '$lib/components/molecules/PostBody.svelte';
	import PostComposer from '$lib/components/organisms/PostComposer.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';
	import { isModerator, isModeratorOf } from '$lib/utils/permissions';

	let { data, form }: { data: any; form: any } = $props();

	const thread = $derived(data.thread);
	const totalPages = $derived(Math.ceil((data.postsMeta?.total ?? 0) / (data.postsMeta?.per_page ?? 20)));
	const currentPage = $derived(data.postsMeta?.page ?? 1);

	let replyContent = $state('');
	let submitting = $state(false);
	let showReply = $state(false);
	let bookmarked = $state(data.isBookmarked ?? false);

	let thumbnailUrl = $state<string | null>(thread?.thumbnail_url ?? null);
	let thumbnailCacheBust = $state(0);
	let thumbnailUploading = $state(false);
	let uploadFormEl = $state<HTMLFormElement | null>(null);

	// Keep local state in sync after load invalidations (post-upload/remove)
	$effect(() => { thumbnailUrl = thread?.thumbnail_url ?? null; });

	const canManageThumbnail = $derived(
		!!data.user &&
		(data.user.id === thread?.author_id || isModerator(data.user))
	);

	function paginationPages(current: number, total: number): (number | null)[] {
		if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1);
		const pages: (number | null)[] = [1];
		const start = Math.max(2, current - 1);
		const end = Math.min(total - 1, current + 1);
		if (start > 2) pages.push(null);
		for (let i = start; i <= end; i++) pages.push(i);
		if (end < total - 1) pages.push(null);
		pages.push(total);
		return pages;
	}

	// Report modal state
	let reportPostId = $state<string | null>(null);
	let reportReason = $state('');
	let reportSubmitting = $state(false);

	const canReply = $derived(
		data.user &&
		thread?.status === 'open' &&
		(data.user.trust_level !== 'new')
	);

	function openReportModal(postId: string) {
		reportPostId = postId;
		reportReason = '';
	}

	async function submitReport() {
		if (!reportPostId || !reportReason.trim()) return;
		reportSubmitting = true;
		try {
			const res = await fetch('/api/reports', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ post_id: reportPostId, reason: reportReason })
			});
			if (res.ok) {
				toast.success('Report submitted. Our moderators will review it.');
			} else {
				const body = await res.json().catch(() => ({}));
				toast.error(body?.error?.message ?? 'Failed to submit report.');
			}
		} catch {
			toast.error('Network error. Please try again.');
		} finally {
			reportSubmitting = false;
			reportPostId = null;
		}
	}
</script>

<div class="fr-content-layout">
<!-- Main reading column -->
<div class="fr-feed-col feed-col">
	<nav aria-label="breadcrumb" class="mb-3">
		<ol class="breadcrumb">
			<li class="breadcrumb-item"><a href={ROUTES.HOME}>Home</a></li>
			{#if data.category}
				<li class="breadcrumb-item">
					<a href={ROUTES.CATEGORY(data.category.slug)}>{data.category.name}</a>
				</li>
			{/if}
			<li class="breadcrumb-item active text-truncate breadcrumb-title">{thread?.title}</li>
		</ol>
	</nav>

	<div class="d-flex justify-content-between align-items-start mb-4">
		<h1 class="h4 mb-0 me-3">
			{#if thread?.is_solved}
				<i class="fa-solid fa-circle-check text-success me-2" title="Solved"></i>
			{/if}
			{#if thread?.status === 'locked'}
				<i class="fa-solid fa-lock text-warning me-2" title="Locked"></i>
			{/if}
			{thread?.title}
		</h1>
		<div class="d-flex gap-2 flex-shrink-0">
			{#if data.user?.id === thread?.author_id}
				<a
					href={ROUTES.EDIT_THREAD(thread.slug)}
					class="btn btn-sm btn-outline-secondary"
					title="Edit thread"
				>
					<i class="fa-solid fa-pen me-1"></i>Edit
				</a>
			{/if}
			{#if data.user}
				<form
					method="POST"
					action={bookmarked ? '?/unbookmark' : '?/bookmark'}
					use:enhance={() => {
						return async ({ result, update }) => {
							if (result.type === 'success') {
								bookmarked = !bookmarked;
							}
							await update({ reset: false });
						};
					}}
				>
					<input type="hidden" name="thread_id" value={thread.id} />
					<button
						type="submit"
						class="btn btn-sm {bookmarked ? 'btn-warning' : 'btn-outline-secondary'}"
						title={bookmarked ? 'Remove bookmark' : 'Bookmark this thread'}
					>
						<i class="fa-solid fa-bookmark"></i>
					</button>
				</form>
			{/if}
			{#if isModeratorOf(data.user, thread?.category_id)}
				<div class="dropdown">
					<button class="btn btn-outline-secondary btn-sm dropdown-toggle" type="button" data-bs-toggle="dropdown">
						Mod
					</button>
					<ul class="dropdown-menu">
						<li>
							<form method="POST" action="?/pin">
								<input type="hidden" name="thread_id" value={thread.id} />
								<input type="hidden" name="is_pinned" value={String(thread.is_pinned)} />
								<button type="submit" class="dropdown-item">
									<i class="fa-solid fa-thumbtack me-2"></i>
									{thread?.is_pinned ? 'Unpin' : 'Pin'}
								</button>
							</form>
						</li>
						<li>
							<form method="POST" action="?/lock">
								<input type="hidden" name="thread_id" value={thread.id} />
								<input type="hidden" name="thread_status" value={thread.status} />
								<button type="submit" class="dropdown-item">
									<i class="fa-solid fa-lock me-2"></i>
									{thread?.status === 'locked' ? 'Unlock' : 'Lock'}
								</button>
							</form>
						</li>
					</ul>
				</div>
			{/if}
		</div>
	</div>

	{#if form?.error}
		<div class="alert alert-danger">{form.error}</div>
	{/if}

	{#if thumbnailUrl || canManageThumbnail}
		<div class="th-wrap rounded-3 overflow-hidden mb-1">
			{#if thumbnailUrl}
				<img src="{thumbnailUrl}?_t={thumbnailCacheBust}" alt="Thread thumbnail" class="th-img" />
			{:else}
				<div class="th-empty">
					<i class="fa-regular fa-image empty-img-icon"></i>
				</div>
			{/if}

			{#if canManageThumbnail}
				<div class="th-overlay">
					<div class="th-btns">
						<form
							method="POST"
							action="?/uploadThumbnail"
							enctype="multipart/form-data"
							bind:this={uploadFormEl}
							use:enhance={() => {
								thumbnailUploading = true;
								return async ({ result, update }) => {
									thumbnailUploading = false;
									if (result.type === 'success') {
										const url = (result.data as any)?.thumbnailUrl;
										if (url) thumbnailUrl = url;
										thumbnailCacheBust = Date.now();
										toast.success('Thumbnail updated.');
									} else if (result.type === 'failure') {
										toast.error((result.data as any)?.thumbnailError ?? 'Upload failed.');
									}
									await update({ invalidateAll: true });
								};
							}}
						>
							<input type="hidden" name="thread_id" value={thread.id} />
							<label class="btn btn-sm btn-light upload-label">
								{#if thumbnailUploading}
									<span class="spinner-border spinner-border-sm me-1"></span>Uploading…
								{:else}
									<i class="fa-solid fa-{thumbnailUrl ? 'arrows-rotate' : 'upload'} me-1"></i>
									{thumbnailUrl ? 'Replace' : 'Add Image'}
								{/if}
								<input
									type="file"
									name="file"
									accept="image/jpeg,image/png,image/webp,image/gif"
									class="d-none"
									disabled={thumbnailUploading}
									onchange={() => uploadFormEl?.requestSubmit()}
								/>
							</label>
						</form>
						{#if thumbnailUrl}
							<form
								method="POST"
								action="?/removeThumbnail"
								use:enhance={() => {
									thumbnailUploading = true;
									return async ({ result, update }) => {
										thumbnailUploading = false;
										if (result.type === 'failure') {
											toast.error((result.data as any)?.thumbnailError ?? 'Failed to remove thumbnail.');
										} else {
											toast.success('Thumbnail removed.');
										}
										await update({ invalidateAll: true });
									};
								}}
							>
								<input type="hidden" name="thread_id" value={thread.id} />
								<button type="submit" class="btn btn-sm btn-danger" disabled={thumbnailUploading}>
									<i class="fa-solid fa-trash me-1"></i>Remove
								</button>
							</form>
						{/if}
					</div>
				</div>
			{/if}
		</div>
		<div class="mb-4"></div>
	{/if}

	<!-- Posts -->
	<div class="posts-list">
		{#each data.posts ?? [] as post}
			<PostBody
				{post}
				isBestAnswer={post.id === thread?.best_answer_id}
				loggedIn={!!data.user}
				currentUsername={data.user?.username}
				myReactions={post.my_reactions ?? []}
				onReport={openReportModal}
				class="mb-4"
			/>
			{#if data.user?.id === thread?.author_id && !thread?.is_solved && post.id !== thread?.best_answer_id}
				<form
					method="POST"
					action="?/markBestAnswer"
					use:enhance={() => {
						return async ({ update }) => { await update(); };
					}}
					class="mb-3 ms-1"
				>
					<input type="hidden" name="thread_id" value={thread.id} />
					<input type="hidden" name="post_id" value={post.id} />
					<button type="submit" class="btn btn-outline-success btn-sm">
						<i class="fa-solid fa-circle-check me-1"></i>Mark as Best Answer
					</button>
				</form>
			{/if}
		{/each}
	</div>

	<!-- Pagination -->
	{#if totalPages > 1}
		<nav class="my-4" aria-label="Page navigation">
			<ul class="pagination">
				{#if currentPage > 1}
					<li class="page-item">
						<a class="page-link" href="?page={currentPage - 1}">Previous</a>
					</li>
				{/if}
				{#each paginationPages(currentPage, totalPages) as p}
					{#if p === null}
						<li class="page-item disabled"><span class="page-link">…</span></li>
					{:else}
						<li class="page-item" class:active={p === currentPage}>
							<a class="page-link" href="?page={p}">{p}</a>
						</li>
					{/if}
				{/each}
				{#if currentPage < totalPages}
					<li class="page-item">
						<a class="page-link" href="?page={currentPage + 1}">Next</a>
					</li>
				{/if}
			</ul>
		</nav>
	{/if}

	<!-- Reply composer -->
	{#if thread?.status === 'locked'}
		<div class="alert alert-secondary">
			<i class="fa-solid fa-lock me-2"></i>This thread is locked and no longer accepts replies.
		</div>
	{:else if canReply}
		{#if !showReply}
			<button class="btn btn-primary" onclick={() => (showReply = true)}>
				<i class="fa-solid fa-reply me-2"></i>Reply
			</button>
		{:else}
			<div class="card mt-4">
				<div class="card-header fw-semibold">Your Reply</div>
				<div class="card-body">
					<form
						method="POST"
						action="?/reply"
						use:enhance={() => {
							submitting = true;
							return async ({ update }) => {
								submitting = false;
								replyContent = '';
								showReply = false;
								await update();
							};
						}}
					>
						<input type="hidden" name="thread_id" value={thread.id} />
						<PostComposer bind:value={replyContent} name="content_md" />
						<div class="mt-3 d-flex gap-2">
							<button type="submit" class="btn btn-primary" disabled={submitting || !replyContent.trim()}>
								{#if submitting}
									<span class="spinner-border spinner-border-sm me-1"></span>
								{/if}
								Post Reply
							</button>
							<button type="button" class="btn btn-outline-secondary" onclick={() => (showReply = false)}>
								Cancel
							</button>
						</div>
					</form>
				</div>
			</div>
		{/if}
	{:else if data.user}
		<div class="alert alert-info">
			Your account needs to reach <strong>Basic</strong> level to post replies.
			Verify your email to get started.
		</div>
	{:else}
		<div class="alert alert-secondary">
			<a href={ROUTES.LOGIN}>Sign in</a> or <a href={ROUTES.REGISTER}>register</a> to reply.
		</div>
	{/if}
</div><!-- end fr-feed-col -->

<!-- Right panel -->
<aside class="fr-right-panel">
	<!-- Thread info -->
	<div class="fr-panel">
		<div class="fr-panel-header">Thread Info</div>
		<div class="fr-panel-body">
			{#if thread?.author}
				<div class="fr-panel-row">
					<i class="fa-regular fa-user detail-icon"></i>
					<span class="text-muted">Author</span>
					<a href={ROUTES.USER_PROFILE(thread.author.username)} class="ms-auto fw-medium text-truncate detail-link">
						{thread.author.display_name ?? thread.author.username}
					</a>
				</div>
			{/if}
			{#if thread?.created_at}
				<div class="fr-panel-row">
					<i class="fa-regular fa-clock detail-icon"></i>
					<span class="text-muted">Posted</span>
					<span class="ms-auto"><Timestamp date={thread.created_at} /></span>
				</div>
			{/if}
		</div>
		<div class="fr-panel-stat">
			<span class="text-muted"><i class="fa-regular fa-eye me-1"></i>Views</span>
			<span class="fw-semibold text-body">{(thread?.view_count ?? 0).toLocaleString()}</span>
		</div>
		<div class="fr-panel-stat">
			<span class="text-muted"><i class="fa-regular fa-comment me-1"></i>Replies</span>
			<span class="fw-semibold text-body">{(thread?.reply_count ?? 0).toLocaleString()}</span>
		</div>
		{#if thread?.is_pinned || thread?.is_solved || thread?.status === 'locked'}
			<div class="fr-panel-stat flag-stat">
				{#if thread.is_pinned}
					<span class="badge bg-primary badge-sm">
						<i class="fa-solid fa-thumbtack me-1"></i>Pinned
					</span>
				{/if}
				{#if thread.is_solved}
					<span class="badge bg-success badge-sm">
						<i class="fa-solid fa-circle-check me-1"></i>Solved
					</span>
				{/if}
				{#if thread?.status === 'locked'}
					<span class="badge bg-warning text-dark badge-sm">
						<i class="fa-solid fa-lock me-1"></i>Locked
					</span>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Category -->
	{#if data.category}
		<div class="fr-panel">
			<div class="fr-panel-header">Category</div>
			<div class="px-3 py-2">
				<a href={ROUTES.CATEGORY(data.category.slug)} class="fw-semibold text-decoration-none d-block mb-1 text-body">
					<i class="fa-solid fa-folder me-1 opacity-50"></i>{data.category.name}
				</a>
				{#if data.category.description}
					<p class="small mb-0 text-muted cat-desc">{data.category.description}</p>
				{/if}
			</div>
		</div>
	{/if}

	<!-- Start new thread CTA -->
	{#if data.user}
		<a href={ROUTES.NEW_THREAD} class="btn btn-primary btn-sm w-100">
			<i class="fa-solid fa-plus me-1"></i>New Thread
		</a>
	{:else}
		<div class="d-grid gap-2">
			<a href={ROUTES.REGISTER} class="btn btn-primary btn-sm">Get started</a>
			<a href={ROUTES.LOGIN} class="btn btn-sm fr-btn-ghost">Sign in</a>
		</div>
	{/if}
</aside>
</div><!-- end fr-content-layout -->

<!-- Report modal -->
{#if reportPostId}
	<div class="modal d-block modal-backdrop-dark" tabindex="-1" role="dialog" aria-modal="true" aria-labelledby="reportModalLabel">
		<div class="modal-dialog modal-dialog-centered">
			<div class="modal-content">
				<div class="modal-header">
					<h5 class="modal-title" id="reportModalLabel">
						<i class="fa-solid fa-flag me-2 text-danger"></i>Report Post
					</h5>
					<button type="button" class="btn-close" aria-label="Close" onclick={() => (reportPostId = null)}></button>
				</div>
				<div class="modal-body">
					<label for="reportReason" class="form-label">Reason for report</label>
					<textarea
						id="reportReason"
						class="form-control"
						rows="3"
						placeholder="Describe why this post violates the rules…"
						bind:value={reportReason}
						maxlength="500"
					></textarea>
				</div>
				<div class="modal-footer">
					<button type="button" class="btn btn-outline-secondary" onclick={() => (reportPostId = null)}>
						Cancel
					</button>
					<button
						type="button"
						class="btn btn-danger"
						disabled={reportSubmitting || !reportReason.trim()}
						onclick={submitReport}
					>
						{#if reportSubmitting}
							<span class="spinner-border spinner-border-sm me-1"></span>
						{/if}
						Submit Report
					</button>
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	.th-wrap {
		position: relative;
		background: var(--bs-secondary-bg);
		max-height: 320px;
	}

	.th-img {
		width: 100%;
		height: 320px;
		object-fit: cover;
		object-position: center;
		display: block;
	}

	.th-empty {
		height: 80px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.th-overlay {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		justify-content: flex-end;
		padding: 0.75rem;
		background: linear-gradient(to top, rgba(0,0,0,0.55) 0%, transparent 60%);
		opacity: 0;
		transition: opacity 0.18s ease;
	}

	.th-wrap:hover .th-overlay,
	.th-wrap:focus-within .th-overlay {
		opacity: 1;
	}

	.th-wrap:not(:has(.th-img)) .th-overlay {
		opacity: 1;
		background: none;
	}

	.th-btns {
		display: flex;
		gap: 0.5rem;
	}

	.feed-col { min-width: 0; }
	.breadcrumb-title { max-width: 300px; }
	.empty-img-icon { font-size: 2rem; opacity: 0.3; }
	.upload-label { cursor: pointer; min-width: 90px; }
	.detail-icon { width: 1rem; opacity: 0.5; flex-shrink: 0; }
	.detail-link { max-width: 110px; color: var(--bs-body-color); }
	.flag-stat { gap: 0.375rem; justify-content: flex-start; }
	.badge-sm { font-weight: 500; font-size: 0.7rem; }
	.cat-desc { line-height: 1.5; }
	.modal-backdrop-dark { background: rgba(0, 0, 0, 0.4); }
</style>
