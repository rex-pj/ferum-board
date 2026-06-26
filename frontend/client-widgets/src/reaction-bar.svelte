<svelte:options customElement={{ tag: "ferum-reaction-bar", shadow: "none" }} />

<script lang="ts">
  import { toggleReaction } from "./lib/reactions";

  const KINDS = ["like", "helpful", "insightful", "funny"] as const;
  const ICONS: Record<string, string> = {
    like: "fa-solid fa-thumbs-up",
    helpful: "fa-solid fa-lightbulb",
    insightful: "fa-solid fa-eye",
    funny: "fa-solid fa-face-laugh",
  };

  let {
    "post-id": postId = "",
    reactions = "{}",
    "user-id": userId = "",
  } = $props<{
    "post-id"?: string;
    reactions?: string;
    "user-id"?: string;
  }>();

  type ReactionMap = Record<string, { count: number; reacted: boolean }>;

  let state = $state<ReactionMap>(
    (() => {
      try {
        const parsed = JSON.parse(reactions);
        const map: ReactionMap = {};
        for (const kind of KINDS) {
          map[kind] = {
            count: parsed[kind]?.count ?? 0,
            reacted: parsed[kind]?.reacted ?? false,
          };
        }
        return map;
      } catch {
        return Object.fromEntries(
          KINDS.map((k) => [k, { count: 0, reacted: false }]),
        );
      }
    })(),
  );

  let errorMsg = $state('');
  let errorTimer = 0;

  function showError(msg: string) {
    errorMsg = msg;
    clearTimeout(errorTimer);
    errorTimer = window.setTimeout(() => { errorMsg = ''; }, 3000);
  }

  async function toggle(kind: string) {
    if (!userId) {
      window.location.href = "/login";
      return;
    }
    const item = state[kind];
    const result = await toggleReaction(postId, kind, item.reacted);
    if (result.ok) {
      state[kind] = {
        count: item.count + (item.reacted ? -1 : 1),
        reacted: !item.reacted,
      };
    } else {
      const MESSAGES: Record<string, string> = {
        cannot_react_to_own_post: "You can't react to your own post.",
        trust_level_insufficient: "Your account needs to be verified to react.",
        account_suspended:        "Your account is suspended.",
      };
      showError(MESSAGES[result.code] ?? result.message);
    }
  }
</script>

<div class="ferum-reactions">
  {#each KINDS as kind}
    <button
      class="reaction-btn {state[kind].reacted ? 'reacted' : ''}"
      onclick={() => toggle(kind)}
      title={kind}
    >
      <i class={ICONS[kind]} aria-hidden="true"></i>
      {#if state[kind].count > 0}
        <span class="count">{state[kind].count}</span>
      {/if}
    </button>
  {/each}
  {#if errorMsg}
    <span class="reaction-error">{errorMsg}</span>
  {/if}
</div>

<style>
  .ferum-reactions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }
  .reaction-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 0 10px;
    height: 32px;
    border-radius: 999px;
    border: 1px solid var(--bs-border-color, #dee2e6);
    background: transparent;
    color: var(--bs-secondary-color, #6c757d);
    cursor: pointer;
    font-size: 0.8125rem;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .reaction-btn:hover {
    border-color: var(--ferum-primary, #6366f1);
    background: rgba(99, 102, 241, 0.05);
    color: var(--ferum-primary, #6366f1);
  }
  .reaction-btn.reacted {
    border-color: var(--ferum-primary, #6366f1);
    background: rgba(99, 102, 241, 0.1);
    color: var(--ferum-primary, #6366f1);
    font-weight: 600;
  }
  .count {
    font-size: 0.75rem;
    font-weight: 600;
  }
  .reaction-error {
    font-size: 0.75rem;
    color: #dc3545;
    align-self: center;
  }

  @media (max-width: 767.98px) {
    .reaction-btn {
      height: 44px;
      padding: 0 14px;
    }
  }
</style>
