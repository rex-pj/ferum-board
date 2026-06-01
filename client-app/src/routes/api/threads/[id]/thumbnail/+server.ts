import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const POST: RequestHandler = async ({ params, request, cookies }) => {
	const token = cookies.get('token');
	if (!token) return new Response(null, { status: 401 });

	// Forward the multipart body as-is
	const contentType = request.headers.get('content-type') ?? '';
	const body = await request.arrayBuffer();

	const res = await fetch(`${API}/api/threads/${params.id}/thumbnail`, {
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

export const DELETE: RequestHandler = async ({ params, cookies }) => {
	const token = cookies.get('token');
	if (!token) return new Response(null, { status: 401 });

	const res = await fetch(`${API}/api/threads/${params.id}/thumbnail`, {
		method: 'DELETE',
		headers: { Authorization: `Bearer ${token}` }
	});

	return new Response(null, { status: res.status });
};
