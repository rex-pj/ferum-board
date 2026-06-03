<script lang="ts">
	import { toast } from '$lib/stores/toast';
	import ToastItem from '$lib/components/atoms/ToastItem.svelte';
</script>

<div
	class="toast-container position-fixed bottom-0 end-0 p-3 fr-toast-container"
	aria-label="Notifications"
>
	{#each $toast as item (item.id)}
		<ToastItem {item} />
	{/each}
</div>

<style>
	.fr-toast-container {
		z-index: 1100;
	}
</style>
