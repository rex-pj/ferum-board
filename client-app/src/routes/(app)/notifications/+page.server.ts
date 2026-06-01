import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token');
	if (!token) return { notifications: [] };
	try {
		const res = await fetch(`${API}/api/notifications?per_page=30`, {
			headers: { Authorization: `Bearer ${token}` }
		});
		const json = res.ok ? await res.json() : { data: [] };
		return { notifications: json.data ?? [] };
	} catch {
		return { notifications: [] };
	}
};

export const actions: Actions = {
	markRead: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		await fetch(`${API}/api/notifications/${id}/read`, {
			method: 'PATCH',
			headers: { Authorization: `Bearer ${token}` }
		});
		return {};
	},

	markAllRead: async ({ cookies, fetch }) => {
		const token = cookies.get('token')!;
		await fetch(`${API}/api/notifications/read-all`, {
			method: 'PATCH',
			headers: { Authorization: `Bearer ${token}` }
		});
		return {};
	}
};
