<script lang="ts">
	interface Props {
		variant?: 'primary' | 'secondary' | 'danger' | 'outline-primary' | 'outline-secondary' | 'link';
		size?: 'sm' | 'md' | 'lg';
		disabled?: boolean;
		loading?: boolean;
		type?: 'button' | 'submit' | 'reset';
		class?: string;
		onclick?: (e: MouseEvent) => void;
	}

	let {
		variant = 'primary',
		size = 'md',
		disabled = false,
		loading = false,
		type = 'button',
		class: extraClass = '',
		onclick,
		children
	}: Props & { children?: any } = $props();

	const sizeClass = size === 'sm' ? 'btn-sm' : size === 'lg' ? 'btn-lg' : '';
</script>

<button
	{type}
	class="btn btn-{variant} {sizeClass} {extraClass}"
	disabled={disabled || loading}
	{onclick}
>
	{#if loading}
		<span class="spinner-border spinner-border-sm me-2" role="status" aria-hidden="true"></span>
	{/if}
	{@render children?.()}
</button>
