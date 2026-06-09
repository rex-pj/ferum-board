<svelte:head>
	<title>{data.category?.name ?? 'Forum'} | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="description" content={data.category?.description ?? `Discussions in ${data.category?.name}`} />
	<link rel="canonical" href={data.canonicalUrl} />
	<meta property="og:type" content="website" />
	<meta property="og:url" content={data.canonicalUrl} />
	<meta property="og:title" content="{data.category?.name ?? 'Forum'} | {data.siteName ?? 'Ferum Board'}" />
	<meta property="og:description" content={data.category?.description ?? `Discussions in ${data.category?.name}`} />
	<meta name="twitter:card" content="summary" />
	<meta name="twitter:title" content="{data.category?.name ?? 'Forum'} | {data.siteName ?? 'Ferum Board'}" />
	<meta name="twitter:description" content={data.category?.description ?? `Discussions in ${data.category?.name}`} />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import ThreadCard from '$lib/components/molecules/ThreadCard.svelte';
	import Pagination from '$lib/components/atoms/Pagination.svelte';
	import { ROUTES } from '$lib/routes';

	let { data, form }: { data: any; form: any } = $props();

	const cat = $derived(data.category);
	const totalPages = $derived(Math.ceil((data.meta?.total ?? 0) / (data.meta?.per_page ?? 20)));
	const currentPage = $derived(data.meta?.page ?? 1);

	// Optimistic watch/mute state — falls back to server value
	let watched = $state(data.watched ?? false);
	let muted = $state(data.muted ?? false);

	$effect(() => { watched = data.watched ?? false; });
	$effect(() => { muted = data.muted ?? false; });

	const subCategories = $derived(
		(data.categories ?? []).filter((c: any) => c.parent_id === cat?.id)
	);
	const siblingCategories = $derived(
		(data.categories ?? []).filter(
			(c: any) => c.parent_id === cat?.parent_id && c.id !== cat?.id && !c.parent_id === !cat?.parent_id
		)
	);
</script>

