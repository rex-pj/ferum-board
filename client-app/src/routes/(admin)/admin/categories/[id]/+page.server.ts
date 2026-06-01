import { error, fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, cookies, fetch }) => {
	const token = cookies.get('token')!;

	const allRes = await fetch(`${API}/api/admin/categories`, {
		headers: { Authorization: `Bearer ${token}` }
	});

	const all = (await allRes.json()).data ?? [];
	const category = all.find((c: any) => c.id === params.id);

	if (!category) error(404, 'Category not found');

	return { category, allCategories: all };
};

export const actions: Actions = {
	default: async ({ params, request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();

		const body: Record<string, unknown> = {
			name: data.get('name') as string,
			slug: data.get('slug') as string,
			description: (data.get('description') as string) || null,
			parent_id: (data.get('parent_id') as string) || null,
			position: parseInt(data.get('position') as string) || 0,
			view_policy: data.get('view_policy') as string,
			post_policy: data.get('post_policy') as string,
			color: (data.get('color') as string) || null
		};

		const res = await fetch(`${API}/api/admin/categories/${params.id}`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify(body)
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to save.' });
		}

		return { success: 'Changes saved.' };
	}
};
