<script lang="ts">
	import { enhance } from '$app/forms';
	import ReactionBar from '$lib/components/molecules/ReactionBar.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import UserMeta from '$lib/components/molecules/UserMeta.svelte';
	import PostComposer from '$lib/components/organisms/PostComposer.svelte';
	import { toast } from '$lib/stores/toast';

	type ReactionKind = 'like' | 'helpful' | 'insightful' | 'funny';

	interface Post {
		id: string;
		content_html: string;
		content_md?: string;
		parent_id?: string;
		author?: { username: string; display_name?: string; avatar_url?: string; role?: string };
		is_deleted: boolean;
		edited_at?: string;
		edit_count: number;
		created_at: string;
		reactions?: { kind: ReactionKind; count: number }[];
		my_reactions?: ReactionKind[];
	}

	interface Props {
		post: Post;
		isBestAnswer?: boolean;
		loggedIn?: boolean;
		currentUsername?: string;
		myReactions?: ReactionKind[];
		onReport?: (postId: string) => void;
		onQuote?: (post: Post) => void;
		canMarkBestAnswer?: boolean;
		threadId?: string;
		class?: string;
	}

	let {
		post,
		isBestAnswer = false,
		loggedIn = false,
		currentUsername,
		myReactions = [],
		onReport,
		onQuote,
		canMarkBestAnswer = false,
		threadId,
		class: extraClass = ''
	}: Props = $props();

	let localCounts = $state<{ kind: ReactionKind; count: number }[]>(post.reactions ?? []);
	let localMyReactions = $state<ReactionKind[]>(myReactions);

	async function handleReact(kind: ReactionKind) {
		if (!loggedIn) return;
		const isActive = localMyReactions.includes(kind);

		// Snapshot for rollback
		const prevCounts = localCounts;
		const prevReactions = localMyReactions;

		// Optimistic update
		if (isActive) {
			localMyReactions = localMyReactions.filter((k) => k !== kind);
			localCounts = localCounts.map((c) =>
				c.kind === kind ? { ...c, count: Math.max(0, c.count - 1) } : c
			);
		} else {
			localMyReactions = [...localMyReactions, kind];
			const existing = localCounts.find((c) => c.kind === kind);
			if (existing) {
				localCounts = localCounts.map((c) =>
					c.kind === kind ? { ...c, count: c.count + 1 } : c
				);
			} else {
				localCounts = [...localCounts, { kind, count: 1 }];
			}
		}

		// Sync with server — rollback on failure
		try {
			const res = isActive
				? await fetch(`/api/posts/${post.id}/reactions/${kind}`, { method: 'DELETE' })
				: await fetch(`/api/posts/${post.id}/reactions`, {
						method: 'POST',
						headers: { 'Content-Type': 'application/json' },
						body: JSON.stringify({ kind })
					});
			if (!res.ok) {
				localCounts = prevCounts;
				localMyReactions = prevReactions;
				toast.error('Failed to save reaction. Please try again.');
			}
		} catch {
			localCounts = prevCounts;
			localMyReactions = prevReactions;
			toast.error('Network error. Check your connection and try again.');
		}
	}

	// Must match POST_EDIT_WINDOW_HOURS in backend/crates/ferum-application/src/constants.rs.
	// This is a UI-only gate; the backend enforces the real window on every PATCH request.
	const EDIT_WINDOW_MS = 24 * 60 * 60 * 1000;
	const canEdit = $derived(
		loggedIn &&
		!!currentUsername &&
		post.author?.username === currentUsername &&
		Date.now() - new Date(post.created_at).getTime() < EDIT_WINDOW_MS
	);

	let editMode = $state(false);
	let editDraft = $state('');
	let editSaving = $state(false);
	let displayHtml = $state(post.content_html);

	// Replace @username with profile links in rendered HTML text (not inside tags/attributes).
	function linkifyMentions(html: string): string {
		return html.replace(
			/(?<![<"'\w])@([a-zA-Z0-9_]{3,32})(?=[\s<,.!?)]|$)/g,
			'<a href="/u/$1">@$1</a>'
		);
	}

	// Inject a jump-to-original link inside the first blockquote (quoted reply).
	// Only replaces the first occurrence so nested blockquotes are unaffected.
	function injectQuoteJumpLink(html: string, parentId: string): string {
		return html.replace(
			'<blockquote>',
			`<blockquote><a href="#post-${parentId}" class="quote-jump"><i class="fa-solid fa-arrow-turn-up fa-xs me-1"></i>View original post</a>`
		);
	}

	const renderedHtml = $derived(
		post.parent_id
			? injectQuoteJumpLink(linkifyMentions(displayHtml), post.parent_id)
			: linkifyMentions(displayHtml)
	);

	function startEdit() {
		editDraft = post.content_md ?? '';
		editMode = true;
	}

	async function saveEdit() {
		if (!editDraft.trim()) return;
		editSaving = true;
		try {
			const res = await fetch(`/api/posts/${post.id}`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ content_md: editDraft })
			});
			if (res.ok) {
				const json = await res.json();
				displayHtml = json.data?.content_html ?? displayHtml;
				editMode = false;
				toast.success('Post updated.');
			} else {
				const body = await res.json().catch(() => ({}));
				toast.error(body?.error?.message ?? 'Failed to save changes.');
			}
		} catch {
			toast.error('Network error. Please try again.');
		} finally {
			editSaving = false;
		}
	}
</script>

