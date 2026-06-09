import { error, fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, url, cookies, fetch }) => {
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;
	const token = cookies.get('token');

	const headers: Record<string, string> = {};
	if (token) headers['Authorization'] = `Bearer ${token}`;

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

	let parentCategory: { name: string; slug: string } | null = cat?.parent ?? null;
	if (cat?.parent_id && !parentCategory) {
		const allCatRes = await allCatFetch;
		if (allCatRes.ok) {
			const allCatJson = await allCatRes.json();
			const parent = (allCatJson.data ?? []).find(
				(c: { id: string; name: string; slug: string }) => c.id === cat.parent_id
			);
			if (parent) parentCategory = { name: parent.name, slug: parent.slug };
		}
	}

	// Load watch/mute status for logged-in users
	let watched = false;
	let muted = false;
	if (token && cat?.id) {
		const statusRes = await fetch(`${API}/api/categories/${cat.id}/watch-status`, { headers });
		if (statusRes.ok) {
			const statusJson = await statusRes.json();
			watched = statusJson.data?.watched ?? false;
			muted = statusJson.data?.muted ?? false;
		}
	}

	return {
		category: cat,
		parentCategory,
		threads: threadJson.data ?? [],
		meta: threadJson.meta ?? { total: 0, page, per_page: 20 },
		canonicalUrl: url.href,
		watched,
		muted
	};
};

export const actions: Actions = {
	watch: async ({ params, cookies, fetch, request }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Sign in to watch categories.' });
		const data = await request.formData();
		const category_id = data.get('category_id') as string;
		await fetch(`${API}/api/categories/${category_id}/watch`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` }
		});
		return { watched: true };
	},

	unwatch: async ({ params, cookies, fetch, request }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Sign in to watch categories.' });
		const data = await request.formData();
		const category_id = data.get('category_id') as string;
		await fetch(`${API}/api/categories/${category_id}/watch`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		return { watched: false };
	},

	mute: async ({ params, cookies, fetch, request }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Sign in to mute categories.' });
		const data = await request.formData();
		const category_id = data.get('category_id') as string;
		await fetch(`${API}/api/categories/${category_id}/mute`, {
			method: 'POST',
			headers: { Authorization: `Bearer ${token}` }
		});
		return { muted: true };
	},

	unmute: async ({ params, cookies, fetch, request }) => {
		const token = cookies.get('token');
		if (!token) return fail(401, { error: 'Sign in to mute categories.' });
		const data = await request.formData();
		const category_id = data.get('category_id') as string;
		await fetch(`${API}/api/categories/${category_id}/mute`, {
			method: 'DELETE',
			headers: { Authorization: `Bearer ${token}` }
		});
		return { muted: false };
	}
};
