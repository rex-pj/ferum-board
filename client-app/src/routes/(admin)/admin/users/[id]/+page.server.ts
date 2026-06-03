import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, params }) => {
	const token = cookies.get('token')!;
	const [userRes, rolesRes] = await Promise.all([
		fetch(`${API}/api/admin/users/${params.id}`, {
			headers: { Authorization: `Bearer ${token}` }
		}),
		fetch(`${API}/api/admin/roles`, {
			headers: { Authorization: `Bearer ${token}` }
		})
	]);
	const userJson = await userRes.json();
	const rolesJson = rolesRes.ok ? await rolesRes.json() : { data: [] };
	return { user: userJson.data, allRoles: rolesJson.data ?? [] };
};

export const actions: Actions = {
	assignRole: async ({ request, cookies, fetch, params }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const role_id = data.get('role_id') as string;
		const category_id = (data.get('category_id') as string) || null;

		if (!role_id) return fail(400, { error: 'role_id is required.' });

		const res = await fetch(`${API}/api/admin/users/${params.id}/roles`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ role_id, category_id: category_id ?? undefined })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to assign role.' });
		}
		return { success: 'Role assigned.' };
	},

	revokeRole: async ({ request, cookies, fetch, params }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const role_id = data.get('role_id') as string;

		if (!role_id) return fail(400, { error: 'role_id is required.' });

		const res = await fetch(`${API}/api/admin/users/${params.id}/roles/${role_id}`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to revoke role.' });
		}
		return { success: 'Role revoked.' };
	},

	ban: async ({ request, cookies, fetch, params }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const reason = data.get('reason') as string;

		const res = await fetch(`${API}/api/admin/users/${params.id}/ban`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ reason })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to ban user.' });
		}
		return { success: 'User banned.' };
	},

	unban: async ({ cookies, fetch, params }) => {
		const token = cookies.get('token')!;
		const res = await fetch(`${API}/api/admin/users/${params.id}/ban`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});

		if (!res.ok) {
			return fail(res.status, { error: 'Failed to unban user.' });
		}
		return { success: 'User unbanned.' };
	}
};
