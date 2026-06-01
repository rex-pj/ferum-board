import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, params }) => {
	const token = cookies.get('token')!;
	const res = await fetch(`${API}/api/admin/users/${params.id}`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = await res.json();
	return { user: json.data };
};

export const actions: Actions = {
	updateRole: async ({ request, cookies, fetch, params }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const body: Record<string, unknown> = {
			role: data.get('role') as string
		};
		const globalMod = data.get('is_global_mod');
		if (globalMod !== null) body.is_global_mod = globalMod === 'true';

		const res = await fetch(`${API}/api/admin/users/${params.id}`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify(body)
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to update user.' });
		}
		return { success: 'User updated.' };
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
