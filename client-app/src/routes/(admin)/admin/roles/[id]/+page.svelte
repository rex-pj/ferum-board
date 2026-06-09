<svelte:head>
	<title>{data.role?.name ?? 'Role'} Permissions | Admin | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();

	const originalKeys = new Set<string>(data.assignedKeys ?? []);
	let selected = $state<Set<string>>(new Set(data.assignedKeys ?? []));
	let saving = $state(false);

	$effect(() => {
		if (form?.success) toast.success(form.success);
		if (form?.error) toast.error(form.error);
	});

	const hasChanges = $derived(
		selected.size !== originalKeys.size ||
			[...selected].some((k) => !originalKeys.has(k)) ||
			[...originalKeys].some((k) => !selected.has(k))
	);

	const addedCount = $derived([...selected].filter((k) => !originalKeys.has(k)).length);
	const removedCount = $derived([...originalKeys].filter((k) => !selected.has(k)).length);
	const changeCount = $derived(addedCount + removedCount);

	function toggle(key: string) {
		const next = new Set(selected);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		selected = next;
	}

	function selectAll(group: string) {
		const next = new Set(selected);
		for (const p of permsForGroup(group)) next.add(p.key);
		selected = next;
	}

	function clearGroup(group: string) {
		const next = new Set(selected);
		for (const p of permsForGroup(group)) next.delete(p.key);
		selected = next;
	}

	function discardChanges() {
		selected = new Set(originalKeys);
	}

	const GROUPS = ['content', 'moderation', 'admin'] as const;

	const GROUP_META: Record<string, { icon: string; label: string }> = {
		content: { icon: 'fa-pen-to-square', label: 'Content' },
		moderation: { icon: 'fa-gavel', label: 'Moderation' },
		admin: { icon: 'fa-screwdriver-wrench', label: 'Admin' }
	};

	const TRUST_COLOR: Record<string, string> = {
		basic: 'info',
		member: 'success',
		regular: 'primary',
		leader: 'warning'
	};

	function permsForGroup(group: string) {
		return (data.allPermissions ?? []).filter((p: any) => p.group_name === group);
	}

	function selectedInGroup(group: string) {
		return permsForGroup(group).filter((p: any) => selected.has(p.key)).length;
	}

	function badgeStyle(role: any) {
		return role?.color ? `background:${role.color};` : 'background:#6c757d;';
	}
</script>

