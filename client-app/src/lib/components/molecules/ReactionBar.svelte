<script lang="ts">
	import ReactionButton from '$lib/components/atoms/ReactionButton.svelte';

	type ReactionKind = 'like' | 'helpful' | 'insightful' | 'funny';

	interface ReactionCount {
		kind: ReactionKind;
		count: number;
	}

	interface Props {
		postId: string;
		counts: ReactionCount[];
		myReactions?: ReactionKind[];
		loggedIn?: boolean;
		onReact?: (kind: ReactionKind) => void;
	}

	const ALL_KINDS: ReactionKind[] = ['like', 'helpful', 'insightful', 'funny'];

	let { postId, counts = [], myReactions = [], loggedIn = false, onReact }: Props = $props();

	function getCount(kind: ReactionKind) {
		return counts.find((c) => c.kind === kind)?.count ?? 0;
	}

	function isActive(kind: ReactionKind) {
		return myReactions.includes(kind);
	}

	async function handleClick(kind: ReactionKind) {
		if (!loggedIn || !onReact) return;
		onReact(kind);
	}
</script>

<div class="d-flex flex-wrap gap-1 reaction-bar" data-post-id={postId}>
	{#each ALL_KINDS as kind}
		{@const count = getCount(kind)}
		{@const active = isActive(kind)}
		{#if count > 0 || loggedIn}
			<ReactionButton
				{kind}
				{count}
				{active}
				disabled={!loggedIn}
				onclick={() => handleClick(kind)}
			/>
		{/if}
	{/each}
</div>
