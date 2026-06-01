import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ fetch, url }) => {
	const page = parseInt(url.searchParams.get('page') ?? '1') || 1;

	const [catRes, threadRes] = await Promise.all([
		fetch(`${API}/api/categories`),
		fetch(`${API}/api/threads?per_page=20&page=${page}`)
	]);

	const [catJson, threadJson] = await Promise.all([
		catRes.ok ? catRes.json() : { data: [] },
		threadRes.ok ? threadRes.json() : { data: [], meta: {} }
	]);

	return {
		categories: catJson.data ?? [],
		threads: threadJson.data ?? [],
		meta: threadJson.meta ?? {}
	};
};
