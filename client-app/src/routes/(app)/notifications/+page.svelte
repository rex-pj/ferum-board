<svelte:head>
	<title>Notifications | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();

	let markingAll = $state(false);
	let readingId = $state<string | null>(null);

	const unread = $derived((data.notifications ?? []).filter((n: any) => !n.is_read).length);
	const total  = $derived((data.notifications ?? []).length);

	const KIND_ICONS: Record<string, string> = {
		reply: 'fa-reply',
		mention: 'fa-at',
		reaction: 'fa-thumbs-up',
		best_answer: 'fa-circle-check',
		warn: 'fa-triangle-exclamation',
		system: 'fa-bell'
	};

	const KIND_LABELS: Record<string, string> = {
		reply: 'New reply in your thread',
		mention: 'You were mentioned',
		reaction: 'Someone reacted to your post',
		best_answer: 'Your post was marked as Best Answer',
		warn: 'You received a warning',
		system: 'System notification'
	};
</script>

<div class="fr-content-layout">
	<!-- Notifications feed -->
	<div class="fr-feed-col">
		<div class="d-flex justify-content-between align-items-center mb-3">
			<h1 class="h5 fw-semibold mb-0">Notifications</h1>
			{#if data.notifications?.length > 0}
				<form method="POST" action="?/markAllRead" use:enhance={() => {
					markingAll = true;
					return async ({ result, update }) => {
						markingAll = false;
						if (result.type === 'success') toast.success('All notifications marked as read.');
						await update();
					};
				}}>
					<button type="submit" class="btn btn-outline-secondary btn-sm" disabled={markingAll}>
						{#if markingAll}
							<span class="spinner-border spinner-border-sm me-1"></span>
						{:else}
							<i class="fa-solid fa-check-double me-1"></i>
						{/if}
						Mark all read
					</button>
				</form>
			{/if}
		</div>

		{#if data.notifications?.length > 0}
			<div class="fr-feed">
				{#each data.notifications as notif}
					{@const threadHref = notif.payload?.thread_slug
						? ROUTES.THREAD(notif.payload.thread_slug) + (notif.payload?.post_id ? `#post-${notif.payload.post_id}` : '')
						: null}
					<div class="fr-notif-row" class:fr-notif-unread={!notif.is_read}>
						<div class="fr-notif-icon-wrap">
							<i class="fa-solid {KIND_ICONS[notif.kind] ?? 'fa-bell'}"></i>
						</div>
						{#if threadHref}
							<a href={threadHref} class="flex-grow-1 min-w-0 text-decoration-none notif-link">
								<div class="fw-medium notif-title">
									{KIND_LABELS[notif.kind] ?? notif.kind}
								</div>
								{#if notif.payload?.message}
									<div class="notif-body">{notif.payload.message}</div>
								{/if}
								<div class="mt-1 notif-time"><Timestamp date={notif.created_at} /></div>
							</a>
						{:else}
							<div class="flex-grow-1 min-w-0">
								<div class="fw-medium notif-title">
									{KIND_LABELS[notif.kind] ?? notif.kind}
								</div>
								{#if notif.payload?.message}
									<div class="notif-body">{notif.payload.message}</div>
								{/if}
								{#if notif.payload?.reason}
									<div class="notif-body">Reason: {notif.payload.reason}</div>
								{/if}
								<div class="mt-1 notif-time"><Timestamp date={notif.created_at} /></div>
							</div>
						{/if}
						{#if !notif.is_read}
							<form method="POST" action="?/markRead" use:enhance={() => {
								readingId = notif.id;
								return async ({ update }) => {
									readingId = null;
									await update();
								};
							}} class="flex-shrink-0">
								<input type="hidden" name="id" value={notif.id} />
								<button type="submit" class="btn btn-sm fr-notif-read-btn" title="Mark as read" disabled={readingId === notif.id}>
									{#if readingId === notif.id}
										<span class="spinner-border spinner-border-sm spinner-xs"></span>
									{:else}
										<i class="fa-solid fa-check"></i>
									{/if}
								</button>
							</form>
						{:else}
							<div class="fr-notif-read-dot" title="Read"></div>
						{/if}
					</div>
				{/each}
			</div>
		{:else}
			<div class="fr-feed">
				<div class="fr-empty-state">
					<div class="fr-empty-icon">
						<i class="fa-regular fa-bell"></i>
					</div>
					<h2 class="fr-empty-title">You're all caught up!</h2>
					<p class="fr-empty-sub">New replies, reactions, and mentions will show up here.</p>
				</div>
			</div>
		{/if}
	</div>

	<!-- Right panel -->
	<aside class="fr-right-panel">
		<!-- Summary stats -->
		<div class="fr-panel">
			<div class="fr-panel-header">Summary</div>
			<div class="px-3 py-2">
				<div class="fr-panel-stat stat-no-border">
					<span class="text-muted stat-label">Unread</span>
					<span class="fw-semibold stat-value">
						{#if unread > 0}
							<span class="badge rounded-pill bg-primary">{unread}</span>
						{:else}
							0
						{/if}
					</span>
				</div>
				<div class="fr-panel-stat stat-top-border">
					<span class="text-muted stat-label">Total shown</span>
					<span class="fw-semibold stat-value">{total}</span>
				</div>
			</div>
		</div>

		<!-- Notification types legend -->
		<div class="fr-panel">
			<div class="fr-panel-header">Types</div>
			<div class="fr-panel-body">
				{#each Object.entries(KIND_LABELS) as [kind, label]}
					<div class="fr-panel-row kind-row">
						<i class="fa-solid {KIND_ICONS[kind]} text-primary kind-icon"></i>
						<span class="kind-label">{label}</span>
					</div>
				{/each}
			</div>
		</div>

		<!-- Quick links -->
		<div class="fr-panel">
			<div class="fr-panel-header">Account</div>
			<div class="fr-panel-body">
				<a href={ROUTES.ACCOUNT} class="fr-panel-row">
					<i class="fa-solid fa-user kind-icon"></i>
					Profile settings
				</a>
				<a href={ROUTES.BOOKMARKS} class="fr-panel-row">
					<i class="fa-solid fa-bookmark kind-icon"></i>
					Bookmarks
				</a>
				<a href={ROUTES.HOME} class="fr-panel-row">
					<i class="fa-solid fa-house kind-icon"></i>
					Back to feed
				</a>
			</div>
		</div>
	</aside>
</div>

<style>
	.notif-link  { color: inherit; }
	.notif-title { font-size: 0.875rem; color: var(--bs-body-color); }
	.notif-body  { font-size: 0.8125rem; color: var(--bs-secondary-color); margin-top: 0.125rem; }
	.notif-time  { font-size: 0.75rem; color: var(--bs-tertiary-color); }
	.spinner-xs  { width: 0.75rem; height: 0.75rem; }

	.stat-no-border  { border: none; padding: 0.5rem 0; }
	.stat-top-border { border-top: 1px solid var(--bs-border-color); padding: 0.5rem 0 0; }
	.stat-label  { color: var(--bs-secondary-color); font-size: 0.8125rem; }
	.stat-value  { font-size: 0.8125rem; color: var(--bs-body-color); }

	.kind-row  { gap: 0.625rem; cursor: default; }
	.kind-icon { width: 1rem; text-align: center; font-size: 0.75rem; }
	.kind-label { font-size: 0.8125rem; }
</style>
