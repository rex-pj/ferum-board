<script lang="ts">
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';

	interface Thread {
		id: string;
		title: string;
		slug: string;
		category_id: string;
		author_id: string;
		author?: { username: string; display_name?: string; avatar_url?: string; role?: string };
		category?: { name: string; slug: string };
		status: string;
		is_pinned: boolean;
		is_solved: boolean;
		reply_count: number;
		view_count: number;
		last_post_at?: string;
		created_at: string;
		excerpt?: string | null;
		thumbnail_url?: string | null;
	}

	interface Props {
		thread: Thread;
		class?: string;
	}

	let { thread, class: extraClass = '' }: Props = $props();
</script>

<article class="fr-thread-row {extraClass}">
	<!-- Thumbnail -->
	{#if thread.thumbnail_url}
		<a href={ROUTES.THREAD(thread.slug)} class="flex-shrink-0" tabindex="-1" aria-hidden="true">
			<img
				src={thread.thumbnail_url}
				alt=""
				class="fr-thread-thumb"
				loading="lazy"
			/>
		</a>
	{:else if thread.author}
		<a href={ROUTES.USER_PROFILE(thread.author.username)} class="flex-shrink-0 mt-1" tabindex="-1">
			<Avatar src={thread.author.avatar_url} username={thread.author.username} size={36} />
		</a>
	{/if}

	<!-- Content -->
	<div class="fr-thread-body">
		<!-- Title row -->
		<div class="d-flex align-items-start gap-2">
			{#if thread.is_pinned || thread.is_solved || thread.status === 'locked'}
				<div class="fr-thread-flags mt-1">
					{#if thread.is_pinned}
						<i class="fa-solid fa-thumbtack fr-status-icon pinned" title="Pinned"></i>
					{/if}
					{#if thread.is_solved}
						<i class="fa-solid fa-circle-check fr-status-icon solved" title="Solved"></i>
					{/if}
					{#if thread.status === 'locked'}
						<i class="fa-solid fa-lock fr-status-icon locked" title="Locked"></i>
					{/if}
				</div>
			{/if}
			<a href={ROUTES.THREAD(thread.slug)} class="fr-thread-title">{thread.title}</a>
		</div>

		<!-- Excerpt -->
		{#if thread.excerpt}
			<p class="fr-thread-excerpt">{thread.excerpt}</p>
		{/if}

		<!-- Meta row -->
		<div class="fr-thread-meta">
			{#if thread.category}
				<a href={ROUTES.CATEGORY(thread.category.slug)} class="fr-category-pill">
					{thread.category.name}
				</a>
			{/if}
			{#if thread.author}
				<span>
					by <a
						href={ROUTES.USER_PROFILE(thread.author.username)}
						class="fw-semibold text-decoration-none author-link"
					>{thread.author.display_name ?? thread.author.username}</a>
				</span>
			{/if}
			<Timestamp date={thread.last_post_at ?? thread.created_at} />
		</div>
	</div>

	<!-- Stats: desktop only -->
	<div class="fr-thread-stats d-none d-sm-flex">
		<span title="Replies"><i class="fa-regular fa-comment me-1"></i>{thread.reply_count}</span>
		<span title="Views"><i class="fa-regular fa-eye me-1"></i>{thread.view_count}</span>
	</div>
</article>

<style>
	.fr-thread-thumb {
		width: 80px;
		height: 50px;
		object-fit: cover;
		border-radius: 4px;
		flex-shrink: 0;
		margin-top: 2px;
	}

	.author-link {
		color: inherit;
	}
</style>
