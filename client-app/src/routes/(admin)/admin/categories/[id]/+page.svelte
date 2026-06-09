<svelte:head>
	<title>Edit Category | Admin | {data.siteName ?? 'Ferum Board'}</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { invalidate } from '$app/navigation';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';

	let { data, form }: { data: any; form: any } = $props();
	let saving = $state(false);

	const VIEW_POLICIES = ['public', 'members_only', 'staff_only'];
	const POST_POLICIES = ['members', 'trusted', 'staff_only', 'closed', 'moderated'];

	const cat = data.category;
</script>

<div class="d-flex align-items-center gap-2 mb-4">
	<a href={ROUTES.ADMIN.CATEGORIES} class="btn btn-outline-secondary btn-sm" aria-label="Back to categories">
		<i class="fa-solid fa-arrow-left"></i>
	</a>
	<h1 class="h4 mb-0">Edit: {cat.name}</h1>
</div>

{#if form?.error}
	<div class="alert alert-danger" role="alert">{form.error}</div>
{/if}

<div class="card">
	<div class="card-body">
		<form
			method="POST"
			use:enhance={() => {
				saving = true;
				return async ({ result, update }) => {
					saving = false;
					if (result.type === 'success') toast.success('Category saved.');
					await update();
					if (result.type === 'success') await invalidate('app:categories');
				};
			}}
		>
			<div class="row g-3">
				<div class="col-md-6">
					<label class="form-label" for="name">Name</label>
					<input type="text" id="name" name="name" class="form-control" required maxlength="80" value={cat.name} />
				</div>
				<div class="col-md-6">
					<label class="form-label" for="slug">Slug</label>
					<input type="text" id="slug" name="slug" class="form-control" required maxlength="80"
						   pattern="[a-z0-9\-]+" value={cat.slug} />
				</div>
				<div class="col-12">
					<label class="form-label" for="description">Description</label>
					<input type="text" id="description" name="description" class="form-control" value={cat.description ?? ''} />
				</div>
				<div class="col-md-4">
					<label class="form-label" for="parent_id">Parent category</label>
					<select id="parent_id" name="parent_id" class="form-select">
						<option value="">None (top-level)</option>
						{#each data.allCategories.filter((c: any) => !c.parent_id && c.id !== cat.id) as c}
							<option value={c.id} selected={c.id === cat.parent_id}>{c.name}</option>
						{/each}
					</select>
				</div>
				<div class="col-md-4">
					<label class="form-label" for="view_policy">View policy</label>
					<select id="view_policy" name="view_policy" class="form-select">
						{#each VIEW_POLICIES as p}
							<option value={p} selected={p === cat.view_policy}>{p}</option>
						{/each}
					</select>
				</div>
				<div class="col-md-4">
					<label class="form-label" for="post_policy">Post policy</label>
					<select id="post_policy" name="post_policy" class="form-select">
						{#each POST_POLICIES as p}
							<option value={p} selected={p === cat.post_policy}>{p}</option>
						{/each}
					</select>
				</div>
				<div class="col-md-2">
					<label class="form-label" for="position">Position</label>
					<input type="number" id="position" name="position" class="form-control" value={cat.position} min="0" />
				</div>
				<div class="col-md-3">
					<label class="form-label" for="color">Color</label>
					<input type="color" id="color" name="color" class="form-control form-control-color"
						   value={cat.color ?? '#0d6efd'} />
				</div>
			</div>
			<div class="mt-4 d-flex gap-2">
				<button type="submit" class="btn btn-primary" disabled={saving}>
					{#if saving}<span class="spinner-border spinner-border-sm me-1"></span>{/if}
					Save changes
				</button>
				<a href={ROUTES.ADMIN.CATEGORY_MODS(cat.id)} class="btn btn-outline-secondary">
					<i class="fa-solid fa-shield-halved me-1"></i>Moderators
				</a>
				<a href={ROUTES.ADMIN.CATEGORIES} class="btn btn-outline-secondary">Cancel</a>
			</div>
		</form>
	</div>
</div>
