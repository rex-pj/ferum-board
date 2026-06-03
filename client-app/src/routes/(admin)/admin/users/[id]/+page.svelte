<svelte:head>
	<title>{data.user?.username ?? 'User'} | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	const u = $derived(data.user);

	let assigning = $state(false);
	let banning = $state(false);
	let showBanForm = $state(false);

	$effect(() => {
		if (form?.success) toast.success(form.success);
		if (form?.error) toast.error(form.error);
	});

	// Roles the user does NOT yet have globally (to show in assign dropdown)
	const assignableRoles = $derived(
		(data.allRoles ?? []).filter((r: any) => {
			return !(u?.roles ?? []).some(
				(a: any) => a.role.id === r.id && !a.category_id
			);
		})
	);
</script>

<div class="mb-3">
	<a href={ROUTES.ADMIN.USERS} class="btn btn-outline-secondary btn-sm">
		<i class="fa-solid fa-arrow-left me-1"></i>Back to Users
	</a>
</div>

{#if u}
	<div class="row g-4">
		<!-- Left: profile card -->
		<div class="col-md-4">
			<div class="card">
				<div class="card-body text-center">
					<div class="mb-3">
						<Avatar src={u.avatar_url} username={u.username} size={80} />
					</div>
					<h2 class="h5 mb-0">{u.username}</h2>
					{#if u.display_name}
						<div class="text-muted small mb-2">{u.display_name}</div>
					{/if}
					<div class="d-flex justify-content-center gap-2 flex-wrap mb-3">
						{#if u.primary_role_slug && u.primary_role_slug !== 'member'}
							<Badge role={u.primary_role_slug} />
						{/if}
						<Badge trust={u.trust_level} />
						{#if u.is_banned}
							<span class="badge bg-danger">banned</span>
						{/if}
					</div>
					<div class="text-muted small">
						<div>Posts: {u.post_count}</div>
						<div class="mt-1">Joined: <Timestamp date={u.created_at} /></div>
					</div>
				</div>
			</div>
		</div>

		<div class="col-md-8 d-flex flex-column gap-3">
			<!-- Role management card -->
			<div class="card">
				<div class="card-header d-flex align-items-center justify-content-between">
					<span>Roles</span>
					<a href={ROUTES.ADMIN.ROLES} class="btn btn-link btn-sm p-0 text-decoration-none">
						Manage roles <i class="fa-solid fa-arrow-up-right-from-square ms-1 small"></i>
					</a>
				</div>
				<div class="card-body d-flex flex-column gap-3">
					<!-- Current assignments -->
					{#if (u.roles ?? []).length > 0}
						<div>
							<div class="small text-muted mb-2 fw-semibold text-uppercase section-label">Current assignments</div>
							<div class="d-flex flex-column gap-1">
								{#each u.roles as assignment}
									<div class="d-flex align-items-center justify-content-between gap-2 border rounded px-3 py-2">
										<div class="d-flex align-items-center gap-2">
											<span
												class="badge"
												style={assignment.role.color
													? `background:${assignment.role.color};`
													: 'background:#6c757d;'}
											>{assignment.role.name}</span>
											{#if assignment.category_id}
												<span class="text-muted small">category-scoped</span>
											{:else}
												<span class="text-muted small">global</span>
											{/if}
											{#if assignment.expires_at}
												<span class="text-muted small">
													until <Timestamp date={assignment.expires_at} />
												</span>
											{/if}
										</div>
										<form
											method="POST"
											action="?/revokeRole"
											use:enhance={() => {
												return async ({ update }) => {
													await update();
												};
											}}
										>
											<input type="hidden" name="role_id" value={assignment.role.id} />
											<button
												type="submit"
												class="btn btn-outline-danger btn-sm"
												title="Revoke role"
												disabled={assignment.role.is_system && assignment.role.slug === 'member'}
											>
												<i class="fa-solid fa-xmark"></i>
											</button>
										</form>
									</div>
								{/each}
							</div>
						</div>
					{/if}

					<!-- Assign new role -->
					{#if assignableRoles.length > 0}
						<form
							method="POST"
							action="?/assignRole"
							use:enhance={() => {
								assigning = true;
								return async ({ update }) => {
									await update();
									assigning = false;
								};
							}}
						>
							<div class="small text-muted mb-2 fw-semibold text-uppercase section-label">Assign role</div>
							<div class="d-flex gap-2 align-items-end flex-wrap">
								<div class="flex-grow-1 role-select-wrap">
									<select name="role_id" class="form-select form-select-sm" required>
										<option value="">Select role…</option>
										{#each assignableRoles as role}
											<option value={role.id}>{role.name}</option>
										{/each}
									</select>
								</div>
								<button type="submit" class="btn btn-primary btn-sm" disabled={assigning}>
									{#if assigning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
									Assign
								</button>
							</div>
						</form>
					{/if}
				</div>
			</div>

			<!-- Ban card -->
			<div class="card">
				<div class="card-header d-flex justify-content-between align-items-center">
					<span>Ban Status</span>
					{#if u.is_banned}
						<span class="badge bg-danger">Currently banned</span>
					{:else}
						<span class="badge bg-success">Active</span>
					{/if}
				</div>
				<div class="card-body">
					{#if u.is_banned}
						<p class="text-muted small mb-3">
							{u.ban_reason ? `Reason: ${u.ban_reason}` : 'No reason recorded.'}
						</p>
						<form
							method="POST"
							action="?/unban"
							use:enhance={() => {
								banning = true;
								return async ({ result, update }) => {
									banning = false;
									if (result.type === 'success') toast.success('User unbanned.');
									await update({ reset: false });
								};
							}}
						>
							<button type="submit" class="btn btn-success btn-sm" disabled={banning}>
								{#if banning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
								Unban User
							</button>
						</form>
					{:else}
						{#if !showBanForm}
							<button class="btn btn-danger btn-sm" onclick={() => (showBanForm = true)}>
								<i class="fa-solid fa-ban me-1"></i>Ban User
							</button>
						{:else}
							<form
								method="POST"
								action="?/ban"
								use:enhance={() => {
									banning = true;
									return async ({ result, update }) => {
										banning = false;
										showBanForm = false;
										if (result.type === 'success') toast.success('User banned.');
										await update({ reset: false });
									};
								}}
							>
								<div class="mb-2">
									<label class="form-label" for="ban_reason">Reason</label>
									<input
										type="text"
										id="ban_reason"
										name="reason"
										class="form-control"
										required
										maxlength="500"
										placeholder="Reason for ban…"
									/>
								</div>
								<div class="d-flex gap-2">
									<button type="submit" class="btn btn-danger btn-sm" disabled={banning}>
										{#if banning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
										Confirm Ban
									</button>
									<button
										type="button"
										class="btn btn-outline-secondary btn-sm"
										onclick={() => (showBanForm = false)}
									>
										Cancel
									</button>
								</div>
							</form>
						{/if}
					{/if}
				</div>
			</div>
		</div>
	</div>
{:else}
	<div class="alert alert-warning">User not found.</div>
{/if}

<style>
	.section-label { font-size: 0.7rem; }
	.role-select-wrap { min-width: 160px; }
</style>
