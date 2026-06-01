import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ url, cookies, fetch }) => {
	const token = cookies.get('token');
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;
	const fallback = { bookmarks: [], meta: { total: 0, page, per_page: 20 } };

	if (!token) return fallback;

	try {
		const res = await fetch(`${API}/api/users/me/bookmarks?page=${page}&per_page=20`, {
			headers: { Authorization: `Bearer ${token}` }
		});
		const json = res.ok ? await res.json() : { data: [], meta: fallback.meta };
		return {
			bookmarks: json.data ?? [],
			meta: json.meta ?? fallback.meta
		};
	} catch {
		return fallback;
	}
};
