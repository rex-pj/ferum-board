<script lang="ts">
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';

	interface Props {
		username: string;
		displayName?: string | null;
		avatarUrl?: string | null;
		role?: string;
		trustLevel?: string;
		date?: string | Date;
		avatarSize?: number;
		showBadge?: boolean;
		class?: string;
	}

	let {
		username,
		displayName,
		avatarUrl,
		role,
		trustLevel,
		date,
		avatarSize = 32,
		showBadge = true,
		class: extraClass = ''
	}: Props = $props();

	const name = displayName ?? username;
</script>

<div class="d-flex align-items-center gap-2 {extraClass}">
	<a href={ROUTES.USER_PROFILE(username)} class="flex-shrink-0">
		<Avatar src={avatarUrl} {username} size={avatarSize} />
	</a>
	<div class="d-flex flex-column lh-sm">
		<a href={ROUTES.USER_PROFILE(username)} class="fw-semibold text-decoration-none text-body link-body-emphasis">
			{name}
		</a>
		<div class="d-flex align-items-center gap-1 flex-wrap">
			{#if showBadge && role && role !== 'member'}
				<Badge role={role as any} />
			{/if}
			{#if date}
				<Timestamp {date} />
			{/if}
		</div>
	</div>
</div>
