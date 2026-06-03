import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, cookies, fetch }) => {
	const token = cookies.get('token')!;
	const [rolePermsRes, allPermsRes, rolesRes] = await Promise.all([
		fetch(`${API}/api/admin/roles/${params.id}/permissions`, {
			headers: { Authorization: `Bearer ${token}` }
		}),
		fetch(`${API}/api/admin/roles/permissions`, {
			headers: { Authorization: `Bearer ${token}` }
		}),
		fetch(`${API}/api/admin/roles`, { headers: { Authorization: `Bearer ${token}` } })
	]);

	const rolePermsJson = rolePermsRes.ok ? await rolePermsRes.json() : { data: [] };
	const allPermsJson = allPermsRes.ok ? await allPermsRes.json() : { data: [] };
	const rolesJson = rolesRes.ok ? await rolesRes.json() : { data: [] };

	const role = (rolesJson.data ?? []).find((r: any) => r.id === params.id);

	return {
		role,
		roleId: params.id,
		assignedKeys: (rolePermsJson.data ?? []).map((p: any) => p.key) as string[],
		allPermissions: allPermsJson.data ?? []
	};
};

export const actions: Actions = {
	save: async ({ request, params, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const keys = data.getAll('perm') as string[];

		const res = await fetch(`${API}/api/admin/roles/${params.id}/permissions`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ permission_keys: keys })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to save permissions.' });
		}
		return { success: 'Permissions saved.' };
	}
};
