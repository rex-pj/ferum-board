import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ url, fetch }) => {
	const q = url.searchParams.get('q') ?? '';
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;

	if (!q.trim()) return { q: '', results: null, meta: null };

	const res = await fetch(
		`${API}/api/search?q=${encodeURIComponent(q)}&page=${page}&per_page=20`
	);

	if (!res.ok) return { q, results: [], meta: null };

	const json = await res.json();
	return { q, results: json.data ?? [], meta: json.meta };
};