<!-- Header -->
<div class="d-flex align-items-center gap-2 mb-4 flex-wrap">
	<a href={ROUTES.ADMIN.ROLES} class="btn btn-sm btn-outline-secondary flex-shrink-0">
		<i class="fa-solid fa-arrow-left"></i>
	</a>
	<i class="fa-solid fa-key text-muted icon-md"></i>
	<h1 class="h5 mb-0">
		Permissions for
		{#if data.role}
			<span class="badge ms-1" style={badgeStyle(data.role)}>{data.role.name}</span>
		{:else}
			<span class="text-muted">role</span>
		{/if}
	</h1>
	{#if data.role?.is_system}
		<span class="badge bg-secondary-subtle text-secondary-emphasis">
			<i class="fa-solid fa-lock me-1 icon-xs"></i>system role
		</span>
	{/if}
	<div class="ms-auto d-flex align-items-center gap-2 flex-shrink-0">
		<span class="text-muted small">{selected.size} selected</span>
	</div>
</div>

<!-- Permission groups -->
<form method="POST" action="?/save" id="permissions-form">
	{#each [...selected] as key}
		<input type="hidden" name="perm" value={key} />
	{/each}

	{#each GROUPS as group}
		{@const meta = GROUP_META[group]}
		{@const perms = permsForGroup(group)}
		{@const selCount = selectedInGroup(group)}

		<div class="card mb-3">
			<div class="card-header d-flex align-items-center gap-2 py-2">
				<i class="fa-solid {meta.icon} text-muted icon-sm"></i>
				<span class="fw-semibold">{meta.label}</span>
				<span class="badge bg-secondary-subtle text-secondary-emphasis">
					{selCount}/{perms.length}
				</span>
				<div class="ms-auto d-flex gap-1">
					<button
						type="button"
						class="btn btn-outline-secondary btn-sm py-0 px-2 group-btn"
						onclick={() => selectAll(group)}
						disabled={selCount === perms.length}
					>
						All
					</button>
					<button
						type="button"
						class="btn btn-outline-secondary btn-sm py-0 px-2 group-btn"
						onclick={() => clearGroup(group)}
						disabled={selCount === 0}
					>
						None
					</button>
				</div>
			</div>
			<div class="card-body p-2">
				<div class="row g-0">
					{#each perms as perm}
						{@const isChecked = selected.has(perm.key)}
						<div class="col-12 col-lg-6">
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								class="d-flex align-items-start gap-3 rounded px-3 py-2 mx-1 my-1 cursor-pointer transition-all perm-item {isChecked
									? 'bg-primary-subtle border border-primary-subtle'
									: 'border border-transparent'}"
								onclick={() => toggle(perm.key)}
							>
								<input
									type="checkbox"
									class="form-check-input mt-1 flex-shrink-0"
									checked={isChecked}
									onclick={(e) => e.stopPropagation()}
									onchange={() => toggle(perm.key)}
								/>
								<div class="flex-grow-1 min-w-0">
									<div class="d-flex align-items-center gap-2 flex-wrap">
										<code class="small fw-semibold" class:text-primary={isChecked}>
											{perm.key}
										</code>
										{#if perm.min_trust && perm.min_trust !== 'new'}
											<span
												class="badge bg-{TRUST_COLOR[perm.min_trust] ?? 'secondary'}-subtle
												       text-{TRUST_COLOR[perm.min_trust] ?? 'secondary'}-emphasis trust-badge"
											>
												min: {perm.min_trust}
											</span>
										{/if}
									</div>
									<div class="text-muted perm-desc">
										{perm.description}
									</div>
								</div>
								{#if isChecked}
									<i class="fa-solid fa-check text-primary mt-1 flex-shrink-0 check-icon"></i>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/each}
</form>

<!-- Floating save bar — appears when there are unsaved changes -->
{#if hasChanges}
	<div class="position-fixed bottom-0 start-0 end-0 py-3 px-4 save-bar">
		<div class="d-flex align-items-center justify-content-between gap-3 save-bar-inner">
			<div class="d-flex align-items-center gap-3">
				<span class="badge bg-warning-subtle text-warning-emphasis">
					{changeCount} unsaved change{changeCount !== 1 ? 's' : ''}
				</span>
				{#if addedCount > 0}
					<span class="small text-success">+{addedCount} added</span>
				{/if}
				{#if removedCount > 0}
					<span class="small text-danger">−{removedCount} removed</span>
				{/if}
			</div>
			<div class="d-flex gap-2">
				<button
					type="button"
					class="btn btn-outline-secondary btn-sm"
					onclick={discardChanges}
				>
					Discard
				</button>
				<button
					type="submit"
					form="permissions-form"
					class="btn btn-primary btn-sm"
					disabled={saving}
					onclick={() => (saving = true)}
				>
					{#if saving}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
					Save Changes
				</button>
			</div>
		</div>
	</div>
	<!-- Spacer so content isn't hidden behind the floating bar -->
	<div class="save-bar-spacer"></div>
{/if}

<style>
	.icon-md  { font-size: 1rem; }
	.icon-sm  { font-size: 0.9rem; }
	.icon-xs  { font-size: 0.65rem; }
	.group-btn { font-size: 0.75rem; }
	.perm-item { min-height: 52px; }
	.trust-badge { font-size: 0.6rem; }
	.perm-desc { font-size: 0.78rem; line-height: 1.3; }
	.check-icon { font-size: 0.8rem; }

	.save-bar {
		background: color-mix(in srgb, var(--bs-body-bg) 95%, transparent);
		backdrop-filter: blur(6px);
		border-top: 1px solid var(--bs-border-color);
		z-index: 1040;
	}

	.save-bar-inner {
		max-width: 900px;
		margin: 0 auto;
	}

	.save-bar-spacer {
		height: 5rem;
	}
</style>
