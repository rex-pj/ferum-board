import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const load: PageServerLoad = async ({ params, fetch, url }) => {
	const page = Number(url.searchParams.get('page') ?? '1');

	const [profileRes, threadsRes] = await Promise.all([
		fetch(`${API}/api/users/${params.username}`),
		fetch(`${API}/api/users/${params.username}/threads?page=${page}&per_page=20`),
	]);

	if (profileRes.status === 404) error(404, 'User not found');
	if (!profileRes.ok) error(500, 'Failed to load profile');

	const profileJson = await profileRes.json();
	const threadsJson = threadsRes.ok ? await threadsRes.json() : { data: [], total: 0, page: 1, per_page: 20 };

	return {
		profile: profileJson.data,
		threads: threadsJson.data ?? [],
		threadTotal: threadsJson.total ?? 0,
		threadPage: threadsJson.page ?? 1,
		threadPerPage: threadsJson.per_page ?? 20,
	};
};
