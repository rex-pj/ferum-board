<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { ROUTES } from "$lib/routes";

  let { data }: { data: any } = $props();

  let query = $state($page.url.searchParams.get("q") ?? "");

  const topCategories = $derived(
    (data.categories ?? []).filter((c: any) => !c.parent_id).slice(0, 8),
  );

  function handleSearch(e: SubmitEvent) {
    e.preventDefault();
    if (query.trim()) {
      goto(`${ROUTES.SEARCH}?q=${encodeURIComponent(query.trim())}`);
    }
  }
</script>

<svelte:head>
  <title>Search | {data.siteName ?? "Ferum Board"}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

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

    {#if data.tag}
      <div class="d-flex align-items-center gap-2 mb-3">
        <span class="badge bg-secondary fs-6 px-3 py-2">
          <i class="fa-solid fa-tag me-1"></i>{data.tag}
        </span>
        <a href={ROUTES.SEARCH} class="btn btn-sm btn-outline-secondary">
          <i class="fa-solid fa-times me-1"></i>Clear filter
        </a>
      </div>
    {/if}

    {#if data.results}
      {#if data.results.length > 0}
        <p class="text-muted small mb-3">
          {data.meta?.total ?? data.results.length} result{data.results
            .length !== 1
            ? "s"
            : ""}
          {#if data.tag}tagged <strong>{data.tag}</strong>{:else}for "<strong
              >{data.q}</strong
            >"{/if}
        </p>
        <div class="list-group">
          {#each data.results as hit}
            <a
              href={ROUTES.THREAD(hit.slug ?? hit.thread_slug)}
              class="list-group-item list-group-item-action py-3"
            >
              <div class="fw-semibold mb-1">{hit.title}</div>
              {#if hit.excerpt}
                <div class="text-muted small">{@html hit.excerpt}</div>
              {/if}
            </a>
          {/each}
        </div>
      {:else if data.apiError}
        <div class="fr-empty-state">
          <div class="fr-empty-icon">
            <i class="fa-solid fa-triangle-exclamation"></i>
          </div>
          <h2 class="fr-empty-title">Something went wrong</h2>
          <p class="fr-empty-sub">
            The search could not be completed. Please try again later.
          </p>
        </div>
      {:else}
        <div class="fr-empty-state">
          <div class="fr-empty-icon">
            <i class="fa-solid fa-magnifying-glass"></i>
          </div>
          <h2 class="fr-empty-title">No results found</h2>
          {#if data.tag}
            <p class="fr-empty-sub">
              No threads tagged "<strong>{data.tag}</strong>".
            </p>
          {:else}
            <p class="fr-empty-sub">
              Nothing matched "<strong>{data.q}</strong>". Try a shorter keyword
              or check the spelling.
            </p>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="fr-empty-state">
        <div class="fr-empty-icon">
          <i class="fa-solid fa-magnifying-glass"></i>
        </div>
        <h2 class="fr-empty-title">Search discussions</h2>
        <p class="fr-empty-sub">
          Enter a keyword above to find threads across the forum.
        </p>
      </div>
    {/if}
  </div>

  <!-- Right panel -->
  <aside class="fr-right-panel">
    <!-- Search tips -->
    <div class="fr-panel">
      <div class="fr-panel-header">Search Tips</div>
      <div class="fr-panel-body">
        <div class="fr-panel-row tip-row">
          <span class="small fw-medium text-body">
            <i class="fa-solid fa-quote-left fa-xs me-1 text-primary"></i>Exact
            phrase
          </span>
          <span class="small text-muted">Put words in "double quotes"</span>
        </div>
        <div class="fr-panel-row tip-row">
          <span class="small fw-medium text-body">
            <i class="fa-solid fa-keyboard fa-xs me-1 text-primary"></i>Short
            keywords
          </span>
          <span class="small text-muted"
            >Use 2–4 keywords for the best results</span
          >
        </div>
        <div class="fr-panel-row tip-row">
          <span class="small fw-medium text-body">
            <i class="fa-solid fa-folder fa-xs me-1 text-primary"></i>Browse
            categories
          </span>
          <span class="small text-muted"
            >Can't find it? Browse by category below</span
          >
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
              <i class="fa-solid fa-folder fa-sm detail-icon"></i>
              <span class="text-truncate text-body cat-name">{cat.name}</span>
            </a>
          {/each}
        </div>
      </div>
    {/if}

    <!-- CTA -->
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
</div>

<style>
  .tip-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 0.25rem;
    min-height: auto;
    padding-top: 0.5rem;
    padding-bottom: 0.5rem;
  }

  .detail-icon {
    width: 1rem;
    opacity: 0.5;
    flex-shrink: 0;
  }
  .cat-name {
    font-size: 0.8125rem;
  }
</style>
