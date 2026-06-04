<svelte:head>
	<title>Permissions | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { ROUTES } from '$lib/routes';

	let { data }: { data: any } = $props();

	const GROUPS = ['content', 'moderation', 'admin'] as const;

	const GROUP_META: Record<string, { icon: string; description: string; color: string }> = {
		content: {
			icon: 'fa-pen-to-square',
			description: 'Controls who can create, edit, and delete threads and posts.',
			color: '#0d6efd'
		},
		moderation: {
			icon: 'fa-gavel',
			description: 'Controls who can review reports, warn, and ban users.',
			color: '#fd7e14'
		},
		admin: {
			icon: 'fa-screwdriver-wrench',
			description: 'Site-wide administrative capabilities: user management, config, and more.',
			color: '#dc3545'
		}
	};

	const TRUST_COLOR: Record<string, string> = {
		basic: 'info',
		member: 'success',
		regular: 'primary',
		leader: 'warning'
	};

	let search = $state('');
	let activeGroup = $state<string>('all');

	const allPerms: any[] = $derived(data.permissions ?? []);
	const totalCount = $derived(allPerms.length);
	const unassignedCount = $derived(
		allPerms.filter((p) => (data.permToRoles?.[p.key] ?? []).length === 0).length
	);

	const filtered = $derived(
		allPerms.filter((p) => {
			const matchesGroup = activeGroup === 'all' || p.group_name === activeGroup;
			if (!search.trim()) return matchesGroup;
			const q = search.toLowerCase();
			return matchesGroup && (p.key.includes(q) || p.description?.toLowerCase().includes(q));
		})
	);

	function permsForGroup(group: string) {
		return filtered.filter((p: any) => p.group_name === group);
	}

	function countForGroup(group: string) {
		return allPerms.filter((p: any) => p.group_name === group).length;
	}

	function rolesForPerm(key: string): any[] {
		const ids: string[] = data.permToRoles?.[key] ?? [];
		return (data.roles ?? []).filter((r: any) => ids.includes(r.id));
	}

	const visibleGroups = $derived(
		GROUPS.filter((g) => activeGroup === 'all' || g === activeGroup).filter(
			(g) => permsForGroup(g).length > 0
		)
	);
</script>

<!-- Header -->
<div class="d-flex align-items-start justify-content-between gap-3 mb-2 flex-wrap">
	<div>
		<h1 class="h5 mb-1">Permissions</h1>
		<p class="text-muted small mb-0">
			{totalCount} total
			{#if unassignedCount > 0}
				·
				<span class="text-warning-emphasis fw-semibold">
					<i class="fa-solid fa-triangle-exclamation me-1"></i>{unassignedCount} unassigned
				</span>
			{/if}
		</p>
	</div>
	<a href={ROUTES.ADMIN.ROLES} class="btn btn-outline-secondary btn-sm flex-shrink-0">
		<i class="fa-solid fa-shield-halved me-1"></i>Manage Roles
	</a>
</div>

<!-- Search + group filter -->
<div class="d-flex gap-2 flex-wrap align-items-center mb-4">
	<div class="input-group input-group-sm filter-search">
		<span class="input-group-text bg-transparent">
			<i class="fa-solid fa-magnifying-glass text-muted"></i>
		</span>
		<input
			type="search"
			class="form-control border-start-0"
			placeholder="Search by key or description…"
			bind:value={search}
		/>
		{#if search}
			<button class="btn btn-outline-secondary" onclick={() => (search = '')}>
				<i class="fa-solid fa-xmark"></i>
			</button>
		{/if}
	</div>

	<div class="btn-group btn-group-sm" role="group" aria-label="Filter by group">
		<button
			type="button"
			class="btn {activeGroup === 'all' ? 'btn-secondary' : 'btn-outline-secondary'}"
			onclick={() => (activeGroup = 'all')}
		>
			All <span class="badge bg-secondary ms-1">{totalCount}</span>
		</button>
		{#each GROUPS as group}
			{@const meta = GROUP_META[group]}
			<button
				type="button"
				class="btn text-capitalize {activeGroup === group
					? 'btn-secondary'
					: 'btn-outline-secondary'}"
				onclick={() => (activeGroup = group)}
			>
				<i class="fa-solid {meta.icon} me-1 btn-icon-sm"></i>{group}
				<span class="badge bg-secondary ms-1">{countForGroup(group)}</span>
			</button>
		{/each}
	</div>

	{#if search || activeGroup !== 'all'}
		<span class="text-muted small">{filtered.length} of {totalCount} shown</span>
	{/if}
</div>

<!-- Permission tables by group -->
{#if filtered.length === 0}
	<div class="card">
		<div class="card-body text-center py-5 text-muted">
			<i class="fa-solid fa-magnifying-glass mb-2 empty-icon"></i>
			<p class="mb-0">No permissions match your search.</p>
		</div>
	</div>
{:else}
	{#each visibleGroups as group}
		{@const meta = GROUP_META[group]}
		{@const perms = permsForGroup(group)}

		<div class="card mb-4">
			<div class="card-header py-2 d-flex align-items-center gap-2 flex-wrap">
				<i class="fa-solid {meta.icon}" style="color:{meta.color};"></i>
				<span class="fw-semibold text-capitalize">{group}</span>
				<span class="badge bg-secondary-subtle text-secondary-emphasis">{perms.length}</span>
				<span class="text-muted small d-none d-md-inline">{meta.description}</span>
			</div>
			<div class="table-responsive">
				<table class="table align-middle mb-0">
					<thead>
						<tr>
							<th class="col-key">Key</th>
							<th>Description</th>
							<th class="col-trust text-center">Min Trust</th>
							<th class="col-granted">Granted to</th>
						</tr>
					</thead>
					<tbody>
						{#each perms as perm}
							{@const grantedRoles = rolesForPerm(perm.key)}
							{@const isUnassigned = grantedRoles.length === 0}
							<tr class={isUnassigned ? 'table-warning' : ''}>
								<td><code class="small">{perm.key}</code></td>
								<td class="small text-muted">{perm.description}</td>
								<td class="text-center">
									{#if perm.min_trust && perm.min_trust !== 'new'}
										<span
											class="badge bg-{TRUST_COLOR[perm.min_trust] ?? 'secondary'}-subtle
											       text-{TRUST_COLOR[perm.min_trust] ?? 'secondary'}-emphasis"
										>
											{perm.min_trust}
										</span>
									{:else}
										<span class="text-muted small fst-italic">Any</span>
									{/if}
								</td>
								<td>
									{#if isUnassigned}
										<span class="text-warning-emphasis small">
											<i class="fa-solid fa-triangle-exclamation me-1"></i>no roles
										</span>
									{:else}
										<div class="d-flex flex-wrap gap-1">
											{#each grantedRoles as role}
												<a
													href={ROUTES.ADMIN.ROLE(role.id)}
													class="badge text-decoration-none"
													style={role.color ? `background:${role.color};` : 'background:#6c757d;'}
													title="Edit {role.name} permissions"
												>{role.name}</a>
											{/each}
										</div>
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</div>
	{/each}
{/if}

<style>
	.filter-search { max-width: 320px; }
	.btn-icon-sm   { font-size: 0.75rem; }
	.empty-icon    { font-size: 1.5rem; opacity: 0.35; }
	.col-key       { width: 28%; }
	.col-trust     { width: 10%; }
	.col-granted   { width: 26%; }
</style>
