import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const DELETE: RequestHandler = async ({ params, cookies }) => {
	const token = cookies.get('token');
	if (!token) return new Response(null, { status: 401 });

	const res = await fetch(`${API}/api/posts/${params.id}/reactions/${params.kind}`, {
		method: 'DELETE',
		headers: { Authorization: `Bearer ${token}` }
	});

	return new Response(res.body, {
		status: res.status,
		headers: { 'Content-Type': 'application/json' }
	});
};
