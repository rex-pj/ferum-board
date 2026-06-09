import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, url }) => {
	const token = cookies.get('token')!;
	const page = url.searchParams.get('page') ?? '1';
	const category_id = url.searchParams.get('category_id') ?? '';

	const params = new URLSearchParams({ page, per_page: '20' });
	if (category_id) params.set('category_id', category_id);

	const res = await fetch(`${API}/api/mod/queue?${params}`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = res.ok ? await res.json() : { data: [], meta: { total: 0, page: 1, per_page: 20 } };
	return { posts: json.data ?? [], meta: json.meta };
};

export const actions: Actions = {
	approve: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;

		const res = await fetch(`${API}/api/mod/queue/${id}/approve`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) return fail(res.status, { error: 'Failed to approve post.' });
		return { success: 'Post approved and published.' };
	},

	reject: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;

		const res = await fetch(`${API}/api/mod/queue/${id}/reject`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) return fail(res.status, { error: 'Failed to reject post.' });
		return { success: 'Post rejected.' };
	}
};
