<svelte:options customElement={{ tag: "ferum-notification-bell", shadow: "none" }} />

<script lang="ts">
  import { getUnreadCount, openNotificationStream } from './lib/notifications';

  let {
    'user-id': userId = '',
    'initial-count': initialCount = '0',
  } = $props<{
    'user-id'?: string;
    'initial-count'?: string;
  }>();

  // Seed from SSR so the badge is correct before the first API call resolves.
  let count = $state(parseInt(initialCount, 10) || 0);
  let pollTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (!userId) return;

    // Refresh immediately in case the SSR value is stale (page was cached).
    getUnreadCount().then((n) => { count = n; });

    const close = openNotificationStream({
      onNotification: () => { count += 1; },
      onDisconnect: () => { pollTimer = setTimeout(poll, 30_000); },
    });

    return () => {
      close();
      if (pollTimer !== null) { clearTimeout(pollTimer); pollTimer = null; }
    };
  });

  async function poll() {
    if (!userId) return;
    count = await getUnreadCount();
    pollTimer = setTimeout(poll, 30_000);
  }
</script>

<a href="/notifications" class="bell-link" title="Notifications" aria-label="Notifications">
  <i class="fa-solid fa-bell bell-icon" aria-hidden="true"></i>
  {#if count > 0}
    <span class="badge" aria-label="{count} unread">{count > 99 ? '99+' : count}</span>
  {/if}
</a>

<style>
  .bell-link {
    position: relative; display: inline-flex; align-items: center;
    text-decoration: none; min-height: 44px; min-width: 44px;
    justify-content: center;
  }
  .bell-icon { font-size: 1.2rem; }
  .badge {
    position: absolute; top: 4px; right: 2px;
    background: #dc3545; color: #fff; font-size: 0.65rem;
    padding: 2px 5px; border-radius: 10px; min-width: 18px;
    text-align: center; line-height: 1.2;
  }
</style>
