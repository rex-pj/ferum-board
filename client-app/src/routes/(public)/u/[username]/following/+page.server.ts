import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, fetch, url, cookies }) => {
	const page = Number(url.searchParams.get('page') ?? '1');

	const profileRes = await fetch(`${API}/api/users/${params.username}`);
	if (profileRes.status === 404) error(404, 'User not found');
	if (!profileRes.ok) error(500, 'Failed to load profile');

	const profileJson = await profileRes.json();
	const profile = profileJson.data;

	const token = cookies.get('token');
	const authHeaders: Record<string, string> = token ? { Authorization: `Bearer ${token}` } : {};

	const [followingRes, statusRes] = await Promise.all([
		fetch(`${API}/api/users/${profile.id}/following?page=${page}&per_page=20`),
		fetch(`${API}/api/users/${profile.id}/follow-status`, { headers: authHeaders })
	]);

	const followingJson = followingRes.ok
		? await followingRes.json()
		: { data: [], meta: { total: 0, page: 1, per_page: 20 } };

	const followStatus = statusRes.ok
		? ((await statusRes.json()).data ?? { following: false, follower_count: 0, following_count: 0 })
		: { following: false, follower_count: 0, following_count: 0 };

	return {
		profile,
		followStatus,
		following: followingJson.data ?? [],
		total: followingJson.meta?.total ?? 0,
		currentPage: followingJson.meta?.page ?? 1,
		perPage: followingJson.meta?.per_page ?? 20
	};
};
