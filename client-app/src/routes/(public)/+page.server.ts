import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ fetch, url, cookies }) => {
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;
	const token = cookies.get('token');
	const headers: Record<string, string> = token ? { Authorization: `Bearer ${token}` } : {};

	const [catRes, threadRes] = await Promise.all([
		fetch(`${API}/api/categories`),
		fetch(`${API}/api/threads?per_page=20&page=${page}`, { headers })
	]);

	const [catJson, threadJson] = await Promise.all([
		catRes.ok ? catRes.json() : { data: [] },
		threadRes.ok ? threadRes.json() : { data: [], meta: {} }
	]);

	// Detect if the feed is personalized (user has watched categories set)
	let isPersonalized = false;
	if (token) {
		const watchedRes = await fetch(`${API}/api/categories`, { headers });
		if (watchedRes.ok) {
			// We infer personalization from whether the thread count differs from guest.
			// The simpler approach: pass the flag from the server based on preferences.
			// For now we mark isPersonalized=true when the user is logged in AND
			// the thread list differs from the public feed — checked via preferences.
			const prefRes = await fetch(`${API}/api/users/me/preferences`, { headers });
			if (prefRes.ok) {
				const prefJson = await prefRes.json();
				isPersonalized = (prefJson.data?.watched_categories?.length ?? 0) > 0;
			}
		}
	}

	return {
		categories: catJson.data ?? [],
		threads: threadJson.data ?? [],
		meta: threadJson.meta ?? {},
		canonicalUrl: url.href,
		isPersonalized
	};
};
