import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, url, cookies, fetch }) => {
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;
	const token = cookies.get('token');

	const headers: Record<string, string> = {};
	if (token) headers['Authorization'] = `Bearer ${token}`;

	// Start the all-categories fetch immediately — it runs in parallel with the
	// category + threads fetches and is only awaited if we need parent breadcrumb data.
	const allCatFetch = fetch(`${API}/api/categories`, { headers });

	const [catRes, threadRes] = await Promise.all([
		fetch(`${API}/api/categories/${params.categorySlug}`, { headers }),
		fetch(`${API}/api/categories/${params.categorySlug}/threads?page=${page}&per_page=20`, { headers })
	]);

	if (!catRes.ok) error(catRes.status === 404 ? 404 : 500, 'Category not found');

	const [catJson, threadJson] = await Promise.all([
		catRes.json(),
		threadRes.ok ? threadRes.json() : { data: [], meta: {} }
	]);

	const cat = catJson.data;

	// Load parent category for breadcrumb if this is a sub-category
	let parentCategory: { name: string; slug: string } | null = cat?.parent ?? null;
	if (cat?.parent_id && !parentCategory) {
		const allCatRes = await allCatFetch;
		if (allCatRes.ok) {
			const allCatJson = await allCatRes.json();
			const parent = (allCatJson.data ?? []).find((c: { id: string; name: string; slug: string }) => c.id === cat.parent_id);
			if (parent) parentCategory = { name: parent.name, slug: parent.slug };
		}
	}

	return {
		category: cat,
		parentCategory,
		threads: threadJson.data ?? [],
		meta: threadJson.meta ?? { total: 0, page, per_page: 20 }
	};
};
