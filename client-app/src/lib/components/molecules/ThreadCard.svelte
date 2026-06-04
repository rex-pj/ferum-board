<script lang="ts">
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';
	import { safeCssColor } from '$lib/utils/format';

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
		tags?: { id: string; name: string; slug: string; color?: string | null }[];
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

			<!-- Tags (max 2 visible) -->
			{#if thread.tags && thread.tags.length > 0}
				<div class="fr-thread-meta-tags">
					{#each thread.tags.slice(0, 2) as tag}
						<a
							href="{ROUTES.SEARCH}?tag={tag.slug}"
							class="fr-tag-chip"
							style="--tag-color:{safeCssColor(tag.color)};"
						>{tag.name}</a>
					{/each}
					{#if thread.tags.length > 2}
						<span class="fr-tag-more">+{thread.tags.length - 2}</span>
					{/if}
				</div>
			{/if}
		</div>
	</div>

	<!-- Stats: desktop only -->
	<div class="fr-thread-stats d-none d-sm-flex">
		<span title="{thread.reply_count} {thread.reply_count === 1 ? 'reply' : 'replies'}">
			<i class="fa-regular fa-comment me-1"></i>{thread.reply_count}
		</span>
		<span title="{thread.view_count.toLocaleString()} views">
			<i class="fa-regular fa-eye me-1"></i>{thread.view_count}
		</span>
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

	/* Compact tag chips inline with metadata */
	.fr-thread-meta-tags {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
	}

	.fr-tag-chip {
		display: inline-flex;
		align-items: center;
		padding: 0 0.375rem;
		height: 18px;
		border-radius: 3px;
		font-size: 0.6875rem;
		font-weight: 500;
		background: color-mix(in srgb, var(--tag-color) 12%, transparent);
		color: color-mix(in srgb, var(--tag-color) 80%, var(--bs-body-color));
		text-decoration: none;
		border: 1px solid color-mix(in srgb, var(--tag-color) 22%, var(--bs-border-color));
		transition: background 0.12s;
		white-space: nowrap;
		line-height: 1;
	}

	.fr-tag-chip:hover {
		background: color-mix(in srgb, var(--tag-color) 22%, transparent);
	}

	.fr-tag-more {
		font-size: 0.6875rem;
		color: var(--bs-tertiary-color);
		font-weight: 500;
		padding: 0 0.125rem;
	}
</style>
