<svelte:head>
	<title>{data.siteName ?? 'Ferum Board'} | Home</title>
	<meta name="description" content={data.siteDescription ?? `${data.siteName ?? 'Ferum Board'} — A modern self-hosted discussion forum.`} />
	<meta property="og:title" content="{data.siteName ?? 'Ferum Board'} | Home" />
	<meta property="og:description" content={data.siteDescription ?? `${data.siteName ?? 'Ferum Board'} — A modern self-hosted discussion forum.`} />
	<meta property="og:type" content="website" />
</svelte:head>

<script lang="ts">
	import ThreadCard from '$lib/components/molecules/ThreadCard.svelte';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import { ROUTES } from '$lib/routes';

	let { data }: { data: any } = $props();

	const topCategories = $derived((data.categories ?? []).filter((c: any) => !c.parent_id));
	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 20)));
	const currentPage = $derived(data.meta?.page ?? 1);

	const popularThreads = $derived(
		[...(data.threads ?? [])]
			.sort((a: any, b: any) => (b.view_count + b.reply_count * 3) - (a.view_count + a.reply_count * 3))
			.slice(0, 5)
	);
</script>

<div class="fr-content-layout">
	<!-- Feed column -->
	<div class="fr-feed-col">
		<div class="d-flex justify-content-between align-items-center mb-3">
			<h1 class="h5 fw-semibold mb-0">Latest Discussions</h1>
		</div>

		{#if data.threads?.length > 0}
			{#if currentPage > 1}
				<Pagination
					{currentPage}
					{totalPages}
					buildHref={(p) => `?page=${p}`}
					navClass="mb-3"
					compact
				/>
			{/if}

			<div class="fr-feed">
				{#each data.threads as thread}
					<ThreadCard {thread} />
				{/each}
			</div>

			<Pagination
				{currentPage}
				{totalPages}
				buildHref={(p) => `?page=${p}`}
			/>
		{:else}
			<div class="fr-feed">
				<div class="fr-empty-state">
					<div class="fr-empty-icon">
						<i class="fa-regular fa-comments"></i>
					</div>
					<h2 class="fr-empty-title">No discussions yet</h2>
					<p class="fr-empty-sub">Be the first to kick things off — every great community starts with a single post.</p>
					{#if data.user}
						<a href={ROUTES.NEW_THREAD} class="btn btn-primary btn-sm px-4">
							<i class="fa-solid fa-plus me-2"></i>Start a Discussion
						</a>
					{:else}
						<div class="d-flex gap-2 justify-content-center flex-wrap">
							<a href={ROUTES.REGISTER} class="btn btn-primary btn-sm px-4">Get started</a>
							<a href={ROUTES.LOGIN} class="btn btn-sm px-4 fr-empty-signin">Sign in</a>
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>

	<!-- Right panel -->
	<aside class="fr-right-panel">
		<!-- Welcome card -->
		<div class="fr-panel">
			<div class="fr-panel-header">About</div>
			<div class="px-3 py-3">
				<p class="small mb-3" style="color: var(--bs-secondary-color); line-height: 1.6;">
					Welcome to <strong style="color: var(--bs-body-color);">{data.siteName ?? 'Ferum Board'}</strong> — a community for open discussion.
				</p>
				{#if data.user}
					<a href={ROUTES.NEW_THREAD} class="btn btn-primary btn-sm w-100">
						<i class="fa-solid fa-plus me-1"></i>Start a Discussion
					</a>
				{:else}
					<div class="d-grid gap-2">
						<a href={ROUTES.REGISTER} class="btn btn-primary btn-sm">Get started</a>
						<a href={ROUTES.LOGIN} class="btn btn-sm" style="border: 1px solid var(--bs-border-color); color: var(--bs-body-color);">Sign in</a>
					</div>
				{/if}
			</div>
			{#if data.meta?.total}
				<div class="fr-panel-stat border-top" style="border-top: 1px solid var(--bs-border-color);">
					<span style="color: var(--bs-secondary-color);">Threads</span>
					<span class="fw-semibold" style="color: var(--bs-body-color);">{data.meta.total.toLocaleString()}</span>
				</div>
			{/if}
			{#if topCategories.length > 0}
				<div class="fr-panel-stat">
					<span style="color: var(--bs-secondary-color);">Categories</span>
					<span class="fw-semibold" style="color: var(--bs-body-color);">{topCategories.length}</span>
				</div>
			{/if}
		</div>

		<!-- Popular threads -->
		{#if popularThreads.length > 0}
			<div class="fr-panel">
				<div class="fr-panel-header">Popular</div>
				<div class="fr-panel-body">
					{#each popularThreads as thread}
						<a href={ROUTES.THREAD(thread.slug)} class="fr-panel-row" style="align-items: flex-start; padding-top: 0.5rem; padding-bottom: 0.5rem;">
							<div style="min-width:0;">
								<div class="text-truncate fw-medium" style="font-size:0.8125rem; color: var(--bs-body-color);">{thread.title}</div>
								<div style="font-size:0.75rem; color: var(--bs-tertiary-color); margin-top:0.125rem;">
									<i class="fa-regular fa-eye me-1"></i>{thread.view_count}
									<span class="mx-1">·</span>
									<i class="fa-regular fa-comment me-1"></i>{thread.reply_count}
								</div>
							</div>
						</a>
					{/each}
				</div>
			</div>
		{/if}
	</aside>
</div>
