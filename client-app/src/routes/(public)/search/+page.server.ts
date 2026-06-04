import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ url, fetch }) => {
	const q = url.searchParams.get('q') ?? '';
	const tag = url.searchParams.get('tag') ?? '';
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;

	// Tag filter: use the thread feed API
	if (tag.trim()) {
		const res = await fetch(
			`${API}/api/threads?tag=${encodeURIComponent(tag)}&page=${page}&per_page=20`
		);
		if (!res.ok) {
			console.error(`[search] tag feed error ${res.status} for tag="${tag}":`, await res.text());
			return { q, tag, results: [], meta: null, apiError: true };
		}
		const json = await res.json();
		return { q, tag, results: json.data ?? [], meta: json.meta, apiError: false };
	}

	if (!q.trim()) return { q, tag: '', results: null, meta: null, apiError: false };

	const res = await fetch(
		`${API}/api/search?q=${encodeURIComponent(q)}&page=${page}&per_page=20`
	);

	if (!res.ok) {
		console.error(`[search] search error ${res.status} for q="${q}":`, await res.text());
		return { q, tag: '', results: [], meta: null, apiError: true };
	}

	const json = await res.json();
	return { q, tag: '', results: json.data ?? [], meta: json.meta, apiError: false };
};
