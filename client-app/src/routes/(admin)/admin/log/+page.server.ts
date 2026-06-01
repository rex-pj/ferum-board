import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch, url }) => {
	const token = cookies.get('token')!;
	const page = url.searchParams.get('page') ?? '1';
	const actor_id = url.searchParams.get('actor_id') ?? '';
	const target_type = url.searchParams.get('target_type') ?? '';

	const params = new URLSearchParams({ page, per_page: '50' });
	if (actor_id) params.set('actor_id', actor_id);
	if (target_type) params.set('target_type', target_type);

	const res = await fetch(`${API}/api/admin/audit-log?${params}`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = res.ok ? await res.json() : { data: [], meta: { total: 0, page: 1, per_page: 50 } };
	return { logs: json.data ?? [], meta: json.meta, actor_id, target_type };
};
