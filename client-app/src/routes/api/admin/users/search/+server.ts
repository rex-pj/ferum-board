import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const GET: RequestHandler = async ({ url, cookies }) => {
	const token = cookies.get('token');
	if (!token) return new Response(null, { status: 401 });

	const q = url.searchParams.get('q') ?? '';
	const perPage = url.searchParams.get('per_page') ?? '10';

	const res = await fetch(
		`${API}/api/admin/users?q=${encodeURIComponent(q)}&per_page=${perPage}&page=1`,
		{ headers: { Authorization: `Bearer ${token}` } }
	);

	const json = await res.json().catch(() => ({}));
	return new Response(JSON.stringify(json), {
		status: res.status,
		headers: { 'Content-Type': 'application/json' }
	});
};
