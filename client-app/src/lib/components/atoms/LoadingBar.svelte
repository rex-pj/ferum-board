<script lang="ts">
	import { navigating } from '$app/stores';

	// When navigation completes, flash the bar to 100% before hiding.
	let show = $state(false);
	let done = $state(false);
	let timer: ReturnType<typeof setTimeout>;

	$effect(() => {
		if ($navigating) {
			clearTimeout(timer);
			done = false;
			show = true;
		} else if (show) {
			done = true;
			timer = setTimeout(() => {
				show = false;
				done = false;
			}, 250);
		}
	});
</script>

{#if show}
	<div
		class="lb"
		class:lb--done={done}
		role="progressbar"
		aria-label="Loading page"
		aria-busy={!done}
	></div>
{/if}

<style>
	.lb {
		position: fixed;
		top: 0;
		left: 0;
		height: 3px;
		z-index: 9999;
		pointer-events: none;
		background: var(--bs-primary, #6366f1);
		animation: lb-grow 10s ease-out forwards;
	}

	.lb--done {
		animation: lb-done 0.25s ease-out forwards;
	}

	@keyframes lb-grow {
		0%   { width: 0%;  opacity: 1; }
		20%  { width: 50%; }
		60%  { width: 75%; }
		100% { width: 88%; }
	}

	@keyframes lb-done {
		from { width: 88%; opacity: 1; }
		to   { width: 100%; opacity: 0; }
	}
</style>
