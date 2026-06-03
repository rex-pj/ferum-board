<script lang="ts">
  import ThreadCard from "$lib/components/molecules/ThreadCard.svelte";
  import { ROUTES } from "$lib/routes";
  import type { ForumIndexGroup, SubcategoryIndex } from "./+page.server";

  let { data }: { data: any } = $props();

  const groups: ForumIndexGroup[] = $derived(data.groups ?? []);

  // Track expanded/collapsed state for each category using plain object
  let expandedCategories: Record<string, boolean> = $state({});

  // Initialize all categories as expanded by default
  $effect(() => {
    if (groups.length > 0) {
      groups.forEach((group) => {
        if (!(group.id in expandedCategories)) {
          expandedCategories[group.id] = true;
        }
      });
    }
  });

  function toggleCategory(categoryId: string) {
    expandedCategories[categoryId] = !expandedCategories[categoryId];
  }
</script>

<svelte:head>
  <title>Forum | {data.siteName ?? "Ferum Board"}</title>
  <meta
    name="description"
    content="Browse all discussion categories on {data.siteName ??
      'Ferum Board'}."
  />
  <meta
    property="og:title"
    content="Forum | {data.siteName ?? 'Ferum Board'}"
  />
  <meta
    property="og:description"
    content="Browse all discussion categories on {data.siteName ??
      'Ferum Board'}."
  />
  <meta property="og:type" content="website" />
</svelte:head>

