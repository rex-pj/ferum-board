<svelte:options customElement={{ tag: "ferum-bookmark-btn", shadow: "none" }} />

<script lang="ts">
  import { getBookmarkStatus, toggleBookmark } from "./lib/bookmarks";
  import { t } from "./lib/i18n";

  let {
    "thread-id": threadId = "",
    bookmarked = "false",
    "user-id": userId = "",
  } = $props<{
    "thread-id"?: string;
    bookmarked?: string;
    "user-id"?: string;
  }>();

  let isBookmarked = $state(bookmarked === "true");
  let loading = $state(false);

  $effect(() => {
    if (!threadId || !userId) return;
    getBookmarkStatus(threadId).then((status) => {
      if (status != null) isBookmarked = status;
    });
  });

  async function toggle() {
    if (!userId) {
      window.location.href = "/login";
      return;
    }
    loading = true;
    const ok = await toggleBookmark(threadId, isBookmarked);
    if (ok) {
      isBookmarked = !isBookmarked;
    }
    loading = false;
  }
</script>

<button
  class="bookmark-btn {isBookmarked ? 'active' : ''}"
  onclick={toggle}
  disabled={loading}
  title={isBookmarked ? t("js-remove-bookmark") : t("js-bookmark-this-thread")}
>
  <i
    class={isBookmarked ? "fa-solid fa-bookmark" : "fa-regular fa-bookmark"}
    aria-hidden="true"
  ></i>
  <span class="label">{isBookmarked ? t("js-bookmarked") : t("js-bookmark")}</span>
</button>

<style>
  .bookmark-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 0.25rem 0.625rem;
    min-height: 36px;
    border-radius: var(--bs-border-radius-sm, 0.25rem);
    border: 1px solid var(--bs-border-color, #dee2e6);
    background: transparent;
    cursor: pointer;
    font-size: 0.8125rem;
    line-height: 1.5;
    color: var(--bs-body-color, inherit);
    transition: all 0.15s;
  }
  .bookmark-btn:hover {
    border-color: var(--bs-secondary-color, #6c757d);
    background: var(--bs-secondary-bg, rgba(0, 0, 0, 0.05));
  }
  .bookmark-btn.active {
    border-color: var(--ferum-primary, #6366f1);
    background: rgba(99, 102, 241, 0.1);
    color: var(--ferum-primary, #6366f1);
  }
  .bookmark-btn:disabled {
    opacity: 0.6;
    cursor: wait;
  }
  @media (max-width: 767.98px) {
    .bookmark-btn {
      min-height: 44px;
      padding: 0.5rem 1rem;
    }
  }
</style>
