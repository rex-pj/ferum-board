<svelte:options customElement={{ tag: "ferum-remote-select", shadow: "none" }} />

<script lang="ts">
  /**
   * Generic remote-data picker for filter bars. Backed by any endpoint that
   * returns `{ data: [{value,label,sublabel?,avatar_url?}], meta:{total,page,per_page} }`.
   * Renders a hidden <input name={name}> so it participates in a surrounding
   * GET <form> exactly like a native <select> — submit the form to apply.
   *
   * Large datasets are handled by debounced server-side search plus
   * scroll-to-load paging, so the full option list is never shipped to the page.
   */
  let {
    endpoint = '',
    name = '',
    value = '',
    label = '',
    placeholder = 'Any',
  } = $props<{
    endpoint?: string;
    name?: string;
    value?: string;
    label?: string;
    placeholder?: string;
  }>();

  type Option = { value: string; label: string; sublabel?: string; avatar_url?: string };

  let selectedValue = $state(value);
  let selectedLabel = $state(label);

  let open = $state(false);
  let query = $state('');
  let options = $state<Option[]>([]);
  let page = $state(1);
  let total = $state(0);
  let loading = $state(false);
  let loaded = $state(0);

  let rootEl: HTMLElement;
  let listEl: HTMLElement | undefined = $state();
  let debounce: ReturnType<typeof setTimeout> | undefined;

  const PER_PAGE = 20;
  const hasMore = $derived(loaded < total);

  async function fetchPage(reset: boolean) {
    if (loading || !endpoint) return;
    loading = true;
    const nextPage = reset ? 1 : page + 1;
    const url = `${endpoint}?q=${encodeURIComponent(query)}&page=${nextPage}&per_page=${PER_PAGE}`;
    try {
      const res = await fetch(url, { headers: { Accept: 'application/json' } });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const body = await res.json();
      const rows: Option[] = body.data ?? [];
      total = body.meta?.total ?? rows.length;
      if (reset) {
        options = rows;
        loaded = rows.length;
        page = 1;
      } else {
        options = [...options, ...rows];
        loaded += rows.length;
        page = nextPage;
      }
    } catch (_e) {
      if (reset) options = [];
    } finally {
      loading = false;
    }
  }

  function openDropdown() {
    open = true;
    if (options.length === 0) fetchPage(true);
  }

  function onInput(e: Event) {
    query = (e.target as HTMLInputElement).value;
    clearTimeout(debounce);
    debounce = setTimeout(() => fetchPage(true), 250);
  }

  function onScroll() {
    if (!listEl || loading || !hasMore) return;
    const { scrollTop, scrollHeight, clientHeight } = listEl;
    if (scrollTop + clientHeight >= scrollHeight - 24) fetchPage(false);
  }

  function choose(opt: Option) {
    selectedValue = opt.value;
    selectedLabel = opt.label;
    open = false;
  }

  function clear(e: Event) {
    e.stopPropagation();
    selectedValue = '';
    selectedLabel = '';
  }

  function onDocClick(e: MouseEvent) {
    if (rootEl && !rootEl.contains(e.target as Node)) open = false;
  }

  $effect(() => {
    document.addEventListener('click', onDocClick);
    return () => document.removeEventListener('click', onDocClick);
  });
</script>

<div class="ferum-rs" bind:this={rootEl}>
  <input type="hidden" {name} value={selectedValue} />

  <button type="button" class="form-select form-select-sm rs-control text-start" onclick={openDropdown}>
    {#if selectedLabel}
      <span class="rs-value">{selectedLabel}</span>
    {:else}
      <span class="text-muted">{placeholder}</span>
    {/if}
  </button>
  {#if selectedValue}
    <button type="button" class="rs-clear" title="Clear" onclick={clear} aria-label="Clear selection">
      <i class="fa-solid fa-xmark"></i>
    </button>
  {/if}

  {#if open}
    <div class="rs-pop card shadow-sm">
      <div class="p-2 border-bottom">
        <input
          type="search"
          class="form-control form-control-sm"
          placeholder="Type to search…"
          value={query}
          oninput={onInput}
          autocomplete="off"
          aria-label="Search"
        />
      </div>
      <div class="rs-list" bind:this={listEl} onscroll={onScroll}>
        {#each options as opt (opt.value)}
          <button type="button" class="rs-item" onclick={() => choose(opt)}>
            {#if opt.avatar_url}
              <img src={opt.avatar_url} alt="" class="rs-avatar" />
            {/if}
            <span class="rs-item-text">
              <span class="rs-item-label">{opt.label}</span>
              {#if opt.sublabel}<span class="rs-item-sub">{opt.sublabel}</span>{/if}
            </span>
          </button>
        {/each}
        {#if loading}
          <div class="rs-note text-muted">Loading…</div>
        {:else if options.length === 0}
          <div class="rs-note text-muted">No matches</div>
        {:else if hasMore}
          <div class="rs-note text-muted">Scroll for more…</div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .ferum-rs { position: relative; display: inline-block; min-width: 180px; }
  .rs-control { padding-right: 1.75rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rs-value { display: inline-block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; vertical-align: bottom; }
  .rs-clear {
    position: absolute; right: 1.4rem; top: 50%; transform: translateY(-50%);
    border: 0; background: transparent; cursor: pointer; color: #888;
    padding: 0 .25rem; line-height: 1; z-index: 2;
  }
  .rs-clear:hover { color: #dc3545; }
  .rs-pop {
    position: absolute; z-index: 1056; top: calc(100% + 2px); left: 0;
    min-width: 240px; max-width: 320px;
  }
  .rs-list { max-height: 260px; overflow-y: auto; }
  .rs-item {
    display: flex; align-items: center; gap: .5rem; width: 100%;
    border: 0; background: transparent; text-align: left;
    padding: .4rem .6rem; cursor: pointer; min-height: 40px;
  }
  .rs-item:hover { background: rgba(99,102,241,0.1); }
  .rs-avatar { width: 24px; height: 24px; border-radius: 50%; object-fit: cover; flex: 0 0 auto; }
  .rs-item-text { display: flex; flex-direction: column; min-width: 0; }
  .rs-item-label { font-size: .85rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rs-item-sub { font-size: .72rem; color: #888; }
  .rs-note { padding: .5rem .6rem; font-size: .8rem; text-align: center; }
</style>