<div class="fr-content-layout">
	<!-- Feed column -->
	<div class="fr-feed-col">
		<!-- Breadcrumb -->
		<nav aria-label="breadcrumb" class="mb-3">
			<ol class="breadcrumb breadcrumb-sm">
				<li class="breadcrumb-item"><a href={ROUTES.HOME}>Home</a></li>
				{#if data.parentCategory}
					<li class="breadcrumb-item">
						<a href={ROUTES.CATEGORY(data.parentCategory.slug)}>{data.parentCategory.name}</a>
					</li>
				{/if}
				<li class="breadcrumb-item active">{cat?.name}</li>
			</ol>
		</nav>

		<!-- Category header -->
		<div class="d-flex justify-content-between align-items-start mb-3 gap-2">
			<div class="d-flex align-items-center gap-2">
				{#if cat?.color}
					<span class="rounded-circle flex-shrink-0" style="width:10px; height:10px; background:{cat.color};"></span>
				{/if}
				<h1 class="h5 fw-semibold mb-0">{cat?.name}</h1>
				{#if watched}
					<span class="badge bg-primary-subtle text-primary-emphasis small">
						<i class="fa-solid fa-eye me-1"></i>Watching
					</span>
				{/if}
				{#if muted}
					<span class="badge bg-secondary-subtle text-secondary-emphasis small">
						<i class="fa-solid fa-eye-slash me-1"></i>Muted
					</span>
				{/if}
			</div>

			<div class="d-flex gap-2 flex-shrink-0">
				{#if data.user}
					<!-- Watch / Unwatch -->
					<form method="POST" action={watched ? '?/unwatch' : '?/watch'} use:enhance={({ submitter }) => {
						watched = !watched;
						return async ({ update }) => update({ reset: false });
					}}>
						<input type="hidden" name="category_id" value={cat?.id} />
						<button type="submit" class="btn btn-outline-secondary btn-sm" title={watched ? 'Stop watching' : 'Watch category'}>
							<i class="fa-solid {watched ? 'fa-eye-slash' : 'fa-eye'} me-1"></i>
							{watched ? 'Unwatch' : 'Watch'}
						</button>
					</form>

					<!-- Mute / Unmute -->
					<form method="POST" action={muted ? '?/unmute' : '?/mute'} use:enhance={() => {
						muted = !muted;
						return async ({ update }) => update({ reset: false });
					}}>
						<input type="hidden" name="category_id" value={cat?.id} />
						<button type="submit" class="btn btn-outline-secondary btn-sm" title={muted ? 'Unmute' : 'Mute category'}>
							<i class="fa-solid {muted ? 'fa-bell' : 'fa-bell-slash'} me-1"></i>
							{muted ? 'Unmute' : 'Mute'}
						</button>
					</form>

					<a href={ROUTES.NEW_THREAD_IN_CATEGORY(cat?.id)} class="btn btn-primary btn-sm">
						<i class="fa-solid fa-plus me-1"></i>New Thread
					</a>
				{/if}
			</div>
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
					<h2 class="fr-empty-title">No threads yet</h2>
					<p class="fr-empty-sub">Be the first to start a discussion in {cat?.name}.</p>
					{#if data.user}
						<a href={ROUTES.NEW_THREAD_IN_CATEGORY(cat?.id)} class="btn btn-primary btn-sm px-4">
							<i class="fa-solid fa-plus me-2"></i>New Thread
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
		<!-- Category info -->
		<div class="fr-panel">
			<div class="fr-panel-header">About</div>
			<div class="px-3 py-3">
				<div class="d-flex align-items-center gap-2 mb-2">
					{#if cat?.color}
						<span class="rounded-circle flex-shrink-0" style="width:10px; height:10px; background:{cat.color};"></span>
					{/if}
					<span class="fw-semibold cat-name">{cat?.name}</span>
				</div>
				{#if cat?.description}
					<p class="small mb-0 text-muted cat-desc">{cat.description}</p>
				{/if}
			</div>
			{#if data.meta?.total}
				<div class="fr-panel-stat border-top">
					<span class="text-muted">Threads</span>
					<span class="fw-semibold text-body">{data.meta.total.toLocaleString()}</span>
				</div>
			{/if}
		</div>

		<!-- Subcategories -->
		{#if subCategories.length > 0}
			<div class="fr-panel">
				<div class="fr-panel-header">Subcategories</div>
				<div class="fr-panel-body">
					{#each subCategories as sub}
						<a href={ROUTES.CATEGORY(sub.slug)} class="fr-panel-row">
							{#if sub.color}
								<span class="rounded-circle flex-shrink-0" style="width:8px; height:8px; background:{sub.color};"></span>
							{:else}
								<i class="fa-solid fa-hashtag panel-hash"></i>
							{/if}
							<span class="text-truncate">{sub.name}</span>
						</a>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Sibling categories -->
		{#if siblingCategories.length > 0}
			<div class="fr-panel">
				<div class="fr-panel-header">Related</div>
				<div class="fr-panel-body">
					{#each siblingCategories.slice(0, 5) as sib}
						<a href={ROUTES.CATEGORY(sib.slug)} class="fr-panel-row">
							{#if sib.color}
								<span class="rounded-circle flex-shrink-0" style="width:8px; height:8px; background:{sib.color};"></span>
							{:else}
								<i class="fa-solid fa-hashtag panel-hash"></i>
							{/if}
							<span class="text-truncate">{sib.name}</span>
						</a>
					{/each}
				</div>
			</div>
		{/if}
	</aside>
</div>

<style>
	.breadcrumb-sm { font-size: 0.8125rem; }
	.cat-name { font-size: 0.9375rem; }
	.cat-desc { line-height: 1.6; }
	.panel-hash {
		font-size: 0.75rem;
		width: 0.875rem;
		text-align: center;
		opacity: 0.5;
	}
</style>
