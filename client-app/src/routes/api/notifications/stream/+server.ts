import type { RequestHandler } from './$types';

const API = process.env.API_URL ?? 'http://localhost:8080';

/**
 * Proxy the Axum SSE stream to the browser.
 * EventSource cannot set custom headers, so we run this server-side to
 * forward the httpOnly auth cookie as a Bearer token to the backend.
 */
export const GET: RequestHandler = async ({ cookies, fetch }) => {
	const token = cookies.get('token');
	if (!token) {
		return new Response(null, { status: 401 });
	}

	const upstream = await fetch(`${API}/api/notifications/stream`, {
		headers: {
			Authorization: `Bearer ${token}`,
			Accept: 'text/event-stream'
		}
	});

	if (!upstream.ok) {
		return new Response(null, { status: upstream.status });
	}

	return new Response(upstream.body, {
		headers: {
			'Content-Type': 'text/event-stream',
			'Cache-Control': 'no-cache',
			Connection: 'keep-alive',
			'X-Accel-Buffering': 'no'
		}
	});
};
