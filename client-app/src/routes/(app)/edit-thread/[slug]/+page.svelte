<svelte:head>
	<title>Edit Thread | Ferum Board</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<script lang="ts">
	import { enhance } from '$app/forms';
	import { goto } from '$app/navigation';
	import PostComposer from '$lib/components/organisms/PostComposer.svelte';
	import ThumbnailPicker from '$lib/components/molecules/ThumbnailPicker.svelte';
	import { ROUTES } from '$lib/routes';
	import { toast } from '$lib/stores/toast';
	import { isModerator } from '$lib/utils/permissions';

	let { data, form }: { data: any; form: any } = $props();

	const thread = $derived(data.thread);
	const firstPost = $derived(data.firstPost);
	const isMod = $derived(isModerator(data.user));
	const topLevel = $derived((data.categories ?? []).filter((c: any) => !c.parent_id));
	const subCats = $derived((data.categories ?? []).filter((c: any) => !!c.parent_id));
	const currentCategory = $derived(
		(data.categories ?? []).find((c: any) => c.id === thread?.category_id)
	);

	let title = $state(thread?.title ?? '');
	let content = $state(firstPost?.content_md ?? '');
	let submitting = $state(false);
	let removingThumbnail = $state(false);
</script>

<div class="fr-page-narrow">
	<nav aria-label="breadcrumb" class="mb-3">
		<ol class="breadcrumb">
			<li class="breadcrumb-item"><a href={ROUTES.HOME}>Home</a></li>
			<li class="breadcrumb-item">
				<a href={ROUTES.THREAD(thread?.slug)}>{thread?.title ?? 'Thread'}</a>
			</li>
			<li class="breadcrumb-item active">Edit</li>
		</ol>
	</nav>

	<h1 class="h4 mb-4">Edit Thread</h1>

	{#if form?.error}
		<div class="alert alert-danger">{form.error}</div>
	{/if}

	<form
		method="POST"
		enctype="multipart/form-data"
		use:enhance={async ({ formData, cancel }) => {
			submitting = true;

			// Pull the file out before it reaches the SvelteKit action.
			// File inputs are always present in multipart even when empty (size 0).
			const rawFile = formData.get('thumbnail');
			formData.delete('thumbnail');
			const pickedFile = rawFile instanceof File && rawFile.size > 0 ? rawFile : null;

			// Detect metadata changes from the hidden original-value fields.
			const newTitle    = (formData.get('title') as string)?.trim() ?? '';
			const origTitle   = (formData.get('original_title') as string)?.trim() ?? '';
			const newContent  = (formData.get('content_md') as string)?.trim() ?? '';
			const origContent = (formData.get('original_content') as string)?.trim() ?? '';
			const newCat      = formData.get('category_id') as string;
			const origCat     = formData.get('original_category_id') as string;
			const metaChanged = newTitle !== origTitle
				|| newContent !== origContent
				|| (!!newCat && newCat !== origCat);

			if (!pickedFile && !metaChanged) {
				cancel();
				submitting = false;
				return;
			}

			// Upload thumbnail directly: browser → backend via Vite proxy.
			// Never goes through the SvelteKit server action — no double-hop.
			let thumbnailFailed = false;
			if (pickedFile) {
				const fd = new FormData();
				fd.append('file', pickedFile);
				try {
					const res = await fetch(`/api/threads/${thread?.id}/thumbnail`, {
						method: 'POST',
						body: fd
					});
					if (!res.ok) {
						const body = await res.json().catch(() => ({}));
						toast.error(body?.error?.message ?? 'Failed to upload thumbnail.');
						thumbnailFailed = true;
					} else {
						toast.success('Thumbnail uploaded.');
					}
				} catch {
					toast.error('Network error uploading thumbnail.');
					thumbnailFailed = true;
				}
			}

			// Thumbnail was the only change — redirect without a server-action round-trip.
			if (!metaChanged) {
				cancel();
				submitting = false;
				if (!thumbnailFailed) goto(ROUTES.THREAD(thread?.slug));
				return;
			}

			// Title / content / category changes go to the server action (text only, no file).
			return async ({ update }) => {
				submitting = false;
				await update();
			};
		}}
	>
		<!-- Hidden fields used for server-side change detection -->
		<input type="hidden" name="thread_id" value={thread?.id} />
		<input type="hidden" name="first_post_id" value={firstPost?.id} />
		<input type="hidden" name="original_title" value={thread?.title ?? ''} />
		<input type="hidden" name="original_category_id" value={thread?.category_id ?? ''} />
		<input type="hidden" name="original_content" value={firstPost?.content_md ?? ''} />

		<!-- Title -->
		<div class="mb-4">
			<label for="thread-title" class="form-label fw-semibold">Title</label>
			<input
				id="thread-title"
				name="title"
				type="text"
				class="form-control"
				bind:value={title}
				maxlength="255"
				minlength="5"
				required
			/>
		</div>

		<!-- Category -->
		<div class="mb-4">
			<div class="fw-semibold mb-1">Category</div>
			{#if isMod}
				<select name="category_id" class="form-select">
					{#each topLevel as cat}
						<option value={cat.id} selected={cat.id === thread?.category_id}>{cat.name}</option>
						{#each subCats.filter((s: any) => s.parent_id === cat.id) as sub}
							<option value={sub.id} selected={sub.id === thread?.category_id}>
								&nbsp;&nbsp;&nbsp;{sub.name}
							</option>
						{/each}
					{/each}
				</select>
				<div class="form-text">Moving a thread is a moderator action.</div>
			{:else}
				<input type="hidden" name="category_id" value={thread?.category_id ?? ''} />
				<div class="form-control-plaintext text-body-secondary">
					<i class="fa-solid fa-folder me-1 opacity-50"></i>
					{currentCategory?.name ?? thread?.category_slug ?? '—'}
				</div>
			{/if}
		</div>

		<!-- Thumbnail -->
		<div class="mb-4">
			<div class="fw-semibold mb-2">Thumbnail</div>
			<ThumbnailPicker name="thumbnail" currentUrl={thread?.thumbnail_url} />

			{#if thread?.thumbnail_url}
				<div class="mt-2">
					<form
						method="POST"
						action="?/removeThumbnail"
						use:enhance={() => {
							removingThumbnail = true;
							return async ({ result, update }) => {
								removingThumbnail = false;
								if (result.type === 'failure') {
									toast.error((result.data as any)?.error ?? 'Failed to remove thumbnail.');
								} else {
									toast.success('Thumbnail removed.');
								}
								await update();
							};
						}}
					>
						<input type="hidden" name="thread_id" value={thread?.id} />
						<button
							type="submit"
							class="btn btn-outline-danger btn-sm"
							disabled={removingThumbnail}
						>
							{#if removingThumbnail}
								<span class="spinner-border spinner-border-sm me-1"></span>
							{:else}
								<i class="fa-solid fa-trash me-1"></i>
							{/if}
							Remove thumbnail
						</button>
					</form>
				</div>
			{/if}
		</div>

		<!-- Content -->
		<div class="mb-4">
			<div class="fw-semibold mb-2">Content</div>
			<PostComposer bind:value={content} name="content_md" placeholder="Edit the opening post…" />
		</div>

		<div class="d-flex gap-2">
			<button
				type="submit"
				class="btn btn-primary"
				disabled={submitting || !title.trim()}
			>
				{#if submitting}
					<span class="spinner-border spinner-border-sm me-1"></span>
				{/if}
				Save Changes
			</button>
			<a href={ROUTES.THREAD(thread?.slug)} class="btn btn-outline-secondary" style="min-height:44px;">
				Cancel
			</a>
		</div>
	</form>
</div>
