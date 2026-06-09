<svelte:head>
	<title>Categories | Admin | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { invalidate } from '$app/navigation';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let showCreateForm = $state(false);
	let creating = $state(false);
	let deletingId = $state<string | null>(null);

	const VIEW_POLICIES = ['public', 'members_only', 'staff_only'];
	const POST_POLICIES = ['members', 'trusted', 'staff_only', 'closed', 'moderated'];
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<i class="fa-solid fa-folder-tree fr-page-icon"></i>
	<h1 class="h5 mb-0">Categories</h1>
	<button class="btn btn-primary btn-sm ms-auto" onclick={() => (showCreateForm = !showCreateForm)}>
		<i class="fa-solid fa-plus me-1"></i>New Category
	</button>
</div>

{#if form?.error}
	<div class="alert alert-danger" role="alert">{form.error}</div>
{/if}

{#if showCreateForm}
	<div class="card mb-4">
		<div class="card-header">Create Category</div>
		<div class="card-body">
			<form
				method="POST"
				action="?/create"
				use:enhance={() => {
					creating = true;
					return async ({ result, update }) => {
						creating = false;
						showCreateForm = false;
						if (result.type === 'success') toast.success('Category created.');
						await update();
						if (result.type === 'success') await invalidate('app:categories');
					};
				}}
			>
				<div class="row g-3">
					<div class="col-md-6">
						<label class="form-label" for="name">Name</label>
						<input type="text" id="name" name="name" class="form-control" required maxlength="80" />
					</div>
					<div class="col-md-6">
						<label class="form-label" for="slug">Slug</label>
						<input type="text" id="slug" name="slug" class="form-control" required maxlength="80"
							   pattern="[a-z0-9\-]+" title="lowercase letters, numbers and hyphens only" />
					</div>
					<div class="col-md-12">
						<label class="form-label" for="description">Description</label>
						<input type="text" id="description" name="description" class="form-control" maxlength="255" />
					</div>
					<div class="col-md-4">
						<label class="form-label" for="parent_id">Parent category</label>
						<select id="parent_id" name="parent_id" class="form-select">
							<option value="">None (top-level)</option>
							{#each data.categories.filter((c: any) => !c.parent_id) as cat}
								<option value={cat.id}>{cat.name}</option>
							{/each}
						</select>
					</div>
					<div class="col-md-4">
						<label class="form-label" for="view_policy">View policy</label>
						<select id="view_policy" name="view_policy" class="form-select">
							{#each VIEW_POLICIES as p}
								<option value={p}>{p}</option>
							{/each}
						</select>
					</div>
					<div class="col-md-4">
						<label class="form-label" for="post_policy">Post policy</label>
						<select id="post_policy" name="post_policy" class="form-select">
							{#each POST_POLICIES as p}
								<option value={p}>{p}</option>
							{/each}
						</select>
					</div>
					<div class="col-md-2">
						<label class="form-label" for="position">Position</label>
						<input type="number" id="position" name="position" class="form-control" value="0" min="0" />
					</div>
					<div class="col-md-3">
						<label class="form-label" for="color">Color</label>
						<input type="color" id="color" name="color" class="form-control form-control-color" value="#0d6efd" />
					</div>
				</div>
				<div class="mt-3 d-flex gap-2">
					<button type="submit" class="btn btn-primary btn-sm" disabled={creating}>
						{#if creating}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
						Create
					</button>
					<button type="button" class="btn btn-outline-secondary btn-sm" onclick={() => (showCreateForm = false)}>
						Cancel
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

<div class="card">
<div class="table-responsive">
	<table class="table table-hover align-middle mb-0">
		<thead>
			<tr>
				<th>Name</th>
				<th>Slug</th>
				<th>Parent</th>
				<th>View</th>
				<th>Post</th>
				<th>Pos.</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each data.categories as cat}
				<tr>
					<td>
						<span class="fw-semibold">{cat.name}</span>
						{#if cat.color}
							<span class="badge ms-1" style="background-color:{cat.color};">&nbsp;</span>
						{/if}
					</td>
					<td><code class="small">{cat.slug}</code></td>
					<td class="text-muted small">
						{data.categories.find((c: any) => c.id === cat.parent_id)?.name ?? 'None'}
					</td>
					<td><span class="badge bg-secondary">{cat.view_policy}</span></td>
					<td><span class="badge bg-secondary">{cat.post_policy}</span></td>
					<td>{cat.position}</td>
					<td>
						<div class="d-flex gap-1">
							<a href={ROUTES.ADMIN.CATEGORY(cat.id)} class="btn btn-outline-secondary btn-sm" title="Edit">
								<i class="fa-solid fa-pen"></i>
							</a>
							<a href={ROUTES.ADMIN.CATEGORY_MODS(cat.id)} class="btn btn-outline-secondary btn-sm" title="Moderators">
								<i class="fa-solid fa-shield-halved"></i>
							</a>
							<form method="POST" action="?/delete" use:enhance={() => {
								deletingId = cat.id;
								return async ({ result, update }) => {
									deletingId = null;
									if (result.type === 'success') toast.success('Category deleted.');
									await update();
									if (result.type === 'success') await invalidate('app:categories');
								};
							}}>
								<input type="hidden" name="id" value={cat.id} />
								<button
									type="submit"
									class="btn btn-outline-danger btn-sm"
									title="Delete"
									disabled={deletingId === cat.id}
									onclick={(e) => { if (!confirm('Delete this category?')) e.preventDefault(); }}
								>
									<i class="fa-solid fa-trash"></i>
								</button>
							</form>
						</div>
					</td>
				</tr>
			{/each}
			{#if data.categories.length === 0}
				<tr>
					<td colspan="7" class="p-0">
						<div class="fr-empty-state">
							<div class="fr-empty-icon"><i class="fa-solid fa-folder-tree"></i></div>
							<h2 class="fr-empty-title">No categories yet</h2>
							<p class="fr-empty-sub">Create your first category to get started.</p>
						</div>
					</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
</div>
