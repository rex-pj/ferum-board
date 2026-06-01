<svelte:head>
	<title>Category Moderators | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let assigning = $state(false);
	let revokingId = $state<string | null>(null);
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<a href={ROUTES.ADMIN.CATEGORY(data.categoryId)} class="btn btn-outline-secondary btn-sm" aria-label="Back to category">
		<i class="fa-solid fa-arrow-left"></i>
	</a>
	<h1 class="h4 mb-0">Moderators for {data.categoryName}</h1>
</div>

{#if form?.error}
	<div class="alert alert-danger" role="alert">{form.error}</div>
{/if}

<div class="card mb-4">
	<div class="card-header">Assign moderator</div>
	<div class="card-body">
		<form
			method="POST"
			action="?/assign"
			use:enhance={() => {
				assigning = true;
				return async ({ result, update }) => {
					assigning = false;
					if (result.type === 'success') toast.success('Moderator assigned.');
					await update();
				};
			}}
		>
			<div class="row g-2 align-items-end">
				<div class="col">
					<label class="form-label" for="user_id">User ID (UUID)</label>
					<input
						type="text"
						id="user_id"
						name="user_id"
						class="form-control"
						required
						placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
						pattern="[0-9a-f]{'{'}8{'}'}-[0-9a-f]{'{'}4{'}'}-[0-9a-f]{'{'}4{'}'}-[0-9a-f]{'{'}4{'}'}-[0-9a-f]{'{'}12{'}'}"
					/>
				</div>
				<div class="col-auto">
					<button type="submit" class="btn btn-primary" disabled={assigning}>
						{#if assigning}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
						Assign
					</button>
				</div>
			</div>
		</form>
	</div>
</div>

<div class="table-responsive">
	<table class="table table-hover align-middle">
		<thead>
			<tr>
				<th>User</th>
				<th>Assigned at</th>
				<th>Assigned by</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each data.moderators as mod}
				<tr>
					<td>
						<div class="fw-semibold">{mod.display_name ?? mod.username}</div>
						<code class="small text-muted">{mod.username}</code>
					</td>
					<td class="text-muted small">{new Date(mod.assigned_at).toLocaleString()}</td>
					<td class="text-muted small"><code class="small">{mod.assigned_by.slice(0, 8)}…</code></td>
					<td>
						<form
							method="POST"
							action="?/revoke"
							use:enhance={() => {
								revokingId = mod.user_id;
								return async ({ result, update }) => {
									revokingId = null;
									if (result.type === 'success') toast.success('Moderator revoked.');
									await update();
								};
							}}
						>
							<input type="hidden" name="user_id" value={mod.user_id} />
							<button
								type="submit"
								class="btn btn-outline-danger btn-sm"
								disabled={revokingId === mod.user_id}
								onclick={(e) => { if (!confirm('Revoke moderator?')) e.preventDefault(); }}
							>
								<i class="fa-solid fa-xmark"></i> Revoke
							</button>
						</form>
					</td>
				</tr>
			{/each}
			{#if data.moderators.length === 0}
				<tr>
					<td colspan="4" class="text-center text-muted py-4">No moderators assigned.</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
