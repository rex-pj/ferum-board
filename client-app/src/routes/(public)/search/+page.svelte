<svelte:head>
	<title>Search | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { ROUTES } from '$lib/routes';

	let { data }: { data: any } = $props();

	let query = $state($page.url.searchParams.get('q') ?? '');

	const topCategories = $derived((data.categories ?? []).filter((c: any) => !c.parent_id).slice(0, 8));

	function handleSearch(e: SubmitEvent) {
		e.preventDefault();
		if (query.trim()) {
			goto(`${ROUTES.SEARCH}?q=${encodeURIComponent(query.trim())}`);
		}
	}
</script>

<div class="fr-content-layout">
<!-- Main search column -->
<div class="fr-feed-col">
	<h1 class="h4 mb-4">Search</h1>

	<form class="mb-4" onsubmit={handleSearch}>
		<div class="input-group">
			<input
				type="search"
				class="form-control form-control-lg"
				placeholder="Search threads…"
				bind:value={query}
				aria-label="Search query"
			/>
			<button type="submit" class="btn btn-primary px-4">
				<i class="fa-solid fa-magnifying-glass me-1"></i>Search
			</button>
		</div>
	</form>

	{#if data.results}
		{#if data.results.length > 0}
			<p class="text-muted small mb-3">
				{data.meta?.total ?? data.results.length} result{data.results.length !== 1 ? 's' : ''} for "<strong>{data.q}</strong>"
			</p>
			<div class="list-group">
				{#each data.results as hit}
					<a href={ROUTES.THREAD(hit.thread_slug)} class="list-group-item list-group-item-action py-3">
						<div class="fw-semibold mb-1">{hit.title}</div>
						{#if hit.excerpt}
							<div class="text-muted small">{@html hit.excerpt}</div>
						{/if}
					</a>
				{/each}
			</div>
		{:else}
			<div class="fr-empty-state">
				<div class="fr-empty-icon">
					<i class="fa-solid fa-magnifying-glass"></i>
				</div>
				<h2 class="fr-empty-title">No results found</h2>
				<p class="fr-empty-sub">Nothing matched "<strong>{data.q}</strong>". Try a shorter keyword or check the spelling.</p>
			</div>
		{/if}
	{:else}
		<div class="fr-empty-state">
			<div class="fr-empty-icon">
				<i class="fa-solid fa-magnifying-glass"></i>
			</div>
			<h2 class="fr-empty-title">Search discussions</h2>
			<p class="fr-empty-sub">Enter a keyword above to find threads across the forum.</p>
		</div>
	{/if}
</div>

<!-- Right panel -->
<aside class="fr-right-panel">
	<!-- Search tips -->
	<div class="fr-panel">
		<div class="fr-panel-header">Search Tips</div>
		<div class="fr-panel-body">
			<div class="fr-panel-row" style="align-items: flex-start; flex-direction: column; gap: 0.25rem; min-height: auto; padding-top: 0.5rem; padding-bottom: 0.5rem;">
				<span class="small" style="color: var(--bs-body-color); font-weight: 500;">
					<i class="fa-solid fa-quote-left fa-xs me-1" style="color: var(--bs-primary);"></i>Exact phrase
				</span>
				<span class="small text-muted">Put words in "double quotes"</span>
			</div>
			<div class="fr-panel-row" style="align-items: flex-start; flex-direction: column; gap: 0.25rem; min-height: auto; padding-top: 0.5rem; padding-bottom: 0.5rem;">
				<span class="small" style="color: var(--bs-body-color); font-weight: 500;">
					<i class="fa-solid fa-keyboard fa-xs me-1" style="color: var(--bs-primary);"></i>Short keywords
				</span>
				<span class="small text-muted">Use 2–4 keywords for the best results</span>
			</div>
			<div class="fr-panel-row" style="align-items: flex-start; flex-direction: column; gap: 0.25rem; min-height: auto; padding-top: 0.5rem; padding-bottom: 0.5rem;">
				<span class="small" style="color: var(--bs-body-color); font-weight: 500;">
					<i class="fa-solid fa-folder fa-xs me-1" style="color: var(--bs-primary);"></i>Browse categories
				</span>
				<span class="small text-muted">Can't find it? Browse by category below</span>
			</div>
		</div>
	</div>

	<!-- Browse categories -->
	{#if topCategories.length > 0}
		<div class="fr-panel">
			<div class="fr-panel-header">Browse Categories</div>
			<div class="fr-panel-body">
				{#each topCategories as cat}
					<a href={ROUTES.CATEGORY(cat.slug)} class="fr-panel-row">
						<i class="fa-solid fa-folder fa-sm" style="width:1rem; opacity:0.5; flex-shrink:0;"></i>
						<span class="text-truncate" style="color: var(--bs-body-color); font-size: 0.8125rem;">{cat.name}</span>
					</a>
				{/each}
			</div>
		</div>
	{/if}

	<!-- CTA -->
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
</aside>
</div>
