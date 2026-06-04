<svelte:head>
	<title>Bookmarks | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { ROUTES } from '$lib/routes';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';

	let { data }: { data: any } = $props();

	const totalPages  = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 20)));
	const currentPage = $derived(data.meta?.page ?? 1);
	const total       = $derived(data.meta?.total ?? 0);
</script>

<div class="fr-content-layout">
	<!-- Bookmarks feed -->
	<div class="fr-feed-col">
		<div class="d-flex justify-content-between align-items-center mb-3">
			<h1 class="h5 fw-semibold mb-0">
				<i class="fa-solid fa-bookmark me-2 text-primary"></i>Bookmarks
			</h1>
			{#if total > 0}
				<span class="badge rounded-pill bm-count-badge">
					{total}
				</span>
			{/if}
		</div>

		{#if data.bookmarks?.length > 0}
			<div class="fr-feed mb-3">
				{#each data.bookmarks as item}
					<a href={ROUTES.THREAD(item.thread.slug)} class="fr-thread-row bm-row">
						<div class="bm-body">
							<div class="fw-medium text-truncate bm-title">
								{#if item.thread.is_solved}
									<i class="fa-solid fa-circle-check text-success me-1 bm-flag-icon" title="Solved"></i>
								{/if}
								{#if item.thread.status === 'locked'}
									<i class="fa-solid fa-lock me-1 bm-flag-icon bm-locked" title="Locked"></i>
								{/if}
								{item.thread.title}
							</div>
							<div class="d-flex align-items-center gap-3 mt-1 bm-meta">
								<span title="{item.thread.reply_count} {item.thread.reply_count === 1 ? 'reply' : 'replies'}"><i class="fa-regular fa-comment me-1"></i>{item.thread.reply_count}</span>
								<span title="{item.thread.view_count.toLocaleString()} views"><i class="fa-regular fa-eye me-1"></i>{item.thread.view_count}</span>
								<span>·</span>
								<span>Saved <Timestamp date={item.bookmarked_at} /></span>
							</div>
						</div>
						<i class="fa-solid fa-bookmark flex-shrink-0 mt-1 bm-icon"></i>
					</a>
				{/each}
			</div>

			{#if totalPages > 1}
				<nav aria-label="Bookmark pages">
					<ul class="pagination pagination-sm">
						{#if currentPage > 1}
							<li class="page-item">
								<a class="page-link" href="?page={currentPage - 1}">Previous</a>
							</li>
						{/if}
						{#each Array.from({ length: Math.min(totalPages, 10) }, (_, i) => i + 1) as p}
							<li class="page-item" class:active={p === currentPage}>
								<a class="page-link" href="?page={p}">{p}</a>
							</li>
						{/each}
						{#if currentPage < totalPages}
							<li class="page-item">
								<a class="page-link" href="?page={currentPage + 1}">Next</a>
							</li>
						{/if}
					</ul>
				</nav>
			{/if}
		{:else}
			<div class="fr-feed">
				<div class="fr-empty-state">
					<div class="fr-empty-icon">
						<i class="fa-regular fa-bookmark"></i>
					</div>
					<h2 class="fr-empty-title">No bookmarks yet</h2>
					<p class="fr-empty-sub">
						Save threads for later by clicking the bookmark icon on any thread page.
					</p>
					<a href={ROUTES.HOME} class="btn btn-primary btn-sm px-4">Browse discussions</a>
				</div>
			</div>
		{/if}
	</div>

	<!-- Right panel -->
	<aside class="fr-right-panel">
		<!-- Stats -->
		<div class="fr-panel">
			<div class="fr-panel-header">Stats</div>
			<div class="fr-panel-body px-3 py-2">
				<div class="fr-panel-stat stat-no-border">
					<span class="text-muted stat-label">Saved threads</span>
					<span class="fw-semibold stat-value">{total}</span>
				</div>
				{#if totalPages > 1}
					<div class="fr-panel-stat stat-top-border">
						<span class="text-muted stat-label">Page</span>
						<span class="fw-semibold stat-value">{currentPage} / {totalPages}</span>
					</div>
				{/if}
			</div>
		</div>

		<!-- Quick links -->
		<div class="fr-panel">
			<div class="fr-panel-header">Account</div>
			<div class="fr-panel-body">
				<a href={ROUTES.NOTIFICATIONS} class="fr-panel-row">
					<i class="fa-solid fa-bell kind-icon"></i>
					Notifications
				</a>
				<a href={ROUTES.ACCOUNT} class="fr-panel-row">
					<i class="fa-solid fa-user kind-icon"></i>
					Profile settings
				</a>
				<a href={ROUTES.HOME} class="fr-panel-row">
					<i class="fa-solid fa-house kind-icon"></i>
					Back to feed
				</a>
			</div>
		</div>

		<!-- Tip -->
		<div class="fr-panel">
			<div class="fr-panel-header">Tip</div>
			<div class="px-3 py-3">
				<p class="mb-0 text-muted tip-text">
					Open any thread and click the <i class="fa-solid fa-bookmark mx-1 text-primary"></i> icon to save or unsave it.
				</p>
			</div>
		</div>
	</aside>
</div>

<style>
	.bm-count-badge {
		background: var(--bs-tertiary-bg);
		color: var(--bs-secondary-color);
		border: 1px solid var(--bs-border-color);
		font-weight: 500;
	}

	.bm-row   { text-decoration: none; display: flex; }
	.bm-body  { flex: 1; min-width: 0; }
	.bm-title { font-size: 0.9375rem; color: var(--bs-body-color); }
	.bm-flag-icon { font-size: 0.8rem; }
	.bm-locked { color: var(--bs-warning); }
	.bm-meta  { font-size: 0.75rem; color: var(--bs-tertiary-color); }
	.bm-icon  { font-size: 0.75rem; color: var(--bs-primary); opacity: 0.6; }

	.stat-no-border  { border: none; padding: 0.5rem 0; }
	.stat-top-border { border-top: 1px solid var(--bs-border-color); padding: 0.5rem 0 0; }
	.stat-label { color: var(--bs-secondary-color); font-size: 0.8125rem; }
	.stat-value { font-size: 0.8125rem; color: var(--bs-body-color); }

	.kind-icon { width: 1rem; text-align: center; font-size: 0.75rem; }
	.tip-text  { font-size: 0.8125rem; line-height: 1.6; }
</style>
