import { error, fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, cookies, fetch }) => {
	const token = cookies.get('token');
	if (!token) redirect(302, ROUTES.LOGIN);

	const auth = { Authorization: `Bearer ${token}` };

	const [threadRes, catRes] = await Promise.all([
		fetch(`${API}/api/threads/${params.slug}`, { headers: auth }),
		fetch(`${API}/api/categories`, { headers: auth })
	]);

	if (!threadRes.ok) error(threadRes.status === 404 ? 404 : 500, 'Thread not found');

	const thread = (await threadRes.json()).data;
	const categories = catRes.ok ? ((await catRes.json()).data ?? []) : [];

	const postsRes = await fetch(
		`${API}/api/threads/${thread.id}/posts?page=1&per_page=1`,
		{ headers: auth }
	);
	const firstPost = postsRes.ok ? ((await postsRes.json()).data?.[0] ?? null) : null;

	return { thread, firstPost, categories };
};

export const actions: Actions = {
	// Handles title / content / category changes only.
	// Thumbnail upload is done directly browser → backend in the page's enhance callback
	// so it never reaches this action (no double-hop through the SvelteKit server).
	save: async ({ request, cookies, fetch, params }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'You must be signed in.' });

		const data = await request.formData();
		const thread_id        = data.get('thread_id') as string;
		const first_post_id    = data.get('first_post_id') as string;
		const original_title   = data.get('original_title') as string;
		const original_category_id = data.get('original_category_id') as string;
		const original_content = data.get('original_content') as string;

		const title      = (data.get('title') as string)?.trim() ?? '';
		const content_md = (data.get('content_md') as string)?.trim() ?? '';
		const category_id = data.get('category_id') as string;

		const titleChanged    = title !== original_title.trim();
		const contentChanged  = content_md !== original_content.trim();
		const categoryChanged = !!category_id && category_id !== original_category_id;

		if (!titleChanged && !contentChanged && !categoryChanged) {
			return fail(422, { error: 'No changes detected.' });
		}

		if (!title) return fail(422, { error: 'Title is required.' });
		if (title.length < 5 || title.length > 255) {
			return fail(422, { error: 'Title must be 5–255 characters.' });
		}

		const errors: string[] = [];

		// Update title via multipart PATCH (backend handler expects multipart for this endpoint)
		if (titleChanged) {
			const outgoing = new FormData();
			outgoing.append('title', title);

			const res = await fetch(`${API}/api/threads/${thread_id}`, {
				method: 'PATCH',
				headers: { Authorization: `Bearer ${token}` },
				body: outgoing
			});
			if (!res.ok) {
				const err = await res.json().catch(() => ({}));
				errors.push(err?.error?.message ?? 'Failed to update thread.');
			}
		}

		// Update first-post content
		if (contentChanged && first_post_id) {
			const res = await fetch(`${API}/api/posts/${first_post_id}`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
				body: JSON.stringify({ content_md })
			});
			if (!res.ok) {
				const err = await res.json().catch(() => ({}));
				errors.push(err?.error?.message ?? 'Failed to update content.');
			}
		}

		// Move to a different category (moderator action)
		if (categoryChanged) {
			const res = await fetch(`${API}/api/threads/${thread_id}/move`, {
				method: 'PATCH',
				headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
				body: JSON.stringify({ category_id })
			});
			if (!res.ok) {
				const err = await res.json().catch(() => ({}));
				errors.push(err?.error?.message ?? 'Cannot move thread to that category.');
			}
		}

		if (errors.length > 0) return fail(422, { error: errors.join(' ') });

		redirect(302, ROUTES.THREAD(params.slug));
	},

	removeThumbnail: async ({ request, cookies, fetch, params }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Unauthorized' });

		const data = await request.formData();
		const thread_id = data.get('thread_id') as string;
		if (!thread_id) return fail(422, { error: 'Thread ID is required.' });

		await fetch(`${API}/api/threads/${thread_id}/thumbnail`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		redirect(302, ROUTES.EDIT_THREAD(params.slug));
	}
};
