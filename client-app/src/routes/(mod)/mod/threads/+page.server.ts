import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, url }) => {
	const token = cookies.get('token')!;
	const page = url.searchParams.get('page') ?? '1';
	const category_slug = url.searchParams.get('category_slug') ?? '';

	const params = new URLSearchParams({ page, per_page: '30' });

	// Filter by category uses /api/categories/{slug}/threads; plain feed uses /api/threads.
	// ThreadListQuery only accepts page + per_page — status filter doesn't exist on the backend.
	const threadsUrl = category_slug
		? `${API}/api/categories/${category_slug}/threads?${params}`
		: `${API}/api/threads?${params}`;

	const [threadsRes, categoriesRes] = await Promise.all([
		fetch(threadsUrl, { headers: { Authorization: `Bearer ${token}` } }),
		fetch(`${API}/api/categories`, { headers: { Authorization: `Bearer ${token}` } })
	]);

	const threadsJson = threadsRes.ok
		? await threadsRes.json()
		: { data: [], meta: { total: 0, page: 1, per_page: 30 } };
	const categoriesJson = categoriesRes.ok ? await categoriesRes.json() : { data: [] };

	return {
		threads: threadsJson.data ?? [],
		meta: threadsJson.meta,
		categories: categoriesJson.data ?? [],
		category_slug
	};
};

export const actions: Actions = {
	pin: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const is_pinned = data.get('is_pinned') === 'true';

		const res = await fetch(`${API}/api/threads/${id}/pin`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ pinned: !is_pinned })
		});

		if (!res.ok) return fail(res.status, { error: 'Failed to update pin status.' });
		return { success: true };
	},

	lock: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const current_status = data.get('status') as string;

		const res = await fetch(`${API}/api/threads/${id}/lock`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ locked: current_status !== 'locked' })
		});

		if (!res.ok) return fail(res.status, { error: 'Failed to update lock status.' });
		return { success: true };
	},

	move: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const category_id = data.get('category_id') as string;

		if (!category_id) return fail(400, { error: 'Category is required.' });

		const res = await fetch(`${API}/api/threads/${id}/move`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ category_id })
		});

		if (!res.ok) return fail(res.status, { error: 'Failed to move thread.' });
		return { success: true };
	}
};
