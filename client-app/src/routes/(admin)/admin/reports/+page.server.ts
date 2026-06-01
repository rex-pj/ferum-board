import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, url }) => {
	const token = cookies.get('token')!;
	const page = url.searchParams.get('page') ?? '1';
	const status = url.searchParams.get('status') ?? '';
	const target_type = url.searchParams.get('target_type') ?? '';

	const params = new URLSearchParams({ page, per_page: '20' });
	if (status) params.set('status', status);
	if (target_type) params.set('target_type', target_type);

	const res = await fetch(`${API}/api/admin/reports?${params}`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = res.ok ? await res.json() : { data: [], meta: { total: 0, page: 1, per_page: 20 } };
	return { reports: json.data ?? [], meta: json.meta, status, target_type };
};

export const actions: Actions = {
	resolve: async ({ request, cookies, fetch }) => {
		const token = cookies.get('token')!;
		const data = await request.formData();
		const id = data.get('id') as string;
		const status = data.get('status') as string;

		const res = await fetch(`${API}/api/mod/reports/${id}`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
			body: JSON.stringify({ status })
		});

		if (!res.ok) {
			const err = await res.json().catch(() => ({}));
			return fail(res.status, { error: err?.error?.message ?? 'Failed to update report.' });
		}
		return { success: `Report ${status}.` };
	}
};
