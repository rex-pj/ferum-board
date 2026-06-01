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

	let updatingRole = $state(false);
	let banning = $state(false);
	let showBanForm = $state(false);

	const ROLES = ['member', 'moderator', 'admin'];
</script>

<div class="mb-3">
	<a href={ROUTES.ADMIN.USERS} class="btn btn-outline-secondary btn-sm">
		<i class="fa-solid fa-arrow-left me-1"></i>Back to Users
	</a>
</div>

{#if form?.error}
	<div class="alert alert-danger" role="alert">{form.error}</div>
{/if}

{#if u}
	<div class="row g-4">
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
						<Badge role={u.role} />
						<Badge trust={u.trust_level} />
						{#if u.is_banned}
							<span class="badge bg-danger">banned</span>
						{/if}
						{#if u.is_global_mod}
							<span class="badge bg-info text-dark">global mod</span>
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
			<div class="card">
				<div class="card-header">Edit Role</div>
				<div class="card-body">
					<form
						method="POST"
						action="?/updateRole"
						use:enhance={() => {
							updatingRole = true;
							return async ({ result, update }) => {
								updatingRole = false;
								if (result.type === 'success') toast.success('Role updated.');
								await update({ reset: false });
							};
						}}
					>
						<div class="row g-2 align-items-end">
							<div class="col">
								<label class="form-label" for="role">Role</label>
								<select id="role" name="role" class="form-select">
									{#each ROLES as r}
										<option value={r} selected={u.role === r}>{r}</option>
									{/each}
								</select>
							</div>
							<div class="col-auto">
								<div class="form-check mt-4">
									<input
										class="form-check-input"
										type="checkbox"
										id="is_global_mod"
										name="is_global_mod"
										value="true"
										checked={u.is_global_mod}
									/>
									<label class="form-check-label" for="is_global_mod">Global moderator</label>
								</div>
							</div>
							<div class="col-auto">
								<button type="submit" class="btn btn-primary btn-sm" disabled={updatingRole}>
									{#if updatingRole}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
									Save
								</button>
							</div>
						</div>
					</form>
				</div>
			</div>

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
									<button type="button" class="btn btn-outline-secondary btn-sm" onclick={() => (showBanForm = false)}>
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
