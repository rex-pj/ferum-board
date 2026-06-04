<svelte:head>
	<title>Roles | Admin | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();

	let showCreate = $state(false);
	let creating = $state(false);
	let deletingId = $state<string | null>(null);

	$effect(() => {
		if (form?.success) toast.success(form.success);
		if (form?.error) toast.error(form.error);
	});
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-shield-halved fr-page-icon"></i>
	<h1 class="h5 mb-0">Roles</h1>
	<button class="btn btn-primary btn-sm ms-auto" onclick={() => (showCreate = !showCreate)}>
		<i class="fa-solid fa-plus me-1"></i>New Role
	</button>
</div>

{#if showCreate}
	<div class="card mb-4">
		<div class="card-header py-2">Create Custom Role</div>
		<div class="card-body">
			<form
				method="POST"
				action="?/create"
				use:enhance={() => {
					creating = true;
					return async ({ update }) => {
						await update();
						creating = false;
						showCreate = false;
					};
				}}
			>
				<div class="row g-3">
					<div class="col-sm-6 col-md-3">
						<label class="form-label" for="slug">Slug <span class="text-danger">*</span></label>
						<input
							id="slug"
							name="slug"
							class="form-control form-control-sm"
							placeholder="vip-member"
							pattern="[a-z0-9\-]+"
							title="Lowercase letters, numbers, and hyphens only"
							required
						/>
					</div>
					<div class="col-sm-6 col-md-3">
						<label class="form-label" for="name">Display Name <span class="text-danger">*</span></label>
						<input
							id="name"
							name="name"
							class="form-control form-control-sm"
							placeholder="VIP Member"
							required
						/>
					</div>
					<div class="col-sm-4 col-md-2">
						<label class="form-label" for="color">Badge Color</label>
						<input
							id="color"
							name="color"
							type="color"
							class="form-control form-control-color form-control-sm w-100"
							value="#0d6efd"
						/>
					</div>
					<div class="col-sm-4 col-md-2">
						<label class="form-label" for="position">Position</label>
						<input
							id="position"
							name="position"
							type="number"
							class="form-control form-control-sm"
							value="100"
							min="0"
						/>
					</div>
					<div class="col-12">
						<label class="form-label" for="description">Description</label>
						<input
							id="description"
							name="description"
							class="form-control form-control-sm"
							placeholder="Optional description"
						/>
					</div>
				</div>
				<div class="d-flex gap-2 mt-3">
					<button type="submit" class="btn btn-primary btn-sm" disabled={creating}>
						{#if creating}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
						Create Role
					</button>
					<button
						type="button"
						class="btn btn-outline-secondary btn-sm"
						onclick={() => (showCreate = false)}
					>
						Cancel
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

<div class="card">
	<div class="table-responsive">
		<table class="table align-middle mb-0">
			<thead>
				<tr>
					<th>Role</th>
					<th>Slug</th>
					<th>Description</th>
					<th>Type</th>
					<th class="text-end">Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each data.roles as role (role.id)}
					<tr>
						<td>
							<span
								class="badge"
								style={role.color ? `background:${role.color};` : 'background:#6c757d;'}
							>
								{role.name}
							</span>
						</td>
						<td><code class="small">{role.slug}</code></td>
						<td class="text-muted small">{role.description ?? 'No description'}</td>
						<td>
							{#if role.is_system}
								<span class="badge bg-secondary-subtle text-secondary-emphasis">
									<i class="fa-solid fa-lock me-1 icon-xs"></i>system
								</span>
							{:else}
								<span class="badge bg-success-subtle text-success-emphasis">custom</span>
							{/if}
						</td>
						<td class="text-end">
							<div class="d-flex gap-1 justify-content-end">
								<a
									href={ROUTES.ADMIN.ROLE(role.id)}
									class="btn btn-sm btn-outline-secondary"
									title="Edit permissions"
								>
									<i class="fa-solid fa-key me-1"></i>Permissions
								</a>
								{#if !role.is_system}
									<form
										method="POST"
										action="?/delete"
										class="d-inline"
										use:enhance={() => {
											deletingId = role.id;
											return async ({ update }) => {
												await update();
												deletingId = null;
											};
										}}
									>
										<input type="hidden" name="id" value={role.id} />
										<button
											type="submit"
											class="btn btn-sm btn-outline-danger"
											disabled={deletingId === role.id}
											title="Delete role"
											onclick={(e) => {
												if (!confirm(`Delete role "${role.name}"?`)) e.preventDefault();
											}}
										>
											{#if deletingId === role.id}
												<span class="spinner-border spinner-border-sm"></span>
											{:else}
												<i class="fa-solid fa-trash"></i>
											{/if}
										</button>
									</form>
								{/if}
							</div>
						</td>
					</tr>
				{/each}
				{#if (data.roles ?? []).length === 0}
					<tr>
						<td colspan="5" class="p-0">
							<div class="fr-empty-state">
								<div class="fr-empty-icon"><i class="fa-solid fa-shield-halved"></i></div>
								<h2 class="fr-empty-title">No roles found</h2>
							</div>
						</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>
</div>

<style>
	.icon-xs { font-size: 0.6rem; }
</style>