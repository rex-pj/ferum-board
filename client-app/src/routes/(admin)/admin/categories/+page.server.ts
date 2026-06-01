import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;
	const res = await fetch(`${API}/api/admin/categories`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = await res.json();
	return { categories: json.data ?? [] };
};

export const actions: Actions = {
	create: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const body = {
			name: data.get('name') as string,
			slug: data.get('slug') as string,
			description: (data.get('description') as string) || null,
			parent_id: (data.get('parent_id') as string) || null,
			position: parseInt(data.get('position') as string) || 0,
			view_policy: data.get('view_policy') as string,
			post_policy: data.get('post_policy') as string,
			color: (data.get('color') as string) || null
		};

		const res = await fetch(`${API}/api/admin/categories`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify(body)
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to create category.' });
		}

		return { success: 'Category created.' };
	},

	delete: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;

		const res = await fetch(`${API}/api/admin/categories/${id}`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) {
			return fail(res.status, { error: 'Failed to delete category.' });
		}

		return { success: 'Category deleted.' };
	}
};
