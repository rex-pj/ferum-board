import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;
	const rolesRes = await fetch(`${API}/api/admin/roles`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const rolesJson = rolesRes.ok ? await rolesRes.json() : { data: [] };
	return { roles: rolesJson.data ?? [] };
};

export const actions: Actions = {
	create: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const body = {
			slug: data.get('slug') as string,
			name: data.get('name') as string,
			description: (data.get('description') as string) || null,
			color: (data.get('color') as string) || null,
			position: parseInt(data.get('position') as string) || 100
		};
		const res = await fetch(`${API}/api/admin/roles`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify(body)
		});
		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to create role.' });
		}
		return { success: 'Role created.' };
	},

	delete: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const res = await fetch(`${API}/api/admin/roles/${id}`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Cannot delete system roles.' });
		}
		return { success: 'Role deleted.' };
	}
};
