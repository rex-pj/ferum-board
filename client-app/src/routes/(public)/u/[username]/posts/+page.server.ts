import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, fetch, url, cookies }) => {
	const page = Number(url.searchParams.get('page') ?? '1');
	const token = cookies.get('token');
	const authHeaders: Record<string, string> = token ? { Authorization: `Bearer ${token}` } : {};

	const profileRes = await fetch(`${API}/api/users/${params.username}`);
	if (profileRes.status === 404) error(404, 'User not found');
	const profile = (await profileRes.json()).data;

	const [postsRes, statusRes] = await Promise.all([
		fetch(`${API}/api/users/${profile.username}/posts?page=${page}&per_page=20`),
		fetch(`${API}/api/users/${profile.id}/follow-status`, { headers: authHeaders })
	]);

	const postsJson = postsRes.ok ? await postsRes.json() : { data: [], meta: {} };
	const followStatus = statusRes.ok ? (await statusRes.json()).data : null;

	return {
		profile,
		followStatus,
		posts: postsJson.data ?? [],
		total: postsJson.meta?.total ?? 0,
		currentPage: postsJson.meta?.page ?? page,
		perPage: postsJson.meta?.per_page ?? 20
	};
};