<div
	class="post-body mb-3 {isBestAnswer ? 'best-answer-post' : ''} {extraClass}"
	id="post-{post.id}"
>
	{#if post.is_deleted}
		<div class="text-muted fst-italic small py-2">[This post has been deleted]</div>
	{:else}
		{#if post.author}
			<div class="d-flex justify-content-between align-items-start mb-2">
				<UserMeta
					username={post.author.username}
					displayName={post.author.display_name}
					avatarUrl={post.author.avatar_url}
					role={post.author.primary_role_slug}
					date={post.created_at}
				/>
				{#if isBestAnswer}
					<span class="badge bg-success">
						<i class="fa-solid fa-check me-1"></i>Best Answer
					</span>
				{/if}
			</div>
		{/if}

		{#if editMode}
			<div class="mb-3">
				<PostComposer
					bind:value={editDraft}
					placeholder="Edit your post…"
					disabled={editSaving}
					class="mb-2"
				/>
				<div class="d-flex gap-2 mt-2">
					<button
						type="button"
						class="btn btn-primary btn-sm edit-btn"
						disabled={editSaving || !editDraft.trim()}
						onclick={saveEdit}
					>
						{#if editSaving}
							<span class="spinner-border spinner-border-sm me-1"></span>
						{/if}
						Save
					</button>
					<button
						type="button"
						class="btn btn-outline-secondary btn-sm edit-btn"
						disabled={editSaving}
						onclick={() => (editMode = false)}
					>
						Cancel
					</button>
				</div>
			</div>
		{:else}
			<div class="post-content prose">
				<!-- eslint-disable-next-line svelte/no-at-html-tags -->
				{@html renderedHtml}
			</div>
		{/if}

		{#if post.edit_count > 0 && post.edited_at}
			<div class="text-muted small mt-2">
				<i class="fa-solid fa-pen-to-square me-1"></i>edited <Timestamp date={post.edited_at} />
			</div>
		{/if}

		<div class="d-flex align-items-center justify-content-between mt-3 pt-2 border-top gap-2">
			<ReactionBar
				postId={post.id}
				counts={localCounts}
				myReactions={localMyReactions}
				{loggedIn}
				onReact={handleReact}
			/>
			<div class="d-flex align-items-center gap-1 flex-wrap justify-content-end">
				{#if canMarkBestAnswer && threadId}
					<form
						method="POST"
						action="?/markBestAnswer"
						use:enhance={() => {
							return async ({ update }) => { await update(); };
						}}
					>
						<input type="hidden" name="thread_id" value={threadId} />
						<input type="hidden" name="post_id" value={post.id} />
						<button type="submit" class="btn btn-outline-success btn-sm">
							<i class="fa-solid fa-circle-check me-1"></i>Mark as Best Answer
						</button>
					</form>
				{/if}
				{#if onQuote && !editMode}
					<button
						type="button"
						class="btn btn-link btn-sm text-muted text-decoration-none p-2"
						title="Quote this post"
						aria-label="Quote post"
						onclick={() => onQuote?.(post)}
					>
						<i class="fa-solid fa-quote-left fa-sm"></i>
					</button>
				{/if}
				{#if canEdit && !editMode}
					<button
						type="button"
						class="btn btn-link btn-sm text-muted text-decoration-none p-2"
						title="Edit this post"
						aria-label="Edit post"
						onclick={startEdit}
					>
						<i class="fa-solid fa-pen fa-sm"></i>
					</button>
				{/if}
				{#if loggedIn}
					<button
						type="button"
						class="btn btn-link btn-sm text-muted text-decoration-none p-2"
						title="Report this post"
						aria-label="Report post"
						onclick={() => onReport?.(post.id)}
					>
						<i class="fa-solid fa-flag fa-sm"></i>
					</button>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.post-body {
		background: var(--bs-body-bg);
		border: 1px solid var(--bs-border-color);
		border-radius: var(--bs-border-radius);
		padding: 1rem 1.25rem;
	}

	.best-answer-post {
		border-left: 3px solid var(--bs-success);
	}

	.post-content :global(pre) {
		background: var(--bs-tertiary-bg);
		border-radius: 4px;
		padding: 1rem;
		overflow-x: auto;
	}
	.post-content :global(blockquote) {
		border-left: 3px solid var(--bs-border-color);
		padding-left: 1rem;
		color: var(--bs-secondary-color);
	}
	.post-content :global(img) {
		max-width: 100%;
		height: auto;
	}
	.post-content :global(a) {
		word-break: break-word;
	}

	/* Jump-to-original link injected before blockquote content */
	.post-content :global(.quote-jump) {
		display: inline-flex;
		align-items: center;
		font-size: 0.75rem;
		color: var(--bs-secondary-color);
		text-decoration: none;
		padding: 0.125rem 0.5rem;
		border-radius: var(--bs-border-radius-sm);
		margin-bottom: 0.375rem;
		background: var(--bs-secondary-bg);
		transition: color 0.1s, background 0.1s;
	}
	.post-content :global(.quote-jump:hover) {
		color: var(--bs-body-color);
		background: var(--bs-border-color);
	}

	/* Highlight the post when it is the :target of a fragment navigation */
	:global(.post-body:target) {
		border-color: var(--bs-primary);
		box-shadow: 0 0 0 3px rgba(var(--bs-primary-rgb), 0.15);
		transition: border-color 0.2s, box-shadow 0.2s;
	}

	.edit-btn {
		min-height: var(--fr-tap-target);
	}
</style>
