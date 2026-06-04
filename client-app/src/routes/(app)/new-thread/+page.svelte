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

	// Tag chip input (max 5 tags)
	let tags = $state<string[]>([]);
	let tagInput = $state('');

	function addTag() {
		const name = tagInput.trim();
		if (name && tags.length < 5 && !tags.includes(name)) {
			tags = [...tags, name];
		}
		tagInput = '';
	}

	function removeTag(tag: string) {
		tags = tags.filter((t) => t !== tag);
	}

	function onTagKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ',') {
			e.preventDefault();
			addTag();
		} else if (e.key === 'Backspace' && tagInput === '' && tags.length > 0) {
			tags = tags.slice(0, -1);
		}
	}

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
				<option value="">Select a category</option>
				{#each topLevelCategories as cat}
					<option value={cat.id}>{cat.name}</option>
					{#each subCategories.filter((s: any) => s.parent_id === cat.id) as sub}
						<option value={sub.id}>&nbsp;&nbsp;&nbsp;{sub.name}</option>
					{/each}
				{/each}
			</select>
		</div>

		<div class="mb-4">
			<label class="form-label fw-semibold">
				Tags
				<span class="text-secondary fw-normal ms-1 small">(optional, max 5)</span>
			</label>
			<div class="tag-input-wrapper border rounded p-2 d-flex flex-wrap gap-1 align-items-center">
				{#each tags as tag}
					<span class="badge bg-secondary d-inline-flex align-items-center gap-1">
						{tag}
						<button
							type="button"
							class="btn-close btn-close-white"
							style="font-size:0.55rem;"
							aria-label="Remove tag {tag}"
							onclick={() => removeTag(tag)}
						></button>
					</span>
					<input type="hidden" name="tags" value={tag} />
				{/each}
				{#if tags.length < 5}
					<input
						type="text"
						class="border-0 outline-0 flex-grow-1"
						style="min-width:120px;outline:none;"
						placeholder={tags.length === 0 ? 'Add tags…' : ''}
						bind:value={tagInput}
						onkeydown={onTagKeydown}
						onblur={addTag}
					/>
				{/if}
			</div>
			<div class="form-text">Press Enter or comma to add a tag.</div>
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
