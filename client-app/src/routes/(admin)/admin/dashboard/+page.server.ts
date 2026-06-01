import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ cookies, fetch }) => {
	const token = cookies.get('token')!;
	const res = await fetch(`${API}/api/admin/stats`, {
		headers: { Authorization: `Bearer ${token}` }
	});
	const json = res.ok ? await res.json() : { data: null };
	return { stats: json.data };
};
