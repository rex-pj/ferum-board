<script lang="ts">
	import { ROUTES } from '$lib/routes';

	interface Props {
		name: string;
		slug: string;
		color?: string | null;
		class?: string;
	}

	let { name, slug, color, class: extraClass = '' }: Props = $props();

	const style = color ? `background-color:${color};color:#fff;` : '';
</script>

<a
	href={ROUTES.SEARCH + '?tag=' + slug}
	class="badge rounded-pill text-decoration-none {extraClass}"
	style={style || 'background-color:var(--bs-secondary-bg);color:var(--bs-body-color);'}
>
	{name}
</a>