<div class="fr-content-layout">
  <div class="fr-feed-col">
    <!-- Breadcrumb -->
    <nav aria-label="breadcrumb" class="mb-3">
      <ol class="breadcrumb breadcrumb-sm">
        <li class="breadcrumb-item"><a href={ROUTES.HOME}>Home</a></li>
        <li class="breadcrumb-item active">Forum</li>
      </ol>
    </nav>

    <div class="d-flex justify-content-between align-items-center mb-3">
      <h1 class="h5 fw-semibold mb-0">All Categories</h1>
    </div>

    {#if groups.length === 0}
      <div class="fr-empty-state">
        <div class="fr-empty-icon">
          <i class="fa-regular fa-folder-open"></i>
        </div>
        <h2 class="fr-empty-title">No categories yet</h2>
        <p class="fr-empty-sub">
          An administrator needs to create categories before discussions can
          start.
        </p>
      </div>
    {:else}
      {#each groups as group}
        <section class="fr-forum-group mb-4">
          <!-- Category header row -->
          <button
            type="button"
            class="fr-forum-group-header"
            onclick={() => toggleCategory(group.id)}
            aria-expanded={expandedCategories[group.id] ?? true}
          >
            <div class="d-flex align-items-center gap-2 min-w-0 flex-grow-1">
              <span class="fr-collapse-indicator flex-shrink-0">
                <i
                  class="fa-solid fa-chevron-down"
                  style="transition: transform 0.2s ease; transform: {(expandedCategories[
                    group.id
                  ] ?? true)
                    ? 'rotate(0deg)'
                    : 'rotate(-90deg)'};"
                ></i>
              </span>
              {#if group.color}
                <span
                  class="rounded-circle flex-shrink-0"
                  style="width:12px; height:12px; background:{group.color};"
                ></span>
              {:else}
                <i class="fa-solid fa-layer-group group-icon"></i>
              {/if}
              <span class="fr-forum-group-title text-truncate inherit-color">
                {group.name}
              </span>
            </div>
            <span class="fr-forum-group-count flex-shrink-0 inherit-color">
              <i class="fa-regular fa-comment me-1"
              ></i>{group.thread_count.toLocaleString()}
            </span>
          </button>

          {#if expandedCategories[group.id] ?? true}
            {#if group.description}
              <p class="fr-forum-group-desc">{group.description}</p>
            {/if}

            <!-- Subcategory chips -->
            {#if group.subcategories.length > 0}
              <div class="fr-subcategory-row">
                {#each group.subcategories as sub}
                  <a
                    href={ROUTES.CATEGORY(sub.slug)}
                    class="fr-subcategory-chip"
                  >
                    {#if sub.color}
                      <span
                        class="rounded-circle flex-shrink-0"
                        style="width:7px; height:7px; background:{sub.color};"
                      ></span>
                    {/if}
                    <span class="text-truncate">{sub.name}</span>
                    <span class="fr-sub-count"
                      >{sub.thread_count.toLocaleString()}</span
                    >
                  </a>
                {/each}
              </div>
            {/if}

            <!-- Recent threads (max 5) -->
            {#if group.recent_threads.length > 0}
              <div class="fr-feed mt-2">
                {#each group.recent_threads as thread}
                  <ThreadCard {thread} />
                {/each}
              </div>
              <div class="fr-forum-group-footer">
                <a href={ROUTES.CATEGORY(group.slug)} class="fr-view-all-link">
                  View all threads in {group.name}
                  <i class="fa-solid fa-arrow-right ms-1 arrow-icon"></i>
                </a>
              </div>
            {:else}
              <p class="fr-forum-group-empty">No threads yet.</p>
            {/if}
          {/if}
        </section>
      {/each}
    {/if}
  </div>

  <!-- Right panel: quick navigation -->
  <aside class="fr-right-panel">
    {#if groups.length > 0}
      <div class="fr-panel">
        <div class="fr-panel-header">Jump to</div>
        <div class="fr-panel-body">
          {#each groups as group}
            <a href={ROUTES.CATEGORY(group.slug)} class="fr-panel-row">
              {#if group.color}
                <span
                  class="rounded-circle flex-shrink-0"
                  style="width:8px; height:8px; background:{group.color};"
                ></span>
              {:else}
                <i class="fa-solid fa-hashtag panel-hash"></i>
              {/if}
              <span class="text-truncate">{group.name}</span>
              <span class="ms-auto panel-count">
                {group.thread_count.toLocaleString()}
              </span>
            </a>
          {/each}
        </div>
      </div>
    {/if}

    {#if data.user}
      <div class="fr-panel">
        <div class="px-3 py-3">
          <a href={ROUTES.NEW_THREAD} class="btn btn-primary btn-sm w-100">
            <i class="fa-solid fa-plus me-1"></i>Start a Discussion
          </a>
        </div>
      </div>
    {:else}
      <div class="fr-panel">
        <div class="px-3 py-3">
          <div class="d-grid gap-2">
            <a href={ROUTES.REGISTER} class="btn btn-primary btn-sm"
              >Get started</a
            >
            <a href={ROUTES.LOGIN} class="btn btn-sm fr-btn-ghost">Sign in</a>
          </div>
        </div>
      </div>
    {/if}
  </aside>
</div>

<style>
  .fr-forum-group {
    border: 1px solid var(--bs-border-color);
    border-radius: var(--bs-border-radius);
    overflow: hidden;
    background: var(--bs-body-bg);
  }

  .fr-forum-group-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: var(--bs-tertiary-bg);
    border: none;
    border-bottom: 1px solid var(--bs-border-color);
    min-width: 0;
    width: 100%;
    cursor: pointer;
    text-align: left;
    transition: background-color 0.15s ease;
  }

  .fr-forum-group-header:hover {
    background: color-mix(
      in srgb,
      var(--bs-tertiary-bg) 95%,
      var(--bs-primary) 5%
    );
  }

  .fr-forum-group-header:active {
    background: color-mix(
      in srgb,
      var(--bs-tertiary-bg) 90%,
      var(--bs-primary) 10%
    );
  }

  .fr-collapse-indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    color: var(--bs-secondary-color);
  }

  .fr-forum-group-title {
    font-weight: 600;
    font-size: 0.9375rem;
    color: var(--bs-body-color);
    text-decoration: none;
  }

  .fr-forum-group-title:hover {
    text-decoration: none;
  }

  .fr-forum-group-count {
    font-size: 0.8125rem;
    color: var(--bs-secondary-color);
  }

  .fr-forum-group-desc {
    padding: 0.5rem 1rem 0;
    font-size: 0.8125rem;
    color: var(--bs-secondary-color);
    margin: 0;
    line-height: 1.5;
  }

  .fr-subcategory-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
    padding: 0.625rem 1rem;
    border-bottom: 1px solid var(--bs-border-color);
  }

  .fr-subcategory-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.2rem 0.6rem;
    border-radius: var(--bs-border-radius-pill);
    border: 1px solid var(--bs-border-color);
    font-size: 0.75rem;
    color: var(--bs-body-color);
    text-decoration: none;
    background: var(--bs-body-bg);
    min-height: 28px;
    transition: background 0.15s;
  }

  .fr-subcategory-chip:hover {
    background: var(--bs-tertiary-bg);
  }

  .fr-sub-count {
    color: var(--bs-tertiary-color);
    font-size: 0.7rem;
  }

  .fr-forum-group :global(.fr-feed) {
    border-radius: 0;
    border: none;
  }

  .fr-forum-group :global(.fr-thread-row) {
    border-left: none;
    border-right: none;
    border-top: none;
  }

  .fr-forum-group :global(.fr-thread-row:last-child) {
    border-bottom: none;
  }

  .fr-forum-group-footer {
    padding: 0.5rem 1rem;
    border-top: 1px solid var(--bs-border-color);
    background: var(--bs-tertiary-bg);
  }

  .fr-view-all-link {
    font-size: 0.8125rem;
    color: var(--bs-secondary-color);
    text-decoration: none;
  }

  .fr-view-all-link:hover {
    color: var(--bs-body-color);
  }

  .fr-forum-group-empty {
    padding: 0.75rem 1rem;
    font-size: 0.8125rem;
    color: var(--bs-tertiary-color);
    margin: 0;
  }

  .breadcrumb-sm { font-size: 0.8125rem; }

  .group-icon {
    font-size: 0.875rem;
    opacity: 0.5;
    flex-shrink: 0;
  }

  .inherit-color { color: inherit; }

  .arrow-icon { font-size: 0.7rem; }

  .panel-hash {
    font-size: 0.75rem;
    width: 0.875rem;
    text-align: center;
    opacity: 0.5;
  }

  .panel-count {
    font-size: 0.75rem;
    color: var(--bs-tertiary-color);
    flex-shrink: 0;
  }
</style>
