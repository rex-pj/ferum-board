<svelte:head>
	<title>New Thread | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import PostComposer from '$lib/components/organisms/PostComposer.svelte';
	import ThumbnailPicker from '$lib/components/molecules/ThumbnailPicker.svelte';
	import { ROUTES } from '$lib/routes';

	let { data, form }: { data: any; form: any } = $props();

	let title = $state((form?.title as string) ?? '');
	let content = $state((form?.content_md as string) ?? '');
	let selectedCategory = $state((form?.category_id as string) ?? (data.preselectedCategoryId ?? ''));
	let submitting = $state(false);

	const topLevelCategories = $derived((data.categories ?? []).filter((c: any) => !c.parent_id));
	const subCategories = $derived((data.categories ?? []).filter((c: any) => !!c.parent_id));
</script>

<div class="fr-page-narrow">
	<nav aria-label="breadcrumb" class="mb-3">
		<ol class="breadcrumb">
			<li class="breadcrumb-item"><a href={ROUTES.HOME}>Home</a></li>
			<li class="breadcrumb-item active">New Thread</li>
		</ol>
	</nav>

	<h1 class="h4 mb-4">Start a New Thread</h1>

	{#if form?.error}
		<div class="alert alert-danger">{form.error}</div>
	{/if}

	<form
		method="POST"
		enctype="multipart/form-data"
		use:enhance={() => {
			submitting = true;
			return async ({ update }) => {
				submitting = false;
				await update();
			};
		}}
	>
		<div class="mb-4">
			<label for="thread-title" class="form-label fw-semibold">Title</label>
			<input
				id="thread-title"
				name="title"
				type="text"
				class="form-control"
				placeholder="What's your thread about?"
				bind:value={title}
				maxlength="255"
				required
			/>
		</div>

		<div class="mb-4">
			<label for="category_id" class="form-label fw-semibold">Category</label>
			<select
				id="category_id"
				name="category_id"
				class="form-select"
				bind:value={selectedCategory}
				required
			>
				<option value="">— Select a category —</option>
				{#each topLevelCategories as cat}
					<option value={cat.id}>{cat.name}</option>
					{#each subCategories.filter((s: any) => s.parent_id === cat.id) as sub}
						<option value={sub.id}>&nbsp;&nbsp;&nbsp;{sub.name}</option>
					{/each}
				{/each}
			</select>
		</div>

		<div class="mb-4">
			<div class="fw-semibold mb-2">
				Thumbnail
				<span class="text-secondary fw-normal ms-1 small">(optional)</span>
			</div>
			<ThumbnailPicker name="thumbnail" />
		</div>

		<div class="mb-4">
			<div class="fw-semibold mb-2">Content</div>
			<PostComposer bind:value={content} name="content_md" />
		</div>

		<div class="d-flex gap-2">
			<button
				type="submit"
				class="btn btn-primary"
				disabled={submitting || !title.trim() || !selectedCategory || !content.trim()}
			>
				{#if submitting}
					<span class="spinner-border spinner-border-sm me-1"></span>
				{/if}
				Post Thread
			</button>
			<a href={ROUTES.HOME} class="btn btn-outline-secondary" style="min-height:44px;">Cancel</a>
		</div>
	</form>
</div>
