import { error, fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, cookies, fetch }) => {
	const token = cookies.get('token')!;

	const [catRes, modsRes] = await Promise.all([
		fetch(`${API}/api/admin/categories`, { headers: { Authorization: `Bearer ${token}` } }),
		fetch(`${API}/api/admin/categories/${params.id}/moderators`, {
			headers: { Authorization: `Bearer ${token}` }
		})
	]);

	if (!modsRes.ok) error(404, 'Category not found');

	const allCats = (await catRes.json()).data ?? [];
	const cat = allCats.find((c: any) => c.id === params.id);

	return {
		categoryId: params.id,
		categoryName: cat?.name ?? params.id,
		moderators: (await modsRes.json()).data ?? []
	};
};

export const actions: Actions = {
	assign: async ({ params, request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const user_id = data.get('user_id') as string;

		if (!user_id) return fail(422, { error: 'User ID is required.' });

		const res = await fetch(`${API}/api/admin/categories/${params.id}/moderators`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ user_id })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			const messages: Record<string, string> = {
				already_assigned: 'This user is already a moderator for this category.',
				not_found: 'User not found.',
				validation_error: 'User must have the moderator or admin role first.'
			};
			return fail(res.status, { error: messages[err?.error?.code] ?? 'Failed to assign.' });
		}

		return { success: 'Moderator assigned.' };
	},

	revoke: async ({ params, request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const user_id = data.get('user_id') as string;

		const res = await fetch(`${API}/api/admin/categories/${params.id}/moderators/${user_id}`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) return fail(res.status, { error: 'Failed to revoke moderator.' });

		return { success: 'Moderator revoked.' };
	}
};
