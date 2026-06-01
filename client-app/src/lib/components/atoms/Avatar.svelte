<script lang="ts">
	interface Props {
		src?: string | null;
		username: string;
		size?: number;
		class?: string;
	}

	let { src, username, size = 36, class: extraClass = '' }: Props = $props();

	let imgError = $state(false);

	$effect(() => {
		// Reset error flag whenever the src changes so a newly uploaded avatar shows
		src;
		imgError = false;
	});

	const initials = username
		.split(/[\s_-]+/)
		.map((w) => w[0]?.toUpperCase() ?? '')
		.slice(0, 2)
		.join('');

	const hue = username.split('').reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360;
	const bg = `hsl(${hue}, 60%, 45%)`;

	const style = `width:${size}px;height:${size}px;font-size:${Math.round(size * 0.4)}px;`;
</script>

{#if src && !imgError}
	<img
		{src}
		alt="{username}'s avatar"
		width={size}
		height={size}
		class="rounded-circle object-fit-cover {extraClass}"
		style={style}
		onerror={() => { imgError = true; }}
	/>
{:else}
	<span
		class="rounded-circle d-inline-flex align-items-center justify-content-center text-white fw-semibold {extraClass}"
		style="{style}background:{bg};"
		title={username}
		aria-label="{username}'s avatar"
	>
		{initials}
	</span>
{/if}
