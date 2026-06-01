import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const POST: RequestHandler = async ({ request, cookies }) => {
	const token = cookies.get('token');
	if (!token) return new Response(null, { status: 401 });

	const contentType = request.headers.get('content-type') ?? '';
	const body = await request.arrayBuffer();

	const res = await fetch(`${API}/api/users/me/avatar`, {
		method: 'POST',
		headers: {
			'Content-Type': contentType,
			Authorization: `Bearer ${token}`
		},
		body
	});

	const json = await res.json().catch(() => ({}));
	return new Response(JSON.stringify(json), {
		status: res.status,
		headers: { 'Content-Type': 'application/json' }
	});
};

export const DELETE: RequestHandler = async ({ cookies }) => {
	const token = cookies.get('token');
	if (!token) return new Response(null, { status: 401 });

	const res = await fetch(`${API}/api/users/me/avatar`, {
		method: 'DELETE',
		headers: { Authorization: `Bearer ${token}` }
	});

	return new Response(null, { status: res.status });
};
