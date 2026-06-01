<script lang="ts">
	interface Props {
		kind: 'like' | 'helpful' | 'insightful' | 'funny';
		count: number;
		active?: boolean;
		disabled?: boolean;
		onclick?: () => void;
	}

	const ICONS: Record<string, string> = {
		like: 'fa-thumbs-up',
		helpful: 'fa-lightbulb',
		insightful: 'fa-eye',
		funny: 'fa-face-laugh'
	};

	const LABELS: Record<string, string> = {
		like: 'Like',
		helpful: 'Helpful',
		insightful: 'Insightful',
		funny: 'Funny'
	};

	let { kind, count, active = false, disabled = false, onclick }: Props = $props();
</script>

<button
	type="button"
	class="btn btn-sm d-inline-flex align-items-center gap-1 reaction-btn"
	class:active
	class:btn-outline-primary={active}
	class:btn-outline-secondary={!active}
	{disabled}
	title={LABELS[kind]}
	aria-label="{LABELS[kind]} ({count})"
	aria-pressed={active}
	{onclick}
>
	<i class="fa-solid {ICONS[kind]} fa-sm"></i>
	{#if count > 0}
		<span class="small">{count}</span>
	{/if}
</button>

<style>
	.reaction-btn {
		min-height: 32px;
		padding: 0.25rem 0.5rem;
	}
</style>
