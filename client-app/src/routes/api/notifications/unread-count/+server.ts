import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

export const GET: RequestHandler = async ({ cookies, fetch }) => {
	const token = cookies.get('token');
	if (!token) {
		return new Response(JSON.stringify({ data: { count: 0 } }), {
			headers: { 'Content-Type': 'application/json' }
		});
	}

	const res = await fetch(`${API}/api/notifications/unread-count`, {
		headers: { Authorization: `Bearer ${token}` }
	});

	if (!res.ok) {
		return new Response(JSON.stringify({ data: { count: 0 } }), {
			headers: { 'Content-Type': 'application/json' }
		});
	}

	return new Response(res.body, {
		headers: { 'Content-Type': 'application/json' }
	});
};
