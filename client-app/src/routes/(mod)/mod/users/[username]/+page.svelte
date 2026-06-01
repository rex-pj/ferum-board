<svelte:head>
	<title>{data.user?.username ?? 'User'} | Mod | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import Avatar from '$lib/components/atoms/Avatar.svelte';
	import Badge from '$lib/components/atoms/Badge.svelte';
	import Timestamp from '$lib/components/atoms/Timestamp.svelte';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	const u = $derived(data.user);

	let warning = $state(false);
	let banning = $state(false);

	// Minimum datetime for ban expiry: 1 hour from now, formatted for datetime-local input
	const minUntil = $derived(() => {
		const d = new Date(Date.now() + 60 * 60 * 1000);
		return d.toISOString().slice(0, 16);
	});

	$effect(() => {
		if (form?.warnSuccess) toast.success('Warning issued.');
		if (form?.banSuccess) toast.success('Temporary ban applied.');
	});
</script>

<div class="mb-3">
	<a href="/mod/reports" class="btn btn-outline-secondary btn-sm">
		<i class="fa-solid fa-arrow-left me-1"></i>Back to Reports
	</a>
</div>

{#if u}
	<div class="row g-4">
		<!-- Profile card -->
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
					</div>
					<div class="text-muted small">
						<div>Posts: {u.post_count ?? 0}</div>
						<div class="mt-1">Joined: <Timestamp date={u.created_at} /></div>
					</div>
					<div class="mt-3">
						<a href={ROUTES.USER_PROFILE(u.username)} class="btn btn-sm btn-outline-secondary w-100" target="_blank">
							<i class="fa-solid fa-arrow-up-right-from-square me-1"></i>View public profile
						</a>
					</div>
				</div>
			</div>
		</div>

		<!-- Mod actions -->
		<div class="col-md-8 d-flex flex-column gap-3">
			<!-- Warn -->
			<div class="card">
				<div class="card-header">
					<i class="fa-solid fa-triangle-exclamation text-warning me-2"></i>Issue Warning
				</div>
				<div class="card-body">
					{#if form?.warnError}
						<div class="alert alert-danger py-2">{form.warnError}</div>
					{/if}
					<form
						method="POST"
						action="?/warn"
						use:enhance={() => {
							warning = true;
							return async ({ result, update }) => {
								warning = false;
								await update({ reset: true });
							};
						}}
					>
						<input type="hidden" name="id" value={u.id} />
						<div class="mb-3">
							<label for="warnReason" class="form-label">Reason <span class="text-danger">*</span></label>
							<textarea
								id="warnReason"
								name="reason"
								class="form-control"
								rows="3"
								maxlength="500"
								placeholder="Explain the rule violation…"
								required
							></textarea>
						</div>
						<button type="submit" class="btn btn-warning btn-sm" disabled={warning}>
							{#if warning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
							<i class="fa-solid fa-triangle-exclamation me-1"></i>Issue Warning
						</button>
					</form>
				</div>
			</div>

			<!-- Temp ban -->
			<div class="card">
				<div class="card-header">
					<i class="fa-solid fa-ban text-danger me-2"></i>Temporary Ban
				</div>
				<div class="card-body">
					{#if u.is_banned}
						<div class="alert alert-warning py-2">
							<i class="fa-solid fa-circle-info me-1"></i>
							This user is already banned.
							{#if u.banned_until}
								Ban expires: <Timestamp date={u.banned_until} />.
							{:else}
								(permanent)
							{/if}
						</div>
					{/if}
					{#if form?.banError}
						<div class="alert alert-danger py-2">{form.banError}</div>
					{/if}
					<form
						method="POST"
						action="?/tempBan"
						use:enhance={() => {
							banning = true;
							return async ({ result, update }) => {
								banning = false;
								await update({ reset: true });
							};
						}}
					>
						<input type="hidden" name="id" value={u.id} />
						<div class="mb-3">
							<label for="banReason" class="form-label">Reason <span class="text-danger">*</span></label>
							<textarea
								id="banReason"
								name="reason"
								class="form-control"
								rows="3"
								maxlength="500"
								placeholder="Explain the ban reason…"
								required
							></textarea>
						</div>
						<div class="mb-3">
							<label for="banUntil" class="form-label">Ban until (UTC) <span class="text-danger">*</span></label>
							<input
								type="datetime-local"
								id="banUntil"
								name="until"
								class="form-control"
								min={minUntil()}
								required
							/>
							<div class="form-text">Time is interpreted as UTC.</div>
						</div>
						<button type="submit" class="btn btn-danger btn-sm" disabled={banning}>
							{#if banning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
							<i class="fa-solid fa-ban me-1"></i>Apply Temporary Ban
						</button>
					</form>
				</div>
			</div>
		</div>
	</div>
{:else}
	<div class="alert alert-warning">User not found.</div>
{/if}
