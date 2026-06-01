import { fail, redirect } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';
import { ROUTES } from '$lib/routes';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, url }) => {
	const token = cookies.get('token');
	if (!token) redirect(302, ROUTES.LOGIN);

	const catRes = await fetch(`${API}/api/categories`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const catJson = catRes.ok ? await catRes.json() : { data: [] };

	return {
		categories: catJson.data ?? [],
		preselectedCategoryId: url.searchParams.get('category') ?? null
	};
};

export const actions: Actions = {
	default: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'You must be signed in.' });

		const data = await request.formData();
		const title = (data.get('title') as string)?.trim();
		const category_id = data.get('category_id') as string;
		const content_md = (data.get('content_md') as string)?.trim();
		const thumbnailFile = data.get('thumbnail') as File | null;

		if (!title) return fail(422, { error: 'Title is required.', title, category_id, content_md });
		if (!category_id) return fail(422, { error: 'Category is required.', title, category_id, content_md });
		if (!content_md) return fail(422, { error: 'Post content is required.', title, category_id, content_md });

		const outgoing = new FormData();
		outgoing.append('category_id', category_id);
		outgoing.append('title', title);
		outgoing.append('content_md', content_md);
		if (thumbnailFile && thumbnailFile.size > 0) {
			outgoing.append('thumbnail', thumbnailFile, thumbnailFile.name);
		}

		const res = await fetch(`${API}/api/threads`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` },
			body: outgoing
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			const msg = err?.error?.message ?? 'Failed to create thread.';
			return fail(res.status, { error: msg, title, category_id, content_md });
		}

		const json = await res.json();
		redirect(302, ROUTES.THREAD(json.data?.thread?.slug));
	}
};
