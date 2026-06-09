import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, fetch, url, cookies }) => {
	const page = Number(url.searchParams.get('page') ?? '1');

	const [profileRes, threadsRes] = await Promise.all([
		fetch(`${API}/api/users/${params.username}`),
		fetch(`${API}/api/users/${params.username}/threads?page=${page}&per_page=20`),
	]);

	if (profileRes.status === 404) error(404, 'User not found');
	if (!profileRes.ok) error(500, 'Failed to load profile');

	const profileJson = await profileRes.json();
	const profile = profileJson.data;
	const threadsJson = threadsRes.ok ? await threadsRes.json() : { data: [], meta: { total: 0, page: 1, per_page: 20 } };

	// Load follow status + counts using the profile's UUID
	let followStatus = { following: false, follower_count: 0, following_count: 0 };
	if (profile?.id) {
		const token = cookies.get('token');
		const headers: Record<string, string> = {};
		if (token) headers['Authorization'] = `Bearer ${token}`;
		const statusRes = await fetch(`${API}/api/users/${profile.id}/follow-status`, { headers });
		if (statusRes.ok) {
			const statusJson = await statusRes.json();
			followStatus = statusJson.data ?? followStatus;
		}
	}

	return {
		profile,
		followStatus,
		threads: threadsJson.data ?? [],
		threadTotal: threadsJson.meta?.total ?? threadsJson.total ?? 0,
		threadPage: threadsJson.meta?.page ?? threadsJson.page ?? 1,
		threadPerPage: threadsJson.meta?.per_page ?? threadsJson.per_page ?? 20,
		canonicalUrl: url.href
	};
};
