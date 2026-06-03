<script lang="ts">
	interface Props {
		role?: string;
		trust?: 'new' | 'basic' | 'member' | 'regular' | 'leader';
		label?: string;
		class?: string;
	}

	let { role, trust, label, class: extraClass = '' }: Props = $props();

	const roleColors: Record<string, string> = {
		admin: 'danger',
		moderator: 'warning',
		member: 'secondary'
	};

	const trustColors: Record<string, string> = {
		leader: 'primary',
		regular: 'info',
		member: 'success',
		basic: 'secondary',
		new: 'light'
	};

	const badgeClass = role
		? `bg-${roleColors[role] ?? 'secondary'}`
		: trust
			? `bg-${trustColors[trust] ?? 'secondary'}`
			: 'bg-secondary';

	const badgeLabel = label ?? role ?? trust ?? '';
</script>

<span class="badge {badgeClass} text-capitalize {extraClass}">{badgeLabel}</span>
