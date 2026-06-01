<script lang="ts">
	import { formatRelative, formatDateTime } from '$lib/utils/format';

	interface Props {
		date: string | Date;
		class?: string;
	}

	let { date, class: extraClass = '' }: Props = $props();

	const relative = $derived(formatRelative(date));
	const absolute = $derived(formatDateTime(date));
</script>

<time
	datetime={typeof date === 'string' ? date : date.toISOString()}
	title={absolute}
	class="text-muted small {extraClass}"
>
	{relative}
</time>
