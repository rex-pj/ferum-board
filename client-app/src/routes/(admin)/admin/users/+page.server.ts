import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, url }) => {
	const token = cookies.get('token')!;
	const page = url.searchParams.get('page') ?? '1';
	const q = url.searchParams.get('q') ?? '';

	const params = new URLSearchParams({ page, per_page: '20' });
	if (q) params.set('q', q);

	const res = await fetch(`${API}/api/admin/users?${params}`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = res.ok ? await res.json() : { data: [], meta: { total: 0, page: 1, per_page: 20 } };
	return { users: json.data ?? [], meta: json.meta, q };
};
